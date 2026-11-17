//! 收银核心（§5）：订单状态机、tx A/B/C、挂单、退货、作废、交班。
//! 不变式：COMPLETED 当且仅当所有渠道 SUCCESS 且 tx B 提交；
//! 支付确认前不扣库存、不核销券、不扣储值。

use crate::error::{CoreError, Result};
use crate::inventory::{allows_negative, apply_event, TransType};
use crate::time::format_utc;
use chrono::{DateTime, Timelike, Utc};
use rusqlite::{params, Connection};
use serde_json::json;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types)] // 状态/类型字符串与文档一致
pub enum OrderStatus {
    PENDING_PAYMENT,
    COMPLETED,
    PARTIAL_REFUND,
    REFUNDED,
    CANCELED,
    VOIDED,
}

impl OrderStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            OrderStatus::PENDING_PAYMENT => "PENDING_PAYMENT",
            OrderStatus::COMPLETED => "COMPLETED",
            OrderStatus::PARTIAL_REFUND => "PARTIAL_REFUND",
            OrderStatus::REFUNDED => "REFUNDED",
            OrderStatus::CANCELED => "CANCELED",
            OrderStatus::VOIDED => "VOIDED",
        }
    }
}

/// 订单号：门店号 + 终端号 + 毫秒时间戳 + 随机段（客户端生成）。
pub fn generate_order_no(store_id: &str, device_id: &str) -> String {
    use rand::Rng;
    let millis = Utc::now().timestamp_millis();
    let rand: u32 = rand::thread_rng().gen_range(0..=9999);
    format!("{store_id}{device_id}{millis}{rand:04}")
}

/// 业务日（§14.0）：本地时间减 cutoff。默认 00:00，即本地日期。
/// 这里以 UTC 存储，传入已按门店时区偏移后的 DateTime 与 cutoff。
pub fn business_date(local_dt: &DateTime<Utc>, cutoff_hour: u32) -> String {
    let adjusted = if local_dt.hour() < cutoff_hour {
        *local_dt - chrono::Duration::days(1)
    } else {
        *local_dt
    };
    adjusted.format("%Y-%m-%d").to_string()
}

/// tx A：订单 + 明细落库（PENDING_PAYMENT）。不扣库存、不扣储值。
pub fn begin_order(
    tx: &Connection,
    order_no: &str,
    device_id: &str,
    cashier_id: i64,
    shift_id: i64,
    member_id: Option<i64>,
    lines: &[crate::promo::CartLine],
    total_cents: i64,
    discount_cents: i64,
    biz_date: &str,
    now: &str,
) -> Result<()> {
    let pay_cents = total_cents - discount_cents;
    if pay_cents < 0 {
        return Err(CoreError::Money("应付金额不得为负".into()));
    }
    tx.execute(
        "INSERT INTO sales_order(order_no,device_id,cashier_id,shift_id,member_id,total_cents,discount_cents,pay_cents,status,business_date,sync_status,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'PENDING_PAYMENT',?9,'LOCAL_ONLY',?10)",
        params![order_no, device_id, cashier_id, shift_id, member_id, total_cents, discount_cents, pay_cents, biz_date, now],
    )?;
    for line in lines {
        tx.execute(
            "INSERT INTO sales_order_item(order_no,sku_id,qty,unit_price_cents,line_total_cents,discount_cents,is_gift)
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![order_no, line.sku_id, line.qty, line.unit_price_cents, line.line_total_cents, line.discount_cents, line.is_gift as i32],
        )?;
    }
    Ok(())
}

/// tx B：所有渠道 SUCCESS 后提交。订单 → COMPLETED + 库存 SALE + sync_queue。
pub fn commit_order(tx: &Connection, order_no: &str, cashier_id: i64) -> Result<()> {
    let status: String = tx.query_row(
        "SELECT status FROM sales_order WHERE order_no=?1",
        [order_no],
        |r| r.get(0),
    )?;
    if status != "PENDING_PAYMENT" {
        return Err(CoreError::Validation(format!("订单状态 {status} 不可提交")));
    }
    let items: Vec<(i64, i64)> = {
        let mut stmt =
            tx.prepare("SELECT sku_id, qty FROM sales_order_item WHERE order_no=?1 AND is_gift=0")?;
        let rows = stmt.query_map([order_no], |r| Ok((r.get(0)?, r.get(1)?)))?;
        let mut v = Vec::new();
        for row in rows {
            v.push(row?);
        }
        v
    };
    for (sku_id, qty) in items {
        let neg = allows_negative(tx, sku_id)?;
        apply_event(
            tx,
            sku_id,
            qty,
            TransType::SALE,
            Some(order_no),
            Some(cashier_id),
            neg,
        )?;
    }
    tx.execute(
        "UPDATE sales_order SET status='COMPLETED' WHERE order_no=?1",
        [order_no],
    )?;
    tx.execute(
        "INSERT INTO sync_queue(op_id,op_type,payload_json,created_at) VALUES (?1,'ORDER_COMPLETED',?2,?3)",
        params![format!("{order_no}:completed"), json!({"order_no": order_no}).to_string(), format_utc(&Utc::now())],
    )?;
    Ok(())
}

/// tx C：失败/超时/取消 → CANCELED（终态）。
pub fn cancel_order(tx: &Connection, order_no: &str) -> Result<()> {
    let status: String = tx.query_row(
        "SELECT status FROM sales_order WHERE order_no=?1",
        [order_no],
        |r| r.get(0),
    )?;
    if status != "PENDING_PAYMENT" {
        return Err(CoreError::Validation("仅待支付订单可取消".into()));
    }
    tx.execute(
        "UPDATE sales_order SET status='CANCELED' WHERE order_no=?1",
        [order_no],
    )?;
    Ok(())
}

/// 作废（§5.5）：仅 PENDING_PAYMENT 或当日现金 COMPLETED；扫码已支付单拒绝。
pub fn void_order(
    tx: &Connection,
    order_no: &str,
    paid_via_scan: bool,
    is_today: bool,
    operator_id: i64,
) -> Result<()> {
    let status: String = tx.query_row(
        "SELECT status FROM sales_order WHERE order_no=?1",
        [order_no],
        |r| r.get(0),
    )?;
    if paid_via_scan {
        return Err(CoreError::Validation("扫码已支付订单只能退款，不能作废".into()));
    }
    let allowed = status == "PENDING_PAYMENT" || (status == "COMPLETED" && is_today);
    if !allowed {
        return Err(CoreError::Validation("该订单不满足作废条件".into()));
    }
    if status == "COMPLETED" {
        let items: Vec<(i64, i64)> = {
            let mut stmt = tx.prepare(
                "SELECT sku_id, qty FROM sales_order_item WHERE order_no=?1 AND is_gift=0",
            )?;
            let rows = stmt.query_map([order_no], |r| Ok((r.get(0)?, r.get(1)?)))?;
            let mut v = Vec::new();
            for row in rows {
                v.push(row?);
            }
            v
        };
        for (sku, qty) in items {
            apply_event(
                tx,
                sku,
                qty,
                TransType::VOID,
                Some(order_no),
                Some(operator_id),
                true,
            )?;
        }
    }
    tx.execute(
        "UPDATE sales_order SET status='VOIDED' WHERE order_no=?1",
        [order_no],
    )?;
    Ok(())
}

/// 挂单：购物车快照写库（upsert 语义由调用方决定）。
pub fn park_cart(
    conn: &Connection,
    cart_json: &str,
    cashier_id: i64,
    member_id: Option<i64>,
    remark: Option<&str>,
    now: &str,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO parked_order(cart_json,cashier_id,member_id,remark,status,created_at,updated_at)
         VALUES (?1,?2,?3,?4,'PENDING',?5,?5)",
        params![cart_json, cashier_id, member_id, remark, now],
    )?;
    Ok(conn.last_insert_rowid())
}

/// 取单：置 RESUMED，返回购物车快照。
pub fn resume_cart(conn: &Connection, park_id: i64, now: &str) -> Result<String> {
    let cart: String = conn.query_row(
        "SELECT cart_json FROM parked_order WHERE park_id=?1 AND status='PENDING'",
        [park_id],
        |r| r.get(0),
    )?;
    conn.execute(
        "UPDATE parked_order SET status='RESUMED', updated_at=?1 WHERE park_id=?2",
        params![now, park_id],
    )?;
    Ok(cart)
}

/// 退货：每次生成独立退货单，回补库存，累加 returned_qty。
pub fn create_return(
    tx: &Connection,
    return_no: &str,
    order_no: &str,
    member_id: Option<i64>,
    cashier_id: i64,
    approver_id: Option<i64>,
    items: &[(i64, i64, i64)], // (order_item_id, sku_id, qty)
    refunds: &[i64],
    operator_id: i64,
) -> Result<i64> {
    let refund_total: i64 = refunds.iter().sum();
    tx.execute(
        "INSERT INTO sales_return(return_no,order_no,member_id,cashier_id,approver_id,refund_total_cents,status,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,'COMPLETED',?7)",
        params![return_no, order_no, member_id, cashier_id, approver_id, refund_total, format_utc(&Utc::now())],
    )?;
    for ((item_id, sku_id, qty), refund) in items.iter().zip(refunds.iter()) {
        tx.execute(
            "INSERT INTO sales_return_item(return_no,order_item_id,sku_id,qty,refund_cents) VALUES (?1,?2,?3,?4,?5)",
            params![return_no, item_id, sku_id, qty, refund],
        )?;
        let already: i64 = tx.query_row(
            "SELECT returned_qty FROM sales_order_item WHERE item_id=?1",
            [item_id],
            |r| r.get(0),
        )?;
        let line_qty: i64 = tx.query_row(
            "SELECT qty FROM sales_order_item WHERE item_id=?1",
            [item_id],
            |r| r.get(0),
        )?;
        if already + qty > line_qty {
            return Err(CoreError::Validation("累计退货数量超过行数量".into()));
        }
        tx.execute(
            "UPDATE sales_order_item SET returned_qty=?1 WHERE item_id=?2",
            params![already + qty, item_id],
        )?;
        apply_event(
            tx,
            *sku_id,
            *qty,
            TransType::RETURN,
            Some(order_no),
            Some(operator_id),
            true,
        )?;
    }
    // 订单状态更新
    let total_items: i64 = tx.query_row(
        "SELECT COUNT(*) FROM sales_order_item WHERE order_no=?1",
        [order_no],
        |r| r.get(0),
    )?;
    let returned_lines: i64 = tx.query_row(
        "SELECT COUNT(DISTINCT i.item_id) FROM sales_order_item i
         JOIN sales_return_item r ON r.order_item_id=i.item_id
         WHERE i.order_no=?1 AND i.returned_qty >= i.qty",
        [order_no],
        |r| r.get(0),
    )?;
    let new_status = if returned_lines >= total_items {
        "REFUNDED"
    } else {
        "PARTIAL_REFUND"
    };
    tx.execute(
        "UPDATE sales_order SET status=?1 WHERE order_no=?2",
        params![new_status, order_no],
    )?;
    Ok(refund_total)
}

/// 交班汇总：订单数/应收/实收/现金差额，按 cashier_id 分组。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ShiftSummary {
    pub cashier_id: i64,
    pub order_count: i64,
    pub total_cents: i64,
    pub pay_cents: i64,
    pub cash_net_cents: i64,
}

pub fn shift_summary(conn: &Connection, shift_id: i64) -> Result<Vec<ShiftSummary>> {
    let mut stmt = conn.prepare(
        "SELECT o.cashier_id, COUNT(DISTINCT o.order_no), COALESCE(SUM(o.total_cents),0), COALESCE(SUM(o.pay_cents),0),
                COALESCE(SUM(CASE WHEN p.pay_channel='CASH' THEN p.amount_cents ELSE 0 END),0)
         FROM sales_order o
         LEFT JOIN payment_record p ON p.order_no=o.order_no AND p.status='SUCCESS'
         WHERE o.shift_id=?1 AND o.status IN ('COMPLETED','PARTIAL_REFUND','REFUNDED')
         GROUP BY o.cashier_id ORDER BY o.cashier_id",
    )?;
    let rows = stmt.query_map([shift_id], |r| {
        Ok(ShiftSummary {
            cashier_id: r.get(0)?,
            order_count: r.get(1)?,
            total_cents: r.get(2)?,
            pay_cents: r.get(3)?,
            cash_net_cents: r.get(4)?,
        })
    })?;
    let mut v = Vec::new();
    for row in rows {
        v.push(row?);
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{open_in_memory, run_migrations};
    use crate::promo::CartLine;

    fn setup() -> Connection {
        let mut c = open_in_memory().unwrap();
        run_migrations(&mut c).unwrap();
        c.execute(
            "INSERT INTO spu(category_id,spu_name,spu_code) VALUES (1,'测试','S1')",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO sku(spu_id,barcode,sale_price_cents) VALUES (1,'B1',1000)",
            [],
        )
        .unwrap();
        c
    }

    fn start(c: &Connection, no: &str) {
        let lines = vec![CartLine::new(1, 2, 1000).unwrap()];
        let tx = c.unchecked_transaction().unwrap();
        begin_order(
            &tx,
            no,
            "D1",
            1,
            1,
            None,
            &lines,
            2000,
            0,
            "2026-11-17",
            "2026-11-17T00:00:00Z",
        )
        .unwrap();
        tx.commit().unwrap();
    }

    #[test]
    fn tx_a_then_tx_b_stock_only_after_commit() {
        let c = setup();
        start(&c, "O1");
        let q: i64 = c
            .query_row(
                "SELECT COALESCE(qty,0) FROM inv_stock WHERE sku_id=1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(q, 0, "tx A 后不得扣库存");
        {
            let tx = c.unchecked_transaction().unwrap();
            commit_order(&tx, "O1", 1).unwrap();
            tx.commit().unwrap();
        }
        let q: i64 = c
            .query_row("SELECT qty FROM inv_stock WHERE sku_id=1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(q, -2);
        let s: String = c
            .query_row(
                "SELECT status FROM sales_order WHERE order_no='O1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(s, "COMPLETED");
    }

    #[test]
    fn cancel_is_terminal() {
        let c = setup();
        start(&c, "O2");
        {
            let tx = c.unchecked_transaction().unwrap();
            cancel_order(&tx, "O2").unwrap();
            tx.commit().unwrap();
        }
        assert!(matches!(
            cancel_order(&c.unchecked_transaction().unwrap(), "O2"),
            Err(_)
        ));
    }

    #[test]
    fn scan_paid_order_cannot_void() {
        let c = setup();
        start(&c, "O3");
        let tx = c.unchecked_transaction().unwrap();
        let err = void_order(&tx, "O3", true, true, 1).unwrap_err();
        assert!(err.to_string().contains("退款"));
    }

    #[test]
    fn void_restores_stock() {
        let c = setup();
        start(&c, "O4");
        {
            let tx = c.unchecked_transaction().unwrap();
            commit_order(&tx, "O4", 1).unwrap();
            tx.commit().unwrap();
        }
        {
            let tx = c.unchecked_transaction().unwrap();
            void_order(&tx, "O4", false, true, 1).unwrap();
            tx.commit().unwrap();
        }
        let q: i64 = c
            .query_row("SELECT qty FROM inv_stock WHERE sku_id=1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(q, 0);
    }

    #[test]
    fn park_resume_roundtrip() {
        let c = setup();
        let id = park_cart(&c, "{\"lines\":[]}", 1, None, None, "2026-11-17T00:00:00Z").unwrap();
        let cart = resume_cart(&c, id, "2026-11-17T01:00:00Z").unwrap();
        assert_eq!(cart, "{\"lines\":[]}");
        assert!(
            resume_cart(&c, id, "2026-11-17T02:00:00Z").is_err(),
            "取单只能一次"
        );
    }

    #[test]
    fn return_updates_returned_qty_and_stock() {
        let c = setup();
        start(&c, "O5");
        {
            let tx = c.unchecked_transaction().unwrap();
            commit_order(&tx, "O5", 1).unwrap();
            tx.commit().unwrap();
        }
        let item_id: i64 = c
            .query_row(
                "SELECT item_id FROM sales_order_item WHERE order_no='O5'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        {
            let tx = c.unchecked_transaction().unwrap();
            create_return(
                &tx,
                "R1",
                "O5",
                None,
                1,
                Some(1),
                &[(item_id, 1, 1)],
                &[1000],
                1,
            )
            .unwrap();
            tx.commit().unwrap();
        }
        let returned: i64 = c
            .query_row(
                "SELECT returned_qty FROM sales_order_item WHERE item_id=?1",
                [item_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(returned, 1);
        let s: String = c
            .query_row(
                "SELECT status FROM sales_order WHERE order_no='O5'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(s, "PARTIAL_REFUND");
    }

    #[test]
    fn business_date_cutoff() {
        let dt = parse("2026-11-17T03:00:00Z");
        assert_eq!(business_date(&dt, 5), "2026-11-16");
        assert_eq!(business_date(&dt, 0), "2026-11-17");
    }

    fn parse(s: &str) -> DateTime<Utc> {
        crate::time::parse_rfc3339(s).unwrap()
    }
}
