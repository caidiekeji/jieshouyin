//! 报表与对账（§14）。所有金额整数运算；日结按 business_date 聚合，幂等可重跑。

use crate::error::Result;
use crate::money::cents_to_yuan;
use rusqlite::{params, Connection};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct DailySummary {
    pub business_date: String,
    pub order_count: i64,
    pub total_sales_cents: i64,
    pub refund_cents: i64,
    pub discount_cents: i64,
}

/// 日结即席查询 + 快照（幂等 upsert）。
pub fn generate_daily_summary(
    conn: &Connection,
    business_date: &str,
    now: &str,
) -> Result<DailySummary> {
    let row: (i64, i64, i64, i64) = conn.query_row(
        "SELECT COUNT(DISTINCT o.order_no),
                COALESCE(SUM(o.pay_cents),0),
                COALESCE((SELECT SUM(r.amount_cents) FROM refund_record r
                          JOIN sales_order o2 ON o2.order_no=r.order_no
                          WHERE o2.business_date=?1 AND r.status='COMPLETED'),0),
                COALESCE(SUM(o.discount_cents),0)
         FROM sales_order o
         WHERE o.status IN ('COMPLETED','PARTIAL_REFUND','REFUNDED') AND o.business_date=?1",
        [business_date],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;
    let s = DailySummary {
        business_date: business_date.into(),
        order_count: row.0,
        // 净销售口径：订单实收 - 已完成退款；否则退货越多"销售额"越高。
        total_sales_cents: row.1 - row.2,
        refund_cents: row.2,
        discount_cents: row.3,
    };
    conn.execute(
        "INSERT INTO daily_summary(business_date,order_count,total_sales_cents,refund_cents,discount_cents,generated_at)
         VALUES (?1,?2,?3,?4,?5,?6)
         ON CONFLICT(business_date) DO UPDATE SET
           order_count=excluded.order_count, total_sales_cents=excluded.total_sales_cents,
           refund_cents=excluded.refund_cents, discount_cents=excluded.discount_cents,
           generated_at=excluded.generated_at",
        params![s.business_date, s.order_count, s.total_sales_cents, s.refund_cents, s.discount_cents, now],
    )?;
    Ok(s)
}

#[derive(Debug, Clone, Serialize)]
pub struct ChannelTotal {
    pub channel: String,
    pub amount_cents: i64,
    pub count: i64,
}

/// 支付方式分布（§14.2）。现金 amount_cents 记净额。
pub fn channel_distribution(conn: &Connection, business_date: &str) -> Result<Vec<ChannelTotal>> {
    let mut stmt = conn.prepare(
        "SELECT p.pay_channel, COALESCE(SUM(p.amount_cents),0), COUNT(*)
         FROM payment_record p JOIN sales_order o ON o.order_no=p.order_no
         WHERE p.status='SUCCESS' AND o.business_date=?1
         GROUP BY p.pay_channel ORDER BY p.pay_channel",
    )?;
    let rows = stmt.query_map([business_date], |r| {
        Ok(ChannelTotal {
            channel: r.get(0)?,
            amount_cents: r.get(1)?,
            count: r.get(2)?,
        })
    })?;
    let mut v = Vec::new();
    for row in rows {
        v.push(row?);
    }
    // 渠道净额 = 成功收款 - 已完成退款（退款按原支付渠道冲减）
    let mut refunds = std::collections::HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT p.pay_channel, COALESCE(SUM(r.amount_cents),0)
             FROM refund_record r
             JOIN payment_record p ON p.pay_id=r.pay_id
             JOIN sales_order o ON o.order_no=r.order_no
             WHERE r.status='COMPLETED' AND o.business_date=?1
             GROUP BY p.pay_channel",
        )?;
        let rows = stmt.query_map([business_date], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (ch, amt) = row?;
            refunds.insert(ch, amt);
        }
    }
    for t in &mut v {
        if let Some(r) = refunds.get(&t.channel) {
            t.amount_cents -= r;
        }
    }
    Ok(v)
}

/// 不变式：Σ渠道净额 == Σ成功订单 pay_cents。
pub fn verify_channel_sum(conn: &Connection, business_date: &str) -> Result<bool> {
    let channel_sum: i64 = conn.query_row(
        "SELECT COALESCE(SUM(p.amount_cents),0) FROM payment_record p
         JOIN sales_order o ON o.order_no=p.order_no
         WHERE p.status='SUCCESS' AND o.business_date=?1",
        [business_date],
        |r| r.get(0),
    )?;
    // 实收不变式：渠道成功流水合计 == 所有已收款订单的 pay_cents（含已退款订单，
    // 退款是资金流出，不改写原收款金额）。
    let order_sum: i64 = conn.query_row(
        "SELECT COALESCE(SUM(pay_cents),0) FROM sales_order
         WHERE status IN ('COMPLETED','PARTIAL_REFUND','REFUNDED') AND business_date=?1",
        [business_date],
        |r| r.get(0),
    )?;
    Ok(channel_sum == order_sum)
}

#[derive(Debug, Clone, Serialize)]
pub struct ProductSales {
    pub sku_id: i64,
    pub qty: i64,
    pub sales_cents: i64,
    pub discount_cents: i64,
}

/// 按商品维度销售报表（§14.3）。
pub fn sales_by_product(conn: &Connection, from: &str, to: &str) -> Result<Vec<ProductSales>> {
    let mut stmt = conn.prepare(
        "SELECT i.sku_id, SUM(i.qty - i.returned_qty),
                SUM(i.line_total_cents - i.discount_cents -
                    COALESCE((SELECT SUM(ri.refund_cents) FROM sales_return_item ri
                              JOIN refund_record rr ON rr.return_no=ri.return_no
                              WHERE ri.order_item_id=i.item_id AND rr.status='COMPLETED'),0)),
                SUM(i.discount_cents)
         FROM sales_order_item i JOIN sales_order o ON o.order_no=i.order_no
         WHERE o.status IN ('COMPLETED','PARTIAL_REFUND') AND o.business_date BETWEEN ?1 AND ?2
         GROUP BY i.sku_id ORDER BY i.sku_id",
    )?;
    let rows = stmt.query_map(params![from, to], |r| {
        Ok(ProductSales {
            sku_id: r.get(0)?,
            qty: r.get(1)?,
            sales_cents: r.get(2)?,
            discount_cents: r.get(3)?,
        })
    })?;
    let mut v = Vec::new();
    for row in rows {
        v.push(row?);
    }
    Ok(v)
}

/// 对账差异导出 CSV（§14.4）。金额用带符号整数格式化，禁止浮点。
pub fn export_reconcile_csv(conn: &Connection, date: &str) -> Result<String> {
    let mut stmt = conn.prepare(
        "SELECT diff_type,recon_date,channel,COALESCE(order_no,''),local_amount_cents,channel_amount_cents
         FROM reconcile_record WHERE recon_date=?1 ORDER BY recon_id",
    )?;
    let rows = stmt.query_map([date], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, i64>(4)?,
            r.get::<_, i64>(5)?,
        ))
    })?;
    let mut out = String::from(
        "差异类型,日期,渠道,订单号,本地金额(元),渠道金额(元),差异金额(元),交易号,处理状态\n",
    );
    for row in rows {
        let (diff, d, ch, order, local, remote) = row?;
        let diff_amount = local - remote;
        out.push_str(&format!(
            "{diff},{d},{ch},{order},{},{},{},,\n",
            cents_to_yuan(local),
            cents_to_yuan(remote),
            cents_to_yuan(diff_amount)
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{open_in_memory, run_migrations};

    fn setup() -> Connection {
        let mut c = open_in_memory().unwrap();
        run_migrations(&mut c).unwrap();
        c.execute(
            "INSERT INTO sales_order(order_no,device_id,cashier_id,shift_id,total_cents,discount_cents,pay_cents,status,business_date,created_at)
             VALUES ('O1','D',1,1,10000,1000,9000,'COMPLETED','2026-11-17','2026-11-17T00:00:00Z')",
            [],
        ).unwrap();
        c.execute(
            "INSERT INTO payment_record(order_no,pay_channel,amount_cents,status,created_at)
             VALUES ('O1','CASH',9000,'SUCCESS','2026-11-17T00:00:00Z')",
            [],
        )
        .unwrap();
        c
    }

    #[test]
    fn daily_summary_is_idempotent() {
        let c = setup();
        let s1 = generate_daily_summary(&c, "2026-11-17", "t1").unwrap();
        let s2 = generate_daily_summary(&c, "2026-11-17", "t2").unwrap();
        assert_eq!(s1.order_count, 1);
        assert_eq!(s1.total_sales_cents, 9000);
        assert_eq!(s1.discount_cents, 1000);
        assert_eq!(s1.total_sales_cents, s2.total_sales_cents);
        let n: i64 = c
            .query_row("SELECT COUNT(*) FROM daily_summary", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn channel_sum_matches_order_sum() {
        let c = setup();
        assert!(verify_channel_sum(&c, "2026-11-17").unwrap());
        assert_eq!(
            channel_distribution(&c, "2026-11-17").unwrap()[0].amount_cents,
            9000
        );
    }

    #[test]
    fn reconcile_csv_negative_amount() {
        let c = setup();
        c.execute(
            "INSERT INTO reconcile_record(recon_date,channel,local_amount_cents,channel_amount_cents,diff_type,created_at)
             VALUES ('2026-11-17','WECHAT',100,150,'AMOUNT_MISMATCH','2026-11-17T23:59:00Z')",
            [],
        ).unwrap();
        let csv = export_reconcile_csv(&c, "2026-11-17").unwrap();
        assert!(csv.contains("1.00"));
        assert!(csv.contains("1.50"));
        assert!(csv.contains("-0.50"), "负数金额必须带符号: {csv}");
    }
}
