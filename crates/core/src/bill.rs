//! 渠道账单解析与对账匹配（§6.7 / PAY-05/06）。
//!
//! 微信交易账单 CSV：字段前有反引号 `，解析时剥离；金额是元字符串（可能
//! 带科学计数法陷阱），转分必须按字符串小数位处理，禁止浮点。
//! 支付宝对账单：跳过 # 摘要行，表头行驱动取列；匹配顺序
//! 商家订单号 → 支付宝交易号 → 金额/类型/时间/状态，不能只比单号。
//! 差异四类分级写 reconcile_record，只冻结告警，绝不自动抹平。

use crate::error::{CoreError, Result};
use crate::payment::DiffType;
use rusqlite::Connection;

/// 一条账单行（两类渠道统一抽象）。
#[derive(Debug, Clone, PartialEq)]
pub struct BillRow {
    pub out_trade_no: String,
    pub trade_no: Option<String>,
    pub amount_cents: i64,
    /// true = 退款行（支付宝对账单业务类型=退款；微信交易状态=REFUND）
    pub is_refund: bool,
}

/// 元字符串 → 分（处理科学计数法与多余空格；禁止浮点）。
pub fn yuan_str_to_cents(s: &str) -> Result<i64> {
    let t = s.trim();
    if t.is_empty() {
        return Err(CoreError::Validation("金额为空".into()));
    }
    // 严格字符集：只允许数字、小数点、负号、指数；逗号/正号/货币符号一律拒绝，
    // 避免"1,234.56"被静默解析成 1234.56 或 12.3456。
    if !t
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | 'e' | 'E'))
    {
        return Err(CoreError::Validation(format!("非法金额: {t}")));
    }
    // 科学计数法：按小数点位置手工移位，避免浮点
    let (mantissa, exp) = match t.to_ascii_lowercase().split_once('e') {
        Some((m, e)) => (
            m.to_string(),
            e.parse::<i32>()
                .map_err(|_| CoreError::Validation(format!("非法指数: {t}")))?,
        ),
        None => (t.to_string(), 0),
    };
    let negative = mantissa.starts_with('-');
    let digits: String = mantissa.chars().filter(|c| c.is_ascii_digit()).collect();
    // 小数点后实际位数（不含小数点字符本身）
    let frac_places = match mantissa.find('.') {
        Some(i) => (mantissa.len() as i32) - i as i32 - 1,
        None => 0,
    };
    // 结果单位是分：以两位小数为基准，总移位 = 原小数位 - 指数 - 2
    let frac_len = frac_places - exp - 2;
    let mut value: i64 = digits
        .parse()
        .map_err(|_| CoreError::Validation(format!("非法金额: {t}")))?;
    if frac_len > 0 {
        // 多余小数位截断（账单金额不会出现精度问题之外的位）
        for _ in 0..frac_len {
            value = value.div_euclid(10);
        }
    } else {
        for _ in 0..(-frac_len) {
            value = value.saturating_mul(10);
        }
    }
    Ok(if negative { -value } else { value })
}

/// 微信交易账单 CSV → 行列表。表头驱动取列，兼容成功/全部两种账单。
pub fn parse_wechat_bill(csv: &str) -> Result<Vec<BillRow>> {
    let mut rows = Vec::new();
    let mut cols: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for line in csv.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<String> = line
            .split(',')
            .map(crate::payment::strip_backticks)
            .map(|s| s.to_string())
            .collect();
        if cols.is_empty() {
            // 表头行
            for (i, f) in fields.iter().enumerate() {
                cols.insert(f.clone(), i);
            }
            if !cols.contains_key("商户订单号") {
                cols.clear(); // 不是表头，继续找
            }
            continue;
        }
        // 汇总行不含商户订单号，跳过
        let out_no = col(&fields, &cols, "商户订单号").unwrap_or_default();
        if out_no.is_empty() || !out_no.chars().any(|c| c.is_ascii_alphanumeric()) {
            continue;
        }
        let status = col(&fields, &cols, "交易状态").unwrap_or_default();
        if status.contains("退款") || status == "REFUND" {
            // 退款行也要入列，否则本地退款永远无法与渠道退款核对。
            let refund_amt = col(&fields, &cols, "退款金额")
                .or_else(|| col(&fields, &cols, "申请退款金额"))
                .unwrap_or_default();
            if refund_amt.is_empty() {
                continue;
            }
            rows.push(BillRow {
                out_trade_no: out_no,
                trade_no: col(&fields, &cols, "微信订单号"),
                amount_cents: yuan_str_to_cents(&refund_amt)?,
                is_refund: true,
            });
            continue;
        }
        let amount = col(&fields, &cols, "订单金额")
            .or_else(|| col(&fields, &cols, "应结订单金额"))
            .unwrap_or_default();
        if amount.is_empty() {
            continue;
        }
        rows.push(BillRow {
            out_trade_no: out_no,
            trade_no: col(&fields, &cols, "微信订单号"),
            amount_cents: yuan_str_to_cents(&amount)?,
            is_refund: false,
        });
    }
    if rows.is_empty() && cols.is_empty() {
        return Err(CoreError::Validation("账单缺少表头行".into()));
    }
    Ok(rows)
}

/// 支付宝对账单 CSV → 行列表。跳过 # 摘要行，表头驱动取列。
pub fn parse_alipay_bill(csv: &str) -> Result<Vec<BillRow>> {
    let mut rows = Vec::new();
    let mut cols: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for line in csv.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("--------") {
            continue;
        }
        let fields: Vec<String> = split_csv_line(line);
        if cols.is_empty() {
            for (i, f) in fields.iter().enumerate() {
                cols.insert(f.trim().to_string(), i);
            }
            if !cols
                .keys()
                .any(|k| k.contains("商家订单号") || k.contains("商户订单号"))
            {
                cols.clear();
            }
            continue;
        }
        let out_no = col(&fields, &cols, "商家订单号")
            .or_else(|| col(&fields, &cols, "商户订单号"))
            .unwrap_or_default();
        if out_no.is_empty() {
            continue;
        }
        let biz_type = col(&fields, &cols, "业务类型").unwrap_or_default();
        let amount = col(&fields, &cols, "商家实收")
            .or_else(|| col(&fields, &cols, "订单金额"))
            .unwrap_or_default();
        if amount.is_empty() {
            continue;
        }
        rows.push(BillRow {
            out_trade_no: out_no,
            trade_no: col(&fields, &cols, "支付宝交易号").or_else(|| col(&fields, &cols, "交易号")),
            amount_cents: yuan_str_to_cents(&amount)?,
            is_refund: biz_type.contains("退款"),
        });
    }
    if rows.is_empty() && cols.is_empty() {
        return Err(CoreError::Validation("账单缺少表头行".into()));
    }
    Ok(rows)
}

/// 逗号分割（支持双引号包裹含逗号字段）。
fn split_csv_line(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let chars = line.chars();
    for c in chars {
        match c {
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                out.push(cur.trim_matches('"').to_string());
                cur = String::new();
            }
            _ => cur.push(c),
        }
    }
    out.push(cur.trim_matches('"').to_string());
    out
}

fn col(
    fields: &[String],
    cols: &std::collections::HashMap<String, usize>,
    name: &str,
) -> Option<String> {
    cols.get(name)
        .and_then(|i| fields.get(*i))
        .map(|s| s.trim().to_string())
}

/// 对账汇总。
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct ReconcileSummary {
    pub matched: usize,
    pub missing_remote: usize,
    pub missing_local: usize,
    pub amount_mismatch: usize,
    pub status_mismatch: usize,
}

/// 渠道对账（§6.7）：本地 SUCCESS 支付流水 vs 账单行，差异分级落库。
/// 只写差异告警，绝不自动抹平。幂等：重跑先清当日同渠道差异。
pub fn reconcile_channel(
    conn: &Connection,
    channel: &str,
    recon_date: &str,
    rows: &[BillRow],
) -> Result<ReconcileSummary> {
    let now = chrono::Utc::now().to_rfc3339();
    // 本地流水：order_no, out_trade_no, trade_no, amount_cents
    let mut stmt = conn.prepare(
        "SELECT p.out_trade_no, p.trade_no, p.amount_cents, o.order_no
         FROM payment_record p
         JOIN sales_order o ON o.order_no = p.order_no
         WHERE p.pay_channel = ?1 AND p.status = 'SUCCESS' AND o.business_date = ?2",
    )?;
    struct LocalRow {
        out_trade_no: String,
        trade_no: Option<String>,
        amount: i64,
        order_no: String,
    }
    let locals: Vec<LocalRow> = stmt
        .query_map([channel, recon_date], |r| {
            Ok(LocalRow {
                out_trade_no: r.get::<_, Option<String>>(0)?.unwrap_or_default(),
                trade_no: r.get(1)?,
                amount: r.get(2)?,
                order_no: r.get(3)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    drop(stmt);

    // 本地退款记录（按订单号索引）：退款行必须核对 refund_record，不能只看支付流水。
    struct LocalRefund {
        order_no: String,
        amount: i64,
    }
    let refunds: Vec<LocalRefund> = {
        let mut stmt = conn.prepare(
            "SELECT r.order_no, r.amount_cents
             FROM refund_record r
             JOIN payment_record p ON p.pay_id = r.pay_id
             JOIN sales_order o ON o.order_no = r.order_no
             WHERE p.pay_channel = ?1 AND r.status IN ('PROCESSING','COMPLETED')
               AND o.business_date = ?2",
        )?;
        stmt.query_map([channel, recon_date], |r| {
            Ok(LocalRefund {
                order_no: r.get(0)?,
                amount: r.get(1)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?
    };
    let mut summary = ReconcileSummary::default();
    let mut diffs: Vec<(DiffType, Option<String>, i64, i64, String)> = Vec::new();

    // 本地索引：商户单号 / 渠道交易号
    let by_out: std::collections::HashMap<&str, &LocalRow> = locals
        .iter()
        .filter(|l| !l.out_trade_no.is_empty())
        .map(|l| (l.out_trade_no.as_str(), l))
        .collect();
    let by_trade: std::collections::HashMap<&str, &LocalRow> = locals
        .iter()
        .filter_map(|l| l.trade_no.as_deref().map(|t| (t, l)))
        .collect();

    for row in rows {
        if row.is_refund {
            // 退款行：按原订单号核对 refund_record 金额（渠道退款可能分多笔）
            match refunds
                .iter()
                .find(|r| r.order_no == row.out_trade_no && r.amount == row.amount_cents)
            {
                Some(_) => summary.matched += 1,
                None => {
                    let sum: i64 = refunds
                        .iter()
                        .filter(|r| r.order_no == row.out_trade_no)
                        .map(|r| r.amount)
                        .sum();
                    if sum > 0 {
                        summary.amount_mismatch += 1;
                        diffs.push((
                            DiffType::AMOUNT_MISMATCH,
                            Some(row.out_trade_no.clone()),
                            sum,
                            row.amount_cents,
                            format!("本地退款合计 {sum} vs 渠道退款 {}", row.amount_cents),
                        ));
                    } else {
                        summary.status_mismatch += 1;
                        diffs.push((
                            DiffType::STATUS_MISMATCH,
                            Some(row.out_trade_no.clone()),
                            0,
                            row.amount_cents,
                            format!("渠道退款行无本地退款记录: {}", row.out_trade_no),
                        ));
                    }
                }
            }
            continue;
        }
        let local = by_out
            .get(row.out_trade_no.as_str())
            .or_else(|| row.trade_no.as_deref().and_then(|t| by_trade.get(t)));
        match local {
            Some(l) => {
                if l.amount == row.amount_cents {
                    summary.matched += 1;
                } else {
                    summary.amount_mismatch += 1;
                    diffs.push((
                        DiffType::AMOUNT_MISMATCH,
                        Some(l.order_no.clone()),
                        l.amount,
                        row.amount_cents,
                        format!("本地 {} vs 渠道 {}", l.amount, row.amount_cents),
                    ));
                }
            }
            None => {
                summary.missing_local += 1;
                diffs.push((
                    DiffType::MISSING_LOCAL,
                    None,
                    0,
                    row.amount_cents,
                    format!("渠道有账无本地流水: {}", row.out_trade_no),
                ));
            }
        }
    }

    // 渠道缺失的本地流水（本地收款但账单无）
    let billed: std::collections::HashSet<&str> = rows
        .iter()
        .flat_map(|r| std::iter::once(r.out_trade_no.as_str()).chain(r.trade_no.as_deref()))
        .collect();
    for l in &locals {
        let hit = billed.contains(l.out_trade_no.as_str())
            || l.trade_no
                .as_deref()
                .map(|t| billed.contains(t))
                .unwrap_or(false);
        if !hit {
            summary.missing_remote += 1;
            diffs.push((
                DiffType::MISSING_REMOTE,
                Some(l.order_no.clone()),
                l.amount,
                0,
                format!("本地有账渠道缺失: {}", l.out_trade_no),
            ));
        }
    }

    // 幂等：先清当日同渠道旧差异再写入
    conn.execute(
        "DELETE FROM reconcile_record WHERE recon_date = ?1 AND channel = ?2",
        rusqlite::params![recon_date, channel],
    )?;
    for (t, order_no, local_amt, chan_amt, detail) in diffs {
        conn.execute(
            "INSERT INTO reconcile_record(recon_date, channel, local_amount_cents, channel_amount_cents, diff_type, order_no, detail_json, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            rusqlite::params![
                recon_date,
                channel,
                local_amt,
                chan_amt,
                t.as_str(),
                order_no,
                detail,
                now
            ],
        )?;
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yuan_to_cents_basic() {
        assert_eq!(yuan_str_to_cents("12.34").unwrap(), 1234);
        assert_eq!(yuan_str_to_cents("0.01").unwrap(), 1);
        assert_eq!(yuan_str_to_cents("100").unwrap(), 10000);
        assert_eq!(yuan_str_to_cents("-12.34").unwrap(), -1234);
        assert_eq!(yuan_str_to_cents(" 12.34 ").unwrap(), 1234);
    }

    #[test]
    fn yuan_to_cents_scientific_notation_trap() {
        // §6.7 科学计数法陷阱：1.23E2 = 123.00 元
        assert_eq!(yuan_str_to_cents("1.23E2").unwrap(), 12300);
        assert_eq!(yuan_str_to_cents("1.23e2").unwrap(), 12300);
        assert_eq!(yuan_str_to_cents("1E2").unwrap(), 10000);
    }

    #[test]
    fn wechat_bill_header_driven() {
        let csv = "\u{feff}`交易时间,`公众账号ID,`商户号,`特约商户号,`设备号,`微信订单号,`商户订单号,`用户标识,`交易类型,`交易状态,`付款银行,`货币种类,`应结订单金额,`代金券金额,`退款金额,`商品名称,`商户数据包,`手续费,`费率,`订单金额,`申请退款金额,`费率备注\n\
`2026-11-18 10:00:00,`wx888,`1900001,`0,`POS1,`42000016,`ORD1P1,`ouser,`付款码,`SUCCESS,`OTHERS,`CNY,`9.90,`0.00,`0.00,`商品,``,``0.06,`0.6%,`9.90,`0.00,`成功\n\
`2026-11-18 10:05:00,`wx888,`1900001,`0,`POS1,`42000017,`ORD2P1,`ouser,`付款码,`SUCCESS,`OTHERS,`CNY,`5.00,`0.00,`0.00,`商品,``,``0.03,`0.6%,`5.00,`0.00,`成功\n\
`总交易单数,应结订单总金额,应结订单净金额,代金券总金额,充值券退款金额,2,14.90,14.84,0.00,0.00,14.90,0.00";
        let rows = parse_wechat_bill(csv).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].out_trade_no, "ORD1P1");
        assert_eq!(rows[0].trade_no.as_deref(), Some("42000016"));
        assert_eq!(rows[0].amount_cents, 990);
        assert_eq!(rows[1].amount_cents, 500);
    }

    #[test]
    fn alipay_bill_parses() {
        let csv = "支付宝交易号说明,xxx
# 摘要行
--------结束--------
账单流水号,业务类型,商品名称,创建时间,完成时间,门店编号,门店名称,操作员,终端号,对方账户,订单金额,商家实收,支付宝红包,集分宝,支付宝优惠,商家优惠,券核销金额,券名称,商家红包消费金额,卡消费金额,退款批次号,退款金额,退款成功时间,支付宝交易号,商家订单号,备注
T20261118001,交易,商品,2026-11-18 10:00:00,2026-11-18 10:00:01,,,,,,9.90,9.90,0.00,0.00,0.00,0.00,0.00,,0.00,0.00,,0.00,,20880001,ORD3P1,
T20261118002,退款,商品,2026-11-18 11:00:00,2026-11-18 11:00:01,,,,,,3.00,3.00,0.00,0.00,0.00,0.00,0.00,,0.00,0.00,RT1,3.00,,20880002,ORD3P1,
";
        let rows = parse_alipay_bill(csv).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].out_trade_no, "ORD3P1");
        assert_eq!(rows[0].amount_cents, 990);
        assert!(!rows[0].is_refund);
        assert!(rows[1].is_refund);
    }

    #[test]
    fn yuan_to_cents_rejects_thousands_and_plus() {
        assert!(yuan_str_to_cents("1,234.56").is_err());
        assert!(yuan_str_to_cents("+12.34").is_err());
    }

    #[test]
    fn csv_quoted_comma_field() {
        assert_eq!(
            split_csv_line("a,\"b,c\",d"),
            vec!["a".to_string(), "b,c".to_string(), "d".to_string()]
        );
    }
}
