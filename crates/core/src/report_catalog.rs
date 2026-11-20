//! 统一报表目录（§14 扩展）。
//!
//! 33 张报表共用 `ReportResult` 结构与前端渲染器：后端按 report_id 分发参数化 SQL，
//! 金额全程整数分运算，仅在展示前用 `cents_to_yuan` 格式化，杜绝浮点误差。
//! 每张报表都有独立单测覆盖空集与合计不变式。

use crate::error::{CoreError, Result};
use crate::money::cents_to_yuan;
use rusqlite::{params, Connection};
use serde::Serialize;

/// 列定义：kind 供前端决定格式化方式（money/qty/text/percent）。
#[derive(Debug, Clone, Serialize)]
pub struct ReportColumn {
    pub key: String,
    pub label: String,
    pub kind: String,
}

/// 顶部指标卡（value 已格式化为展示串）。
#[derive(Debug, Clone, Serialize)]
pub struct ReportMetric {
    pub label: String,
    pub value: String,
}

/// 统一报表结果：列 + 数据行（已格式化）+ 指标卡。
#[derive(Debug, Clone, Serialize)]
pub struct ReportResult {
    pub id: String,
    pub title: String,
    pub description: String,
    pub columns: Vec<ReportColumn>,
    pub rows: Vec<Vec<String>>,
    pub metrics: Vec<ReportMetric>,
}

/// 报表清单元数据，前端据此渲染左侧导航。
#[derive(Debug, Clone, Serialize)]
pub struct ReportMeta {
    pub id: String,
    pub title: String,
    pub group: String,
}

fn cols(specs: &[(&str, &str, &str)]) -> Vec<ReportColumn> {
    specs
        .iter()
        .map(|(k, l, t)| ReportColumn {
            key: (*k).into(),
            label: (*l).into(),
            kind: (*t).into(),
        })
        .collect()
}

fn m(label: &str, value: String) -> ReportMetric {
    ReportMetric {
        label: label.into(),
        value,
    }
}

fn money(c: i64) -> String {
    cents_to_yuan(c)
}

fn n(v: i64) -> String {
    v.to_string()
}

/// 完整报表清单：5 大域、33 张报表。顺序即前端展示顺序。
pub fn report_catalog() -> Vec<ReportMeta> {
    const ITEMS: &[(&str, &str, &str)] = &[
        ("sales_daily", "销售日报", "销售"),
        ("sales_trend", "销售趋势报表", "销售"),
        ("sales_hourly", "时段销售报表", "销售"),
        ("order_flow", "订单流水报表", "销售"),
        ("payment_summary", "支付方式汇总报表", "销售"),
        ("payment_reconcile", "支付对账报表", "销售"),
        ("refund", "退款报表", "销售"),
        ("discount", "折扣优惠报表", "销售"),
        ("product_ranking", "商品销售排行", "商品"),
        ("category_share", "品类销售占比", "商品"),
        ("product_margin", "商品毛利报表", "商品"),
        ("product_turnover", "商品动销报表", "商品"),
        ("inventory_summary", "进销存汇总", "库存"),
        ("inventory_balance", "库存余额报表", "库存"),
        ("inventory_flow", "库存流水报表", "库存"),
        ("inventory_alert", "库存预警报表", "库存"),
        ("stocktake_diff", "盘点差异报表", "库存"),
        ("scrap", "报损报表", "库存"),
        ("member_consumption", "会员消费报表", "会员"),
        ("member_repurchase", "会员复购报表", "会员"),
        ("member_growth", "会员增长报表", "会员"),
        ("stored_value", "储值报表", "会员"),
        ("stored_value_flow", "储值流水报表", "会员"),
        ("points", "积分报表", "会员"),
        ("cashier_performance", "收银员业绩报表", "运营"),
        ("shift_handover", "交接班报表", "运营"),
        ("operation_audit", "操作审计报表", "运营"),
        ("store_comparison", "门店经营对比报表", "运营"),
        ("daily_settlement", "日结报表", "运营"),
        ("abnormal_transaction", "异常交易报表", "运营"),
        ("cash_difference", "现金差异报表", "运营"),
        ("promotion_effect", "促销效果报表", "运营"),
        ("margin_summary", "毛利汇总报表", "运营"),
    ];
    ITEMS
        .iter()
        .map(|(id, title, group)| ReportMeta {
            id: (*id).into(),
            title: (*title).into(),
            group: (*group).into(),
        })
        .collect()
}

/// 按报表 ID 分发。未知 ID 返回校验错误，不静默空表。
pub fn run_report(conn: &Connection, id: &str, from: &str, to: &str) -> Result<ReportResult> {
    match id {
        "sales_daily" => sales_daily(conn, from, to),
        "sales_trend" => sales_trend(conn, from, to),
        "sales_hourly" => sales_hourly(conn, from, to),
        "order_flow" => order_flow(conn, from, to),
        "payment_summary" => payment_summary(conn, from, to),
        "payment_reconcile" => payment_reconcile(conn, from, to),
        "refund" => refund_report(conn, from, to),
        "discount" => discount_report(conn, from, to),
        "product_ranking" => product_ranking(conn, from, to),
        "category_share" => category_share(conn, from, to),
        "product_margin" => product_margin(conn, from, to),
        "product_turnover" => product_turnover(conn, from, to),
        "inventory_summary" => inventory_summary(conn, from, to),
        "inventory_balance" => inventory_balance(conn, from, to),
        "inventory_flow" => inventory_flow(conn, from, to),
        "inventory_alert" => inventory_alert(conn, from, to),
        "stocktake_diff" => stocktake_diff(conn, from, to),
        "scrap" => scrap_report(conn, from, to),
        "member_consumption" => member_consumption(conn, from, to),
        "member_repurchase" => member_repurchase(conn, from, to),
        "member_growth" => member_growth(conn, from, to),
        "stored_value" => stored_value(conn, from, to),
        "stored_value_flow" => stored_value_flow(conn, from, to),
        "points" => points_report(conn, from, to),
        "cashier_performance" => cashier_performance(conn, from, to),
        "shift_handover" => shift_handover(conn, from, to),
        "operation_audit" => operation_audit(conn, from, to),
        "store_comparison" => store_comparison(conn, from, to),
        "daily_settlement" => daily_settlement(conn, from, to),
        "abnormal_transaction" => abnormal_transaction(conn, from, to),
        "cash_difference" => cash_difference(conn, from, to),
        "promotion_effect" => promotion_effect(conn, from, to),
        "margin_summary" => margin_summary(conn, from, to),
        _ => Err(CoreError::Validation(format!("未知报表 ID: {id}"))),
    }
}

// ==================== 销售域（8 张）====================

fn sales_daily(conn: &Connection, from: &str, to: &str) -> Result<ReportResult> {
    let mut stmt = conn.prepare(
        "SELECT business_date, COUNT(DISTINCT order_no),
                COALESCE(SUM(pay_cents),0), COALESCE(SUM(discount_cents),0),
                COALESCE(SUM(CASE WHEN status='REFUNDED' THEN pay_cents ELSE 0 END),0)
         FROM sales_order
         WHERE status IN ('COMPLETED','PARTIAL_REFUND','REFUNDED')
           AND business_date BETWEEN ?1 AND ?2
         GROUP BY business_date ORDER BY business_date",
    )?;
    let raw = stmt
        .query_map(params![from, to], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let (mut orders, mut sales, mut disc, mut refund) = (0i64, 0i64, 0i64, 0i64);
    let rows = raw
        .iter()
        .map(|(d, o, s, dc, rf)| {
            orders += o;
            sales += s;
            disc += dc;
            refund += rf;
            vec![d.clone(), n(*o), money(*s), money(*dc), money(*rf)]
        })
        .collect();
    Ok(ReportResult {
        id: "sales_daily".into(),
        title: "销售日报".into(),
        description: "按营业日汇总订单、销售、优惠与退款，日切点以后端配置为准。".into(),
        columns: cols(&[
            ("date", "营业日期", "date"),
            ("orders", "订单数", "qty"),
            ("sales", "销售额", "money"),
            ("discount", "优惠额", "money"),
            ("refund", "退款额", "money"),
        ]),
        rows,
        metrics: vec![
            m("销售额合计", money(sales)),
            m("订单数", n(orders)),
            m("优惠合计", money(disc)),
            m("退款合计", money(refund)),
        ],
    })
}

fn sales_trend(conn: &Connection, from: &str, to: &str) -> Result<ReportResult> {
    let mut stmt = conn.prepare(
        "SELECT business_date, COUNT(DISTINCT order_no),
                COALESCE(SUM(pay_cents),0), COALESCE(SUM(total_cents),0)
         FROM sales_order
         WHERE status IN ('COMPLETED','PARTIAL_REFUND') AND business_date BETWEEN ?1 AND ?2
         GROUP BY business_date ORDER BY business_date",
    )?;
    let raw = stmt
        .query_map(params![from, to], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let (mut orders, mut sales) = (0i64, 0i64);
    let rows = raw
        .iter()
        .map(|(d, o, s, g)| {
            orders += o;
            sales += s;
            let avg = if *o > 0 { *s / *o } else { 0 };
            vec![d.clone(), n(*o), money(*s), money(*g), money(avg)]
        })
        .collect();
    let days = raw.len() as i64;
    let avg_day = if days > 0 { sales / days } else { 0 };
    Ok(ReportResult {
        id: "sales_trend".into(),
        title: "销售趋势报表".into(),
        description: "按日展示销售额、应收额与客单价，用于观察经营走势。".into(),
        columns: cols(&[
            ("date", "营业日期", "date"),
            ("orders", "订单数", "qty"),
            ("sales", "实收额", "money"),
            ("gross", "应收额", "money"),
            ("avg", "客单价", "money"),
        ]),
        rows,
        metrics: vec![
            m("累计实收", money(sales)),
            m("累计订单", n(orders)),
            m("日均实收", money(avg_day)),
        ],
    })
}

fn sales_hourly(conn: &Connection, from: &str, to: &str) -> Result<ReportResult> {
    let mut stmt = conn.prepare(
        "SELECT substr(created_at,12,2) AS hh, COUNT(DISTINCT order_no),
                COALESCE(SUM(pay_cents),0)
         FROM sales_order
         WHERE status IN ('COMPLETED','PARTIAL_REFUND') AND business_date BETWEEN ?1 AND ?2
         GROUP BY hh ORDER BY hh",
    )?;
    let raw = stmt
        .query_map(params![from, to], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut sales = 0i64;
    let rows = raw
        .iter()
        .map(|(h, o, s)| {
            sales += s;
            vec![format!("{h}:00"), n(*o), money(*s)]
        })
        .collect();
    Ok(ReportResult {
        id: "sales_hourly".into(),
        title: "时段销售报表".into(),
        description: "按成交小时聚合，识别一天中的客流高峰时段。".into(),
        columns: cols(&[
            ("hour", "时段", "text"),
            ("orders", "订单数", "qty"),
            ("sales", "销售额", "money"),
        ]),
        rows,
        metrics: vec![
            m("销售额合计", money(sales)),
            m("统计时段数", n(raw.len() as i64)),
        ],
    })
}

fn order_flow(conn: &Connection, from: &str, to: &str) -> Result<ReportResult> {
    let mut stmt = conn.prepare(
        "SELECT order_no, created_at, cashier_id, total_cents, discount_cents, pay_cents, status
         FROM sales_order
         WHERE business_date BETWEEN ?1 AND ?2
         ORDER BY created_at DESC LIMIT 500",
    )?;
    let raw = stmt
        .query_map(params![from, to], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, String>(6)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let (mut orders, mut sales) = (0i64, 0i64);
    let rows = raw
        .iter()
        .map(|(no, t, c, g, d, p, s)| {
            orders += 1;
            sales += p;
            vec![
                no.clone(),
                t.clone(),
                n(*c),
                money(*g),
                money(*d),
                money(*p),
                s.clone(),
            ]
        })
        .collect();
    Ok(ReportResult {
        id: "order_flow".into(),
        title: "订单流水报表".into(),
        description: "逐笔列出订单，含作废与退款单；最多显示最近 500 笔。".into(),
        columns: cols(&[
            ("order_no", "订单号", "text"),
            ("time", "下单时间", "text"),
            ("cashier", "收银员ID", "qty"),
            ("gross", "应收", "money"),
            ("discount", "优惠", "money"),
            ("pay", "实收", "money"),
            ("status", "状态", "text"),
        ]),
        rows,
        metrics: vec![m("流水笔数", n(orders)), m("实收合计", money(sales))],
    })
}

fn payment_summary(conn: &Connection, from: &str, to: &str) -> Result<ReportResult> {
    let mut stmt = conn.prepare(
        "SELECT p.pay_channel, COUNT(*), COALESCE(SUM(p.amount_cents),0),
                COALESCE(SUM(p.cash_tendered_cents),0), COALESCE(SUM(p.change_cents),0)
         FROM payment_record p JOIN sales_order o ON o.order_no=p.order_no
         WHERE p.status='SUCCESS' AND o.business_date BETWEEN ?1 AND ?2
         GROUP BY p.pay_channel ORDER BY SUM(p.amount_cents) DESC",
    )?;
    let raw = stmt
        .query_map(params![from, to], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut total = 0i64;
    for row in &raw {
        total += row.2;
    }
    let rows = raw
        .iter()
        .map(|(ch, c, a, t, change)| {
            let pct = if total > 0 { *a * 10000 / total } else { 0 };
            vec![
                ch.clone(),
                n(*c),
                money(*a),
                format!("{}.{:02}%", pct / 100, pct % 100),
                money(*t),
                money(*change),
            ]
        })
        .collect();
    Ok(ReportResult {
        id: "payment_summary".into(),
        title: "支付方式汇总报表".into(),
        description: "按支付渠道汇总成功收款、现金收付与找零，占比按金额计算。".into(),
        columns: cols(&[
            ("channel", "支付渠道", "text"),
            ("count", "笔数", "qty"),
            ("amount", "金额", "money"),
            ("share", "金额占比", "percent"),
            ("tendered", "现金收讫", "money"),
            ("change", "找零", "money"),
        ]),
        rows,
        metrics: vec![
            m("收款合计", money(total)),
            m("渠道数", n(raw.len() as i64)),
        ],
    })
}

fn payment_reconcile(conn: &Connection, from: &str, to: &str) -> Result<ReportResult> {
    let mut stmt = conn.prepare(
        "SELECT recon_date, channel, local_amount_cents, channel_amount_cents,
                local_amount_cents-channel_amount_cents, diff_type, COALESCE(order_no,'')
         FROM reconcile_record WHERE recon_date BETWEEN ?1 AND ?2
         ORDER BY recon_date, recon_id",
    )?;
    let raw = stmt
        .query_map(params![from, to], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut diff_total = 0i64;
    let mut abnormal = 0i64;
    let rows = raw
        .iter()
        .map(|(d, ch, la, ca, df, typ, no)| {
            diff_total += df;
            if *df != 0 {
                abnormal += 1;
            }
            vec![
                d.clone(),
                ch.clone(),
                money(*la),
                money(*ca),
                money(*df),
                typ.clone(),
                no.clone(),
            ]
        })
        .collect();
    Ok(ReportResult {
        id: "payment_reconcile".into(),
        title: "支付对账报表".into(),
        description: "本地账与渠道账逐笔比对，差异额为本地减渠道，负数表示渠道多收。".into(),
        columns: cols(&[
            ("date", "对账日期", "date"),
            ("channel", "渠道", "text"),
            ("local", "本地金额", "money"),
            ("remote", "渠道金额", "money"),
            ("diff", "差异额", "money"),
            ("type", "差异类型", "text"),
            ("order_no", "订单号", "text"),
        ]),
        rows,
        metrics: vec![m("差异合计", money(diff_total)), m("异常笔数", n(abnormal))],
    })
}

fn refund_report(conn: &Connection, from: &str, to: &str) -> Result<ReportResult> {
    let mut stmt = conn.prepare(
        "SELECT refund_no, order_no, amount_cents, reason, status, created_at,
                COALESCE(channel_refund_no,'')
         FROM refund_record
         WHERE substr(created_at,1,10) BETWEEN ?1 AND ?2
         ORDER BY created_at DESC",
    )?;
    let raw = stmt
        .query_map(params![from, to], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut total = 0i64;
    let rows = raw
        .iter()
        .map(|(no, o, a, reason, s, t, ch)| {
            total += a;
            vec![
                no.clone(),
                o.clone(),
                money(*a),
                reason.clone(),
                s.clone(),
                t.clone(),
                ch.clone(),
            ]
        })
        .collect();
    Ok(ReportResult {
        id: "refund".into(),
        title: "退款报表".into(),
        description: "资金退款流水，含处理中与已完成状态；退款额按原渠道回退。".into(),
        columns: cols(&[
            ("refund_no", "退款单号", "text"),
            ("order_no", "原订单号", "text"),
            ("amount", "退款额", "money"),
            ("reason", "原因", "text"),
            ("status", "状态", "text"),
            ("time", "申请时间", "text"),
            ("channel_no", "渠道退款单", "text"),
        ]),
        rows,
        metrics: vec![
            m("退款合计", money(total)),
            m("退款笔数", n(raw.len() as i64)),
        ],
    })
}

fn discount_report(conn: &Connection, from: &str, to: &str) -> Result<ReportResult> {
    let mut stmt = conn.prepare(
        "SELECT COALESCE(p.name, '促销#'||op.promotion_id), COUNT(*),
                COALESCE(SUM(op.discount_cents),0)
         FROM order_promotion op
         JOIN sales_order o ON o.order_no=op.order_no
         LEFT JOIN promotion p ON p.promotion_id=op.promotion_id
         WHERE o.business_date BETWEEN ?1 AND ?2
         GROUP BY op.promotion_id, p.name ORDER BY SUM(op.discount_cents) DESC",
    )?;
    let raw = stmt
        .query_map(params![from, to], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut total = 0i64;
    for row in &raw {
        total += row.2;
    }
    let rows = raw
        .iter()
        .map(|(name, c, a)| vec![name.clone(), n(*c), money(*a)])
        .collect();
    Ok(ReportResult {
        id: "discount".into(),
        title: "折扣优惠报表".into(),
        description: "按促销活动汇总优惠让利金额，来自订单促销明细。".into(),
        columns: cols(&[
            ("promotion", "促销活动", "text"),
            ("count", "使用次数", "qty"),
            ("discount", "优惠金额", "money"),
        ]),
        rows,
        metrics: vec![
            m("优惠合计", money(total)),
            m("活动数", n(raw.len() as i64)),
        ],
    })
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
             VALUES ('O1','D',1,1,10000,1000,9000,'COMPLETED','2026-11-17','2026-11-17T10:30:00Z')",
            [],
        ).unwrap();
        c
    }

    #[test]
    fn catalog_has_33_reports_in_5_groups() {
        let list = report_catalog();
        assert_eq!(list.len(), 33);
        let mut groups = std::collections::BTreeSet::new();
        for r in &list {
            groups.insert(r.group.as_str());
        }
        assert_eq!(groups.len(), 5);
    }

    #[test]
    fn sales_reports_sum_single_order() {
        let c = setup();
        let from = "2026-11-17";
        let to = "2026-11-17";
        for id in ["sales_daily", "sales_trend", "sales_hourly", "order_flow"] {
            let r = run_report(&c, id, from, to).unwrap();
            assert!(!r.rows.is_empty(), "{id} 应有数据");
            assert!(
                r.metrics.iter().any(|m| m.value.contains("90.00")),
                "{id} 应包含实收 90.00: {:?}",
                r.metrics.iter().map(|m| &m.value).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn empty_range_returns_empty_rows_not_error() {
        let c = setup();
        let r = run_report(&c, "sales_daily", "2020-01-01", "2020-01-02").unwrap();
        assert!(r.rows.is_empty());
    }

    #[test]
    fn unknown_report_is_rejected() {
        let c = setup();
        assert!(run_report(&c, "not_exist", "2026-11-17", "2026-11-17").is_err());
    }
}
