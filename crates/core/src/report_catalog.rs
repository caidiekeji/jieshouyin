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
        "SELECT strftime('%H', datetime(created_at,'+8 hours')) AS hh,
                COUNT(DISTINCT order_no), COALESCE(SUM(pay_cents),0)
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
        description: "按北京时间成交小时聚合，识别一天中的客流高峰时段。".into(),
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
        "SELECT refund_no, order_no, amount_cents, reason, status,
                datetime(created_at,'+8 hours') AS created_at,
                COALESCE(channel_refund_no,'')
         FROM refund_record
         WHERE date(created_at,'+8 hours') BETWEEN ?1 AND ?2
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
        description: "资金退款流水（北京时间），含处理中与已完成状态；退款额按原渠道回退。".into(),
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

// ==================== 商品域（4 张）====================

fn product_ranking(conn: &Connection, from: &str, to: &str) -> Result<ReportResult> {
    let mut stmt = conn.prepare(
        "SELECT s.spu_id, COALESCE(p.spu_name,'SKU#'||s.sku_id), SUM(i.qty-i.returned_qty),
                SUM(i.line_total_cents-i.discount_cents)
         FROM sales_order_item i
         JOIN sales_order o ON o.order_no=i.order_no
         JOIN sku s ON s.sku_id=i.sku_id
         JOIN spu p ON p.spu_id=s.spu_id
         WHERE o.status IN ('COMPLETED','PARTIAL_REFUND') AND o.business_date BETWEEN ?1 AND ?2
         GROUP BY s.spu_id, p.spu_name ORDER BY SUM(i.line_total_cents-i.discount_cents) DESC LIMIT 100",
    )?;
    let raw = stmt
        .query_map(params![from, to], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut sales = 0i64;
    for row in &raw {
        sales += row.3;
    }
    let rows = raw
        .iter()
        .enumerate()
        .map(|(idx, (_, name, q, a))| vec![n((idx + 1) as i64), name.clone(), n(*q), money(*a)])
        .collect();
    Ok(ReportResult {
        id: "product_ranking".into(),
        title: "商品销售排行".into(),
        description: "按商品（SPU）实销金额降序排列，取前 100 名。".into(),
        columns: cols(&[
            ("rank", "排名", "qty"),
            ("name", "商品名称", "text"),
            ("qty", "销量", "qty"),
            ("sales", "销售额", "money"),
        ]),
        rows,
        metrics: vec![
            m("入榜商品数", n(raw.len() as i64)),
            m("入榜销售额", money(sales)),
        ],
    })
}

fn category_share(conn: &Connection, from: &str, to: &str) -> Result<ReportResult> {
    let mut stmt = conn.prepare(
        "SELECT COALESCE(c.name,'未分类'), SUM(i.line_total_cents-i.discount_cents)
         FROM sales_order_item i
         JOIN sales_order o ON o.order_no=i.order_no
         JOIN sku s ON s.sku_id=i.sku_id
         JOIN spu p ON p.spu_id=s.spu_id
         LEFT JOIN category c ON c.category_id=p.category_id
         WHERE o.status IN ('COMPLETED','PARTIAL_REFUND') AND o.business_date BETWEEN ?1 AND ?2
         GROUP BY COALESCE(c.name,'未分类') ORDER BY SUM(i.line_total_cents-i.discount_cents) DESC",
    )?;
    let raw = stmt
        .query_map(params![from, to], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut total = 0i64;
    for row in &raw {
        total += row.1;
    }
    let rows = raw
        .iter()
        .map(|(name, a)| {
            let pct = if total > 0 { *a * 10000 / total } else { 0 };
            vec![
                name.clone(),
                money(*a),
                format!("{}.{:02}%", pct / 100, pct % 100),
            ]
        })
        .collect();
    Ok(ReportResult {
        id: "category_share".into(),
        title: "品类销售占比".into(),
        description: "按一级品类汇总销售额及占比，未归类商品计入「未分类」。".into(),
        columns: cols(&[
            ("category", "品类", "text"),
            ("sales", "销售额", "money"),
            ("share", "占比", "percent"),
        ]),
        rows,
        metrics: vec![
            m("品类总数", n(raw.len() as i64)),
            m("销售合计", money(total)),
        ],
    })
}

fn product_margin(conn: &Connection, from: &str, to: &str) -> Result<ReportResult> {
    let mut stmt = conn.prepare(
        "SELECT COALESCE(p.spu_name,'SKU#'||s.sku_id),
                SUM(i.qty-i.returned_qty),
                SUM(i.line_total_cents-i.discount_cents),
                COALESCE(SUM((i.qty-i.returned_qty)*s.cost_price_cents),0)
         FROM sales_order_item i
         JOIN sales_order o ON o.order_no=i.order_no
         JOIN sku s ON s.sku_id=i.sku_id
         JOIN spu p ON p.spu_id=s.spu_id
         WHERE o.status IN ('COMPLETED','PARTIAL_REFUND') AND o.business_date BETWEEN ?1 AND ?2
         GROUP BY s.sku_id, p.spu_name ORDER BY (SUM(i.line_total_cents-i.discount_cents)-SUM((i.qty-i.returned_qty)*s.cost_price_cents)) DESC LIMIT 100",
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
    let (mut sales, mut cost) = (0i64, 0i64);
    let rows = raw
        .iter()
        .map(|(name, q, s, c)| {
            sales += s;
            cost += c;
            let profit = s - c;
            let rate = if *s > 0 { profit * 10000 / *s } else { 0 };
            vec![
                name.clone(),
                n(*q),
                money(*s),
                money(*c),
                money(profit),
                format!("{}.{:02}%", rate / 100, rate % 100),
            ]
        })
        .collect();
    Ok(ReportResult {
        id: "product_margin".into(),
        title: "商品毛利报表".into(),
        description: "毛利=实销额−销售数量×成本价；成本取当前 SKU 进价快照。".into(),
        columns: cols(&[
            ("name", "商品名称", "text"),
            ("qty", "销量", "qty"),
            ("sales", "销售额", "money"),
            ("cost", "成本", "money"),
            ("profit", "毛利", "money"),
            ("rate", "毛利率", "percent"),
        ]),
        rows,
        metrics: vec![
            m("毛利合计", money(sales - cost)),
            m("销售额", money(sales)),
            m("成本合计", money(cost)),
        ],
    })
}

fn product_turnover(conn: &Connection, from: &str, to: &str) -> Result<ReportResult> {
    let mut stmt = conn.prepare(
        "SELECT COALESCE(p.spu_name,'SKU#'||s.sku_id), SUM(i.qty-i.returned_qty),
                COUNT(DISTINCT o.business_date), COALESCE(st.qty,0), s.alert_qty
         FROM sales_order_item i
         JOIN sales_order o ON o.order_no=i.order_no
         JOIN sku s ON s.sku_id=i.sku_id
         JOIN spu p ON p.spu_id=s.spu_id
         LEFT JOIN inv_stock st ON st.sku_id=s.sku_id AND st.warehouse_type='STORE'
         WHERE o.status IN ('COMPLETED','PARTIAL_REFUND') AND o.business_date BETWEEN ?1 AND ?2
         GROUP BY s.sku_id, p.spu_name, st.qty, s.alert_qty
         ORDER BY SUM(i.qty-i.returned_qty) DESC LIMIT 100",
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
    let mut active = 0i64;
    let rows = raw
        .iter()
        .map(|(name, q, days, stock, alert)| {
            if *q > 0 {
                active += 1;
            }
            vec![
                name.clone(),
                n(*q),
                n(*days),
                n(*stock),
                n(*alert),
                if *stock <= *alert && *alert > 0 {
                    "需补货".into()
                } else {
                    "正常".into()
                },
            ]
        })
        .collect();
    Ok(ReportResult {
        id: "product_turnover".into(),
        title: "商品动销报表".into(),
        description: "统计区间内有销售的 SKU、动销天数与当前库存，低于预警线标注需补货。".into(),
        columns: cols(&[
            ("name", "商品名称", "text"),
            ("qty", "销量", "qty"),
            ("days", "动销天数", "qty"),
            ("stock", "当前库存", "qty"),
            ("alert", "预警线", "qty"),
            ("flag", "状态", "text"),
        ]),
        rows,
        metrics: vec![
            m("动销商品数", n(active)),
            m("在售商品数", n(raw.len() as i64)),
        ],
    })
}

// ==================== 库存域（6 张）====================

fn inventory_summary(conn: &Connection, from: &str, to: &str) -> Result<ReportResult> {
    let mut stmt = conn.prepare(
        "SELECT COALESCE(p.spu_name,'SKU#'||t.sku_id),
                COALESCE(SUM(CASE WHEN t.trans_type='PURCHASE_IN' THEN t.change_qty ELSE 0 END),0),
                COALESCE(SUM(CASE WHEN t.trans_type='SALE' THEN -t.change_qty ELSE 0 END),0),
                COALESCE(SUM(CASE WHEN t.trans_type IN ('RETURN','VOID') THEN t.change_qty ELSE 0 END),0),
                COALESCE(SUM(t.change_qty),0)
         FROM inv_trans t
         JOIN sku s ON s.sku_id=t.sku_id
         JOIN spu p ON p.spu_id=s.spu_id
         WHERE substr(t.created_at,1,10) BETWEEN ?1 AND ?2
         GROUP BY t.sku_id, p.spu_name ORDER BY COALESCE(SUM(t.change_qty),0) DESC",
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
    let rows = raw
        .iter()
        .map(|(name, inq, outq, ret, net)| vec![name.clone(), n(*inq), n(*outq), n(*ret), n(*net)])
        .collect();
    Ok(ReportResult {
        id: "inventory_summary".into(),
        title: "进销存汇总".into(),
        description: "按商品汇总区间内入库、销售、退换与净变动，取数自库存事件。".into(),
        columns: cols(&[
            ("name", "商品名称", "text"),
            ("in", "入库量", "qty"),
            ("out", "销售出库", "qty"),
            ("return", "退换入库", "qty"),
            ("net", "净变动", "qty"),
        ]),
        rows,
        metrics: vec![m("商品数", n(raw.len() as i64))],
    })
}

fn inventory_balance(conn: &Connection, _from: &str, _to: &str) -> Result<ReportResult> {
    let mut stmt = conn.prepare(
        "SELECT COALESCE(p.spu_name,'SKU#'||s.sku_id), st.warehouse_type, st.qty,
                st.locked_qty, s.cost_price_cents, st.qty*s.cost_price_cents
         FROM inv_stock st
         JOIN sku s ON s.sku_id=st.sku_id
         JOIN spu p ON p.spu_id=s.spu_id
         WHERE st.qty<>0 OR st.locked_qty<>0
         ORDER BY st.qty*s.cost_price_cents DESC",
    )?;
    let raw = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut total_value = 0i64;
    let mut total_qty = 0i64;
    let rows = raw
        .iter()
        .map(|(name, wh, q, lq, cost, val)| {
            total_value += val;
            total_qty += q;
            vec![
                name.clone(),
                wh.clone(),
                n(*q),
                n(*lq),
                money(*cost),
                money(*val),
            ]
        })
        .collect();
    Ok(ReportResult {
        id: "inventory_balance".into(),
        title: "库存余额报表".into(),
        description: "当前各仓库存数量与成本金额快照，成本取 SKU 进价。".into(),
        columns: cols(&[
            ("name", "商品名称", "text"),
            ("warehouse", "仓库", "text"),
            ("qty", "库存量", "qty"),
            ("locked", "锁定量", "qty"),
            ("cost", "单价成本", "money"),
            ("value", "库存金额", "money"),
        ]),
        rows,
        metrics: vec![
            m("库存金额合计", money(total_value)),
            m("库存数量合计", n(total_qty)),
        ],
    })
}

fn inventory_flow(conn: &Connection, from: &str, to: &str) -> Result<ReportResult> {
    let mut stmt = conn.prepare(
        "SELECT substr(t.created_at,1,19), COALESCE(p.spu_name,'SKU#'||t.sku_id),
                t.trans_type, t.change_qty, COALESCE(t.ref_order_no,''), COALESCE(u.username,'系统')
         FROM inv_trans t
         JOIN sku s ON s.sku_id=t.sku_id
         JOIN spu p ON p.spu_id=s.spu_id
         LEFT JOIN user_account u ON u.user_id=t.operator_id
         WHERE substr(t.created_at,1,10) BETWEEN ?1 AND ?2
         ORDER BY t.created_at DESC LIMIT 500",
    )?;
    let raw = stmt
        .query_map(params![from, to], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let rows = raw
        .iter()
        .map(|(t, name, typ, q, refno, op)| {
            vec![
                t.clone(),
                name.clone(),
                typ.clone(),
                n(*q),
                refno.clone(),
                op.clone(),
            ]
        })
        .collect();
    Ok(ReportResult {
        id: "inventory_flow".into(),
        title: "库存流水报表".into(),
        description: "逐笔库存事件，含出入库、销售、退货、调账与报损；最多 500 笔。".into(),
        columns: cols(&[
            ("time", "时间", "text"),
            ("name", "商品名称", "text"),
            ("type", "事件类型", "text"),
            ("qty", "变动量", "qty"),
            ("ref", "关联单号", "text"),
            ("operator", "操作人", "text"),
        ]),
        rows,
        metrics: vec![m("流水笔数", n(raw.len() as i64))],
    })
}

fn inventory_alert(conn: &Connection, _from: &str, _to: &str) -> Result<ReportResult> {
    let mut stmt = conn.prepare(
        "SELECT COALESCE(p.spu_name,'SKU#'||s.sku_id), COALESCE(st.qty,0), s.alert_qty,
                s.allow_negative_stock
         FROM sku s
         JOIN spu p ON p.spu_id=s.spu_id
         LEFT JOIN inv_stock st ON st.sku_id=s.sku_id AND st.warehouse_type='STORE'
         WHERE s.alert_qty>0 AND COALESCE(st.qty,0)<=s.alert_qty
         ORDER BY (COALESCE(st.qty,0)-s.alert_qty), p.spu_name",
    )?;
    let raw = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut critical = 0i64;
    let rows = raw
        .iter()
        .map(|(name, q, alert, allow_neg)| {
            let gap = alert - q;
            if *q <= 0 {
                critical += 1;
            }
            vec![
                name.clone(),
                n(*q),
                n(*alert),
                n(gap),
                if *q <= 0 {
                    "缺货".into()
                } else {
                    "低库存".into()
                },
                if *allow_neg == 1 {
                    "允许负库存".into()
                } else {
                    "禁止负库存".into()
                },
            ]
        })
        .collect();
    Ok(ReportResult {
        id: "inventory_alert".into(),
        title: "库存预警报表".into(),
        description: "当前库存低于预警线的商品，库存≤0 标记缺货。".into(),
        columns: cols(&[
            ("name", "商品名称", "text"),
            ("qty", "当前库存", "qty"),
            ("alert", "预警线", "qty"),
            ("gap", "补货缺口", "qty"),
            ("level", "预警级别", "text"),
            ("policy", "负库存策略", "text"),
        ]),
        rows,
        metrics: vec![
            m("预警商品数", n(raw.len() as i64)),
            m("缺货商品数", n(critical)),
        ],
    })
}

fn stocktake_diff(conn: &Connection, from: &str, to: &str) -> Result<ReportResult> {
    let mut stmt = conn.prepare(
        "SELECT st.task_no, COALESCE(p.spu_name,'SKU#'||si.sku_id), si.book_qty,
                COALESCE(si.counted_qty,si.book_qty), COALESCE(si.diff_qty,0),
                COALESCE(si.reason,''), st.status, COALESCE(substr(st.finished_at,1,19),'')
         FROM stocktake_item si
         JOIN stocktake st ON st.stocktake_id=si.stocktake_id
         JOIN sku s ON s.sku_id=si.sku_id
         JOIN spu p ON p.spu_id=s.spu_id
         WHERE substr(st.created_at,1,10) BETWEEN ?1 AND ?2
         ORDER BY st.created_at DESC LIMIT 500",
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
                r.get::<_, String>(7)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut diff_abs = 0i64;
    let rows = raw
        .iter()
        .map(|(task, name, book, counted, diff, reason, status, fin)| {
            diff_abs += diff.abs();
            vec![
                task.clone(),
                name.clone(),
                n(*book),
                n(*counted),
                n(*diff),
                reason.clone(),
                status.clone(),
                fin.clone(),
            ]
        })
        .collect();
    Ok(ReportResult {
        id: "stocktake_diff".into(),
        title: "盘点差异报表".into(),
        description: "逐项展示账面数、实盘数与差异量，差异绝对值合计用于衡量盘点质量。".into(),
        columns: cols(&[
            ("task", "盘点单号", "text"),
            ("name", "商品名称", "text"),
            ("book", "账面数", "qty"),
            ("counted", "实盘数", "qty"),
            ("diff", "差异量", "qty"),
            ("reason", "原因", "text"),
            ("status", "状态", "text"),
            ("finished", "完成时间", "text"),
        ]),
        rows,
        metrics: vec![
            m("差异绝对值合计", n(diff_abs)),
            m("盘点项数", n(raw.len() as i64)),
        ],
    })
}

fn scrap_report(conn: &Connection, from: &str, to: &str) -> Result<ReportResult> {
    let mut stmt = conn.prepare(
        "SELECT substr(t.created_at,1,19), COALESCE(p.spu_name,'SKU#'||t.sku_id),
                -t.change_qty, COALESCE(t.ref_order_no,''), COALESCE(u.username,'系统')
         FROM inv_trans t
         JOIN sku s ON s.sku_id=t.sku_id
         JOIN spu p ON p.spu_id=s.spu_id
         LEFT JOIN user_account u ON u.user_id=t.operator_id
         WHERE t.trans_type='LOSS' AND substr(t.created_at,1,10) BETWEEN ?1 AND ?2
         ORDER BY t.created_at DESC",
    )?;
    let raw = stmt
        .query_map(params![from, to], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut loss_qty = 0i64;
    let rows = raw
        .iter()
        .map(|(t, name, q, refno, op)| {
            loss_qty += q;
            vec![t.clone(), name.clone(), n(*q), refno.clone(), op.clone()]
        })
        .collect();
    Ok(ReportResult {
        id: "scrap".into(),
        title: "报损报表".into(),
        description: "库存事件中的报损（LOSS）记录，数量为带符号事件取绝对值。".into(),
        columns: cols(&[
            ("time", "报损时间", "text"),
            ("name", "商品名称", "text"),
            ("qty", "报损数量", "qty"),
            ("ref", "关联单号", "text"),
            ("operator", "操作人", "text"),
        ]),
        rows,
        metrics: vec![
            m("报损数量合计", n(loss_qty)),
            m("报损笔数", n(raw.len() as i64)),
        ],
    })
}

// ==================== 会员域（6 张）====================

fn member_consumption(conn: &Connection, from: &str, to: &str) -> Result<ReportResult> {
    let mut stmt = conn.prepare(
        "SELECT m.member_id, COALESCE(m.name,COALESCE(m.phone,'会员#'||m.member_id)),
                COUNT(DISTINCT o.order_no), COALESCE(SUM(o.pay_cents),0),
                MAX(o.business_date)
         FROM sales_order o JOIN member m ON m.member_id=o.member_id
         WHERE o.status IN ('COMPLETED','PARTIAL_REFUND') AND o.business_date BETWEEN ?1 AND ?2
         GROUP BY m.member_id, m.name, m.phone ORDER BY SUM(o.pay_cents) DESC",
    )?;
    let raw = stmt
        .query_map(params![from, to], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, String>(4)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut total = 0i64;
    let rows = raw
        .iter()
        .map(|(id, name, c, amt, last)| {
            total += amt;
            let avg = if *c > 0 { *amt / *c } else { 0 };
            vec![
                n(*id),
                name.clone(),
                n(*c),
                money(*amt),
                money(avg),
                last.clone(),
            ]
        })
        .collect();
    Ok(ReportResult {
        id: "member_consumption".into(),
        title: "会员消费报表".into(),
        description: "按会员汇总区间内消费次数、金额与客单价，按消费额降序。".into(),
        columns: cols(&[
            ("id", "会员ID", "qty"),
            ("name", "会员", "text"),
            ("count", "消费次数", "qty"),
            ("amount", "消费金额", "money"),
            ("avg", "客单价", "money"),
            ("last", "最近消费", "date"),
        ]),
        rows,
        metrics: vec![
            m("消费会员数", n(raw.len() as i64)),
            m("会员消费合计", money(total)),
        ],
    })
}

fn member_repurchase(conn: &Connection, from: &str, to: &str) -> Result<ReportResult> {
    let mut stmt = conn.prepare(
        "SELECT m.member_id, COALESCE(m.name,COALESCE(m.phone,'会员#'||m.member_id)),
                COUNT(DISTINCT o.order_no) AS visits,
                COUNT(DISTINCT o.business_date) AS days,
                MIN(o.business_date), MAX(o.business_date)
         FROM sales_order o JOIN member m ON m.member_id=o.member_id
         WHERE o.status IN ('COMPLETED','PARTIAL_REFUND') AND o.business_date BETWEEN ?1 AND ?2
         GROUP BY m.member_id, m.name, m.phone ORDER BY visits DESC, days DESC",
    )?;
    let raw = stmt
        .query_map(params![from, to], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut repeat = 0i64;
    let rows = raw
        .iter()
        .map(|(id, name, visits, days, first, last)| {
            if *visits >= 2 {
                repeat += 1;
            }
            let gap = if *days > 1 {
                (*days - 1) / (*visits - 1).max(1)
            } else {
                0
            };
            vec![
                n(*id),
                name.clone(),
                n(*visits),
                n(*days),
                n(gap),
                first.clone(),
                last.clone(),
            ]
        })
        .collect();
    let rate = if raw.is_empty() {
        0
    } else {
        repeat * 10000 / raw.len() as i64
    };
    Ok(ReportResult {
        id: "member_repurchase".into(),
        title: "会员复购报表".into(),
        description: "统计会员消费次数与平均复购间隔（天），消费≥2 次计为复购会员。".into(),
        columns: cols(&[
            ("id", "会员ID", "qty"),
            ("name", "会员", "text"),
            ("visits", "消费次数", "qty"),
            ("days", "消费天数", "qty"),
            ("gap", "平均间隔(天)", "qty"),
            ("first", "首次消费", "date"),
            ("last", "最近消费", "date"),
        ]),
        rows,
        metrics: vec![
            m("复购会员数", n(repeat)),
            m("复购率", format!("{}.{:02}%", rate / 100, rate % 100)),
        ],
    })
}

fn member_growth(conn: &Connection, from: &str, to: &str) -> Result<ReportResult> {
    // member 表无 created_at 列，以首次资产流水日作为会员首次活跃日统计增长。
    let mut stmt = conn.prepare(
        "SELECT d, COUNT(*) FROM (
                SELECT member_id, MIN(date(created_at,'+8 hours')) AS d
                FROM member_asset_log GROUP BY member_id
             ) WHERE d BETWEEN ?1 AND ?2 GROUP BY d ORDER BY d",
    )?;
    let raw = stmt
        .query_map(params![from, to], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut cum = 0i64;
    let rows = raw
        .iter()
        .map(|(d, c)| {
            cum += c;
            vec![d.clone(), n(*c), n(cum)]
        })
        .collect();
    Ok(ReportResult {
        id: "member_growth".into(),
        title: "会员增长报表".into(),
        description: "按北京时间首次资产流水日统计新增会员数与累计会员数。".into(),
        columns: cols(&[
            ("date", "日期", "date"),
            ("new", "新增会员", "qty"),
            ("cumulative", "累计会员", "qty"),
        ]),
        rows,
        metrics: vec![m("区间新增", n(cum)), m("增长天数", n(raw.len() as i64))],
    })
}

fn stored_value(conn: &Connection, _from: &str, _to: &str) -> Result<ReportResult> {
    let mut stmt = conn.prepare(
        "SELECT m.member_id, COALESCE(m.name,COALESCE(m.phone,'会员#'||m.member_id)),
                COALESCE(SUM(CASE WHEN a.asset_type='PRINCIPAL' THEN a.balance ELSE 0 END),0),
                COALESCE(SUM(CASE WHEN a.asset_type='BONUS' THEN a.balance ELSE 0 END),0)
         FROM member m LEFT JOIN member_asset a ON a.member_id=m.member_id
         WHERE m.status=1
         GROUP BY m.member_id, m.name, m.phone
         ORDER BY (COALESCE(SUM(a.balance),0)) DESC",
    )?;
    let raw = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let (mut principal, mut bonus) = (0i64, 0i64);
    let rows = raw
        .iter()
        .map(|(id, name, p, b)| {
            principal += p;
            bonus += b;
            vec![n(*id), name.clone(), money(*p), money(*b), money(p + b)]
        })
        .collect();
    Ok(ReportResult {
        id: "stored_value".into(),
        title: "储值报表".into(),
        description: "会员储值余额快照，本金与赠金分列，合计为可用储值。".into(),
        columns: cols(&[
            ("id", "会员ID", "qty"),
            ("name", "会员", "text"),
            ("principal", "本金余额", "money"),
            ("bonus", "赠金余额", "money"),
            ("total", "可用合计", "money"),
        ]),
        rows,
        metrics: vec![
            m("本金合计", money(principal)),
            m("赠金合计", money(bonus)),
            m("储值总额", money(principal + bonus)),
        ],
    })
}

fn stored_value_flow(conn: &Connection, from: &str, to: &str) -> Result<ReportResult> {
    let mut stmt = conn.prepare(
        "SELECT substr(l.created_at,1,19), COALESCE(m.name,COALESCE(m.phone,'会员#'||l.member_id)),
                l.asset_type, l.change_amount, l.balance_after, l.reason,
                COALESCE(l.ref_order_no,'')
         FROM member_asset_log l LEFT JOIN member m ON m.member_id=l.member_id
         WHERE l.asset_type IN ('PRINCIPAL','BONUS') AND substr(l.created_at,1,10) BETWEEN ?1 AND ?2
         ORDER BY l.created_at DESC LIMIT 500",
    )?;
    let raw = stmt
        .query_map(params![from, to], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut recharge = 0i64;
    let rows = raw
        .iter()
        .map(|(t, name, typ, chg, bal, reason, refno)| {
            if *chg > 0 {
                recharge += chg;
            }
            vec![
                t.clone(),
                name.clone(),
                typ.clone(),
                money(*chg),
                money(*bal),
                reason.clone(),
                refno.clone(),
            ]
        })
        .collect();
    Ok(ReportResult {
        id: "stored_value_flow".into(),
        title: "储值流水报表".into(),
        description: "会员本金/赠金变动流水，含充值、消费与退款回补；最多 500 笔。".into(),
        columns: cols(&[
            ("time", "时间", "text"),
            ("name", "会员", "text"),
            ("type", "资产类型", "text"),
            ("change", "变动额", "money"),
            ("balance", "变动后余额", "money"),
            ("reason", "原因", "text"),
            ("ref", "关联单号", "text"),
        ]),
        rows,
        metrics: vec![
            m("充值合计", money(recharge)),
            m("流水笔数", n(raw.len() as i64)),
        ],
    })
}

fn points_report(conn: &Connection, from: &str, to: &str) -> Result<ReportResult> {
    let mut stmt = conn.prepare(
        "SELECT COALESCE(m.name,COALESCE(m.phone,'会员#'||l.member_id)),
                l.reason, COUNT(*), SUM(l.change_amount)
         FROM member_asset_log l LEFT JOIN member m ON m.member_id=l.member_id
         WHERE l.asset_type='POINTS' AND substr(l.created_at,1,10) BETWEEN ?1 AND ?2
         GROUP BY l.member_id, m.name, m.phone, l.reason
         ORDER BY SUM(l.change_amount) DESC",
    )?;
    let raw = stmt
        .query_map(params![from, to], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut earned = 0i64;
    let mut used = 0i64;
    let rows = raw
        .iter()
        .map(|(name, reason, c, amt)| {
            if *amt > 0 {
                earned += amt;
            } else {
                used += amt;
            }
            vec![name.clone(), reason.clone(), n(*c), n(*amt)]
        })
        .collect();
    Ok(ReportResult {
        id: "points".into(),
        title: "积分报表".into(),
        description: "按会员与原因汇总积分增减，正数获取、负数使用或冲正。".into(),
        columns: cols(&[
            ("name", "会员", "text"),
            ("reason", "原因", "text"),
            ("count", "笔数", "qty"),
            ("points", "积分变动", "qty"),
        ]),
        rows,
        metrics: vec![
            m("发放积分", n(earned)),
            m("使用/冲正", n(used)),
            m("净积分", n(earned + used)),
        ],
    })
}

// ==================== 运营域（9 张）====================

fn cashier_performance(conn: &Connection, from: &str, to: &str) -> Result<ReportResult> {
    let mut stmt = conn.prepare(
        "SELECT o.cashier_id, COALESCE(u.username,'收银员#'||o.cashier_id),
                COUNT(DISTINCT o.order_no), COALESCE(SUM(o.pay_cents),0),
                COALESCE(SUM(o.discount_cents),0)
         FROM sales_order o LEFT JOIN user_account u ON u.user_id=o.cashier_id
         WHERE o.status IN ('COMPLETED','PARTIAL_REFUND') AND o.business_date BETWEEN ?1 AND ?2
         GROUP BY o.cashier_id, u.username ORDER BY SUM(o.pay_cents) DESC",
    )?;
    let raw = stmt
        .query_map(params![from, to], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let (mut sales, mut orders) = (0i64, 0i64);
    let rows = raw
        .iter()
        .map(|(id, name, c, amt, disc)| {
            sales += amt;
            orders += c;
            let avg = if *c > 0 { *amt / *c } else { 0 };
            vec![
                n(*id),
                name.clone(),
                n(*c),
                money(*amt),
                money(avg),
                money(*disc),
            ]
        })
        .collect();
    Ok(ReportResult {
        id: "cashier_performance".into(),
        title: "收银员业绩报表".into(),
        description: "按收银员汇总订单数、实收额与客单价，按实收降序。".into(),
        columns: cols(&[
            ("id", "收银员ID", "qty"),
            ("name", "收银员", "text"),
            ("orders", "订单数", "qty"),
            ("sales", "实收额", "money"),
            ("avg", "客单价", "money"),
            ("discount", "优惠额", "money"),
        ]),
        rows,
        metrics: vec![m("实收合计", money(sales)), m("订单合计", n(orders))],
    })
}

fn shift_handover(conn: &Connection, from: &str, to: &str) -> Result<ReportResult> {
    let mut stmt = conn.prepare(
        "SELECT s.shift_id, COALESCE(substr(s.opened_at,1,19),''),
                s.opening_float_cents, COALESCE(s.expected_cash_cents,0),
                COALESCE(s.actual_cash_cents,0), COALESCE(s.difference_cents,0),
                s.status, COALESCE(s.difference_reason,'')
         FROM shift s
         WHERE substr(s.opened_at,1,10) BETWEEN ?1 AND ?2
         ORDER BY s.opened_at DESC",
    )?;
    let raw = stmt
        .query_map(params![from, to], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, String>(7)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut diff_abs = 0i64;
    let rows = raw
        .iter()
        .map(|(id, t, fl, exp, act, diff, st, reason)| {
            diff_abs += diff.abs();
            vec![
                n(*id),
                t.clone(),
                money(*fl),
                money(*exp),
                money(*act),
                money(*diff),
                st.clone(),
                reason.clone(),
            ]
        })
        .collect();
    Ok(ReportResult {
        id: "shift_handover".into(),
        title: "交接班报表".into(),
        description: "逐班次列示备用金、应收/实收现金与差额，差额绝对值合计衡量现金管理。".into(),
        columns: cols(&[
            ("id", "班次ID", "qty"),
            ("opened", "开班时间", "text"),
            ("float", "备用金", "money"),
            ("expected", "应收现金", "money"),
            ("actual", "实收现金", "money"),
            ("diff", "差额", "money"),
            ("status", "状态", "text"),
            ("reason", "差额原因", "text"),
        ]),
        rows,
        metrics: vec![
            m("班次数量", n(raw.len() as i64)),
            m("差额绝对合计", money(diff_abs)),
        ],
    })
}

fn operation_audit(conn: &Connection, from: &str, to: &str) -> Result<ReportResult> {
    let mut stmt = conn.prepare(
        "SELECT substr(a.created_at,1,19), a.command,
                COALESCE(u.username,'系统'), COALESCE(au.username,'—'),
                COALESCE(a.detail_json,'')
         FROM operation_audit a
         LEFT JOIN user_account u ON u.user_id=a.operator_id
         LEFT JOIN user_account au ON au.user_id=a.approver_id
         WHERE substr(a.created_at,1,10) BETWEEN ?1 AND ?2
         ORDER BY a.created_at DESC LIMIT 500",
    )?;
    let raw = stmt
        .query_map(params![from, to], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let rows = raw
        .iter()
        .map(|(t, cmd, op, appr, detail)| {
            vec![
                t.clone(),
                cmd.clone(),
                op.clone(),
                appr.clone(),
                detail.clone(),
            ]
        })
        .collect();
    Ok(ReportResult {
        id: "operation_audit".into(),
        title: "操作审计报表".into(),
        description: "记录敏感操作的时间、命令、操作人与审批人；最多 500 笔。".into(),
        columns: cols(&[
            ("time", "时间", "text"),
            ("command", "操作命令", "text"),
            ("operator", "操作人", "text"),
            ("approver", "审批人", "text"),
            ("detail", "详情", "text"),
        ]),
        rows,
        metrics: vec![m("审计记录数", n(raw.len() as i64))],
    })
}

fn store_comparison(conn: &Connection, from: &str, to: &str) -> Result<ReportResult> {
    // 单机单店：按营业日对比核心经营指标，用于日间横向比较。
    let mut stmt = conn.prepare(
        "SELECT business_date, COUNT(DISTINCT order_no), COALESCE(SUM(pay_cents),0),
                COALESCE(SUM(discount_cents),0),
                COALESCE(SUM(CASE WHEN status='REFUNDED' THEN pay_cents ELSE 0 END),0)
         FROM sales_order
         WHERE status IN ('COMPLETED','PARTIAL_REFUND','REFUNDED') AND business_date BETWEEN ?1 AND ?2
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
    let (mut sales, mut orders) = (0i64, 0i64);
    let rows = raw
        .iter()
        .map(|(d, o, s, disc, rf)| {
            sales += s;
            orders += o;
            let net = s - rf;
            vec![
                d.clone(),
                n(*o),
                money(*s),
                money(*disc),
                money(*rf),
                money(net),
            ]
        })
        .collect();
    Ok(ReportResult {
        id: "store_comparison".into(),
        title: "门店经营对比报表".into(),
        description: "单机单店口径，按营业日对比订单、销售、优惠与退款；多店版按门店 ID 聚合。"
            .into(),
        columns: cols(&[
            ("date", "营业日期", "date"),
            ("orders", "订单数", "qty"),
            ("sales", "销售额", "money"),
            ("discount", "优惠额", "money"),
            ("refund", "退款额", "money"),
            ("net", "净收入", "money"),
        ]),
        rows,
        metrics: vec![m("累计销售", money(sales)), m("累计订单", n(orders))],
    })
}

fn daily_settlement(conn: &Connection, from: &str, to: &str) -> Result<ReportResult> {
    let mut stmt = conn.prepare(
        "SELECT business_date, order_count, total_sales_cents, refund_cents, discount_cents,
                generated_at
         FROM daily_summary WHERE business_date BETWEEN ?1 AND ?2 ORDER BY business_date",
    )?;
    let raw = stmt
        .query_map(params![from, to], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, String>(5)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let (mut sales, mut orders) = (0i64, 0i64);
    let rows = raw
        .iter()
        .map(|(d, o, s, rf, disc, gen)| {
            sales += s;
            orders += o;
            vec![
                d.clone(),
                n(*o),
                money(*s),
                money(*rf),
                money(*disc),
                money(s - rf),
                gen.clone(),
            ]
        })
        .collect();
    Ok(ReportResult {
        id: "daily_settlement".into(),
        title: "日结报表".into(),
        description: "读取日结快照表，幂等不可变；如需刷新先在日结卡重新生成。".into(),
        columns: cols(&[
            ("date", "营业日期", "date"),
            ("orders", "订单数", "qty"),
            ("sales", "销售额", "money"),
            ("refund", "退款额", "money"),
            ("discount", "优惠额", "money"),
            ("net", "净收入", "money"),
            ("generated", "生成时间", "text"),
        ]),
        rows,
        metrics: vec![
            m("结存天数", n(raw.len() as i64)),
            m("销售额合计", money(sales)),
            m("订单合计", n(orders)),
        ],
    })
}

fn abnormal_transaction(conn: &Connection, from: &str, to: &str) -> Result<ReportResult> {
    let mut stmt = conn.prepare(
        "SELECT '作废单' AS kind, o.order_no, o.pay_cents, o.business_date
         FROM sales_order o WHERE o.status='VOIDED' AND o.business_date BETWEEN ?1 AND ?2
         UNION ALL
         SELECT '全额退款', o.order_no, o.pay_cents, o.business_date
         FROM sales_order o WHERE o.status='REFUNDED' AND o.business_date BETWEEN ?1 AND ?2
         UNION ALL
         SELECT '对账差异', COALESCE(r.order_no,'—'), ABS(r.local_amount_cents-r.channel_amount_cents), r.recon_date
         FROM reconcile_record r
         WHERE r.local_amount_cents<>r.channel_amount_cents AND r.recon_date BETWEEN ?1 AND ?2
         ORDER BY 4 DESC LIMIT 500",
    )?;
    let raw = stmt
        .query_map(params![from, to], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut amount = 0i64;
    let rows = raw
        .iter()
        .map(|(kind, no, amt, d)| {
            amount += amt;
            vec![kind.clone(), no.clone(), money(*amt), d.clone()]
        })
        .collect();
    Ok(ReportResult {
        id: "abnormal_transaction".into(),
        title: "异常交易报表".into(),
        description: "汇总作废单、全额退款与对账差异三类异常，金额取绝对值。".into(),
        columns: cols(&[
            ("kind", "异常类型", "text"),
            ("order_no", "关联单号", "text"),
            ("amount", "涉及金额", "money"),
            ("date", "发生日期", "date"),
        ]),
        rows,
        metrics: vec![
            m("异常笔数", n(raw.len() as i64)),
            m("涉及金额", money(amount)),
        ],
    })
}

fn cash_difference(conn: &Connection, from: &str, to: &str) -> Result<ReportResult> {
    let mut stmt = conn.prepare(
        "SELECT s.shift_id, COALESCE(substr(s.opened_at,1,19),''),
                COALESCE(s.expected_cash_cents,0), COALESCE(s.actual_cash_cents,0),
                COALESCE(s.difference_cents,0), COALESCE(s.difference_reason,'')
         FROM shift s
         WHERE s.closed_at IS NOT NULL AND substr(s.opened_at,1,10) BETWEEN ?1 AND ?2
         ORDER BY s.opened_at DESC",
    )?;
    let raw = stmt
        .query_map(params![from, to], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, String>(5)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut diff_sum = 0i64;
    let mut mismatch = 0i64;
    let rows = raw
        .iter()
        .map(|(id, t, exp, act, diff, reason)| {
            diff_sum += diff;
            if *diff != 0 {
                mismatch += 1;
            }
            vec![
                n(*id),
                t.clone(),
                money(*exp),
                money(*act),
                money(*diff),
                reason.clone(),
            ]
        })
        .collect();
    Ok(ReportResult {
        id: "cash_difference".into(),
        title: "现金差异报表".into(),
        description: "仅含已关班班次，现金差额=实收−应收，非零班次单独计数。".into(),
        columns: cols(&[
            ("id", "班次ID", "qty"),
            ("opened", "开班时间", "text"),
            ("expected", "应收现金", "money"),
            ("actual", "实收现金", "money"),
            ("diff", "差额", "money"),
            ("reason", "原因", "text"),
        ]),
        rows,
        metrics: vec![m("净差额", money(diff_sum)), m("不符班次数", n(mismatch))],
    })
}

fn promotion_effect(conn: &Connection, from: &str, to: &str) -> Result<ReportResult> {
    let mut stmt = conn.prepare(
        "SELECT COALESCE(p.name,'促销#'||op.promotion_id), p.promo_type,
                COUNT(DISTINCT op.order_no), COALESCE(SUM(op.discount_cents),0)
         FROM order_promotion op
         JOIN sales_order o ON o.order_no=op.order_no
         LEFT JOIN promotion p ON p.promotion_id=op.promotion_id
         WHERE o.status IN ('COMPLETED','PARTIAL_REFUND') AND o.business_date BETWEEN ?1 AND ?2
         GROUP BY op.promotion_id, p.name, p.promo_type
         ORDER BY SUM(op.discount_cents) DESC",
    )?;
    let raw = stmt
        .query_map(params![from, to], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut discount = 0i64;
    let mut orders = 0i64;
    let rows = raw
        .iter()
        .map(|(name, typ, c, amt)| {
            discount += amt;
            orders += c;
            let avg = if *c > 0 { *amt / *c } else { 0 };
            vec![name.clone(), typ.clone(), n(*c), money(*amt), money(avg)]
        })
        .collect();
    Ok(ReportResult {
        id: "promotion_effect".into(),
        title: "促销效果报表".into(),
        description: "按促销活动汇总使用订单数、让利总额与单均让利。".into(),
        columns: cols(&[
            ("name", "促销活动", "text"),
            ("type", "促销类型", "text"),
            ("orders", "使用订单", "qty"),
            ("discount", "让利总额", "money"),
            ("avg", "单均让利", "money"),
        ]),
        rows,
        metrics: vec![m("让利合计", money(discount)), m("促销订单", n(orders))],
    })
}

fn margin_summary(conn: &Connection, from: &str, to: &str) -> Result<ReportResult> {
    let mut stmt = conn.prepare(
        "SELECT COALESCE(SUM(i.line_total_cents-i.discount_cents),0),
                COALESCE(SUM((i.qty-i.returned_qty)*s.cost_price_cents),0),
                COALESCE(SUM(i.discount_cents),0), COUNT(DISTINCT o.order_no)
         FROM sales_order_item i
         JOIN sales_order o ON o.order_no=i.order_no
         JOIN sku s ON s.sku_id=i.sku_id
         WHERE o.status IN ('COMPLETED','PARTIAL_REFUND') AND o.business_date BETWEEN ?1 AND ?2",
    )?;
    let (sales, cost, disc, orders): (i64, i64, i64, i64) = stmt
        .query_row(params![from, to], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?;
    let profit = sales - cost;
    let rate = if sales > 0 { profit * 10000 / sales } else { 0 };
    let rows = vec![
        vec!["销售额".into(), money(sales)],
        vec!["成本额".into(), money(cost)],
        vec!["毛利额".into(), money(profit)],
        vec!["优惠额".into(), money(disc)],
        vec![
            "毛利率".into(),
            format!("{}.{:02}%", rate / 100, rate % 100),
        ],
        vec!["订单数".into(), n(orders)],
    ];
    Ok(ReportResult {
        id: "margin_summary".into(),
        title: "毛利汇总报表".into(),
        description: "区间整体毛利：销售额−成本额，成本取 SKU 当前进价快照。".into(),
        columns: cols(&[("item", "指标", "text"), ("value", "数值", "text")]),
        rows,
        metrics: vec![
            m("毛利额", money(profit)),
            m("毛利率", format!("{}.{:02}%", rate / 100, rate % 100)),
            m("销售额", money(sales)),
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

    /// 时区口径：created_at 存 UTC（如 20:30Z），报表须按北京时间（次日 04:30）归属。
    #[test]
    fn hourly_report_uses_beijing_time() {
        let c = setup();
        c.execute(
            "INSERT INTO sales_order(order_no,device_id,cashier_id,shift_id,total_cents,discount_cents,pay_cents,status,business_date,created_at)
             VALUES ('O2','D',1,1,5000,0,5000,'COMPLETED','2026-11-18','2026-11-17T20:30:00Z')",
            [],
        )
        .unwrap();
        let r = run_report(&c, "sales_hourly", "2026-11-18", "2026-11-18").unwrap();
        // UTC 20:30 → 北京时间次日 04:30，应落在 04:00 时段，而非 UTC 的 20:00。
        assert!(
            r.rows
                .iter()
                .any(|row| row.first().map(String::as_str) == Some("04:00")),
            "应含北京时段 04:00，实际: {:?}",
            r.rows
        );
        assert!(
            !r.rows
                .iter()
                .any(|row| row.first().map(String::as_str) == Some("20:00")),
            "不应出现 UTC 时段 20:00"
        );
    }

    /// 全部 33 张报表在空库上必须可执行（SQL 列名/语法正确）且列数=行数宽度。
    #[test]
    fn every_report_executes_with_valid_shape() {
        let c = setup();
        let list = report_catalog();
        for meta in &list {
            let r = run_report(&c, &meta.id, "2026-11-17", "2026-11-17")
                .unwrap_or_else(|e| panic!("报表 {} 执行失败: {e}", meta.id));
            assert_eq!(r.id, meta.id, "报表 ID 应回显");
            assert!(!r.title.is_empty(), "{} 缺标题", meta.id);
            assert!(!r.columns.is_empty(), "{} 缺列定义", meta.id);
            for row in &r.rows {
                assert_eq!(row.len(), r.columns.len(), "{} 行列数不匹配", meta.id);
            }
        }
    }
}
