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

/// 业务日（§14.0）：门店本地时间减 cutoff。默认 00:00，即本地日期。
/// 调用方必须先把 UTC 转换到门店时区，再取本地日期与小时传给 `business_date_at`。
pub fn business_date_at(
    local_date: chrono::NaiveDate,
    local_hour: u32,
    cutoff_hour: u32,
) -> String {
    let date = if local_hour < cutoff_hour {
        local_date - chrono::Duration::days(1)
    } else {
        local_date
    };
    date.format("%Y-%m-%d").to_string()
}

/// 兼容入口：调用方保证传入的时间已是门店本地时间。
pub fn business_date(local_dt: &DateTime<Utc>, cutoff_hour: u32) -> String {
    business_date_at(local_dt.date_naive(), local_dt.hour(), cutoff_hour)
}

/// tx A：订单 + 明细落库（PENDING_PAYMENT）。不扣库存、不扣储值。
#[allow(clippy::too_many_arguments)]
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
    // 领域自洽：行合计必须等于订单合计，优惠不得超出合计。
    let lines_total: i64 = lines.iter().map(|l| l.line_total_cents).sum();
    if lines_total != total_cents {
        return Err(CoreError::Money(format!(
            "行合计 {lines_total} 与订单合计 {total_cents} 不一致"
        )));
    }
    if discount_cents < 0 || discount_cents > total_cents {
        return Err(CoreError::Money("优惠金额越界".into()));
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
    let (status, pay_cents): (String, i64) = tx.query_row(
        "SELECT status, pay_cents FROM sales_order WHERE order_no=?1",
        [order_no],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if status != "PENDING_PAYMENT" {
        return Err(CoreError::Validation(format!("订单状态 {status} 不可提交")));
    }
    // 完工不变式：已确认收款必须覆盖应付，否则不得扣库存、不得置 COMPLETED。
    let paid: i64 = tx.query_row(
        "SELECT COALESCE(SUM(amount_cents),0) FROM payment_record
         WHERE order_no=?1 AND status='SUCCESS'",
        [order_no],
        |r| r.get(0),
    )?;
    if paid < pay_cents {
        return Err(CoreError::Money(format!(
            "已确认收款 {paid} 不足应付 {pay_cents}，拒绝完成订单"
        )));
    }
    let items: Vec<(i64, i64)> = {
        // 服务类 SKU 不计库存，销售提交时跳过库存事件。
        let mut stmt = tx.prepare(
            "SELECT i.sku_id, i.qty FROM sales_order_item i
             JOIN sku s ON s.sku_id=i.sku_id
             WHERE i.order_no=?1 AND i.is_gift=0 AND s.is_service=0",
        )?;
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
    let updated = tx.execute(
        "UPDATE sales_order SET status='COMPLETED' WHERE order_no=?1 AND status='PENDING_PAYMENT'",
        [order_no],
    )?;
    if updated != 1 {
        return Err(CoreError::Validation("订单状态已变化，完成失败".into()));
    }
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

/// 作废（§5.5）：只允许未支付订单。已支付订单必须走退货退款，
/// 否则会出现"库存回滚但钱未退、券未回退、流水仍 SUCCESS"的资金断层。
pub fn void_order(tx: &Connection, order_no: &str) -> Result<()> {
    let status: String = tx.query_row(
        "SELECT status FROM sales_order WHERE order_no=?1",
        [order_no],
        |r| r.get(0),
    )?;
    if status != "PENDING_PAYMENT" {
        return Err(CoreError::Validation(
            "已支付订单不能作废，请走退货退款".into(),
        ));
    }
    // 未支付 = 没有任何已确认收款；有 SUCCESS 流水说明钱已到账，只能退款。
    let paid: i64 = tx.query_row(
        "SELECT COALESCE(SUM(amount_cents),0) FROM payment_record
         WHERE order_no=?1 AND status='SUCCESS'",
        [order_no],
        |r| r.get(0),
    )?;
    if paid != 0 {
        return Err(CoreError::Validation(format!(
            "订单已有 {paid} 分已确认收款，不能作废，请走退货退款"
        )));
    }
    let updated = tx.execute(
        "UPDATE sales_order SET status='VOIDED' WHERE order_no=?1 AND status='PENDING_PAYMENT'",
        [order_no],
    )?;
    if updated == 0 {
        return Err(CoreError::Validation("订单状态已变化，作废失败".into()));
    }
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
#[allow(clippy::too_many_arguments)]
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
    if items.len() != refunds.len() {
        return Err(CoreError::Validation(
            "退货明细与退款金额逐行数量不一致".into(),
        ));
    }
    if items.is_empty() {
        return Err(CoreError::Validation("退货明细不能为空".into()));
    }
    let (status, pay_cents): (String, i64) = tx.query_row(
        "SELECT status, pay_cents FROM sales_order WHERE order_no=?1",
        [order_no],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if !matches!(status.as_str(), "COMPLETED" | "PARTIAL_REFUND") {
        return Err(CoreError::Validation(format!("订单状态 {status} 不可退货")));
    }
    // 累计退款（含渠道处理中）不得超过订单实收，避免多次部分退货超退。
    let already: i64 = tx.query_row(
        "SELECT COALESCE(SUM(amount_cents),0) FROM refund_record
         WHERE order_no=?1 AND status IN ('PROCESSING','COMPLETED')",
        [order_no],
        |r| r.get(0),
    )?;
    if already + refund_total > pay_cents {
        return Err(CoreError::Money(format!(
            "累计退款 {} 超过订单实收 {}",
            already + refund_total,
            pay_cents
        )));
    }
    tx.execute(
        "INSERT INTO sales_return(return_no,order_no,member_id,cashier_id,approver_id,refund_total_cents,status,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,'PROCESSING',?7)",
        params![return_no, order_no, member_id, cashier_id, approver_id, refund_total, format_utc(&Utc::now())],
    )?;
    // 退货单先落 PROCESSING：渠道退款执行完毕且无在途再置 COMPLETED，
    // 与 refund_record 的 PROCESSING/COMPLETED 语义保持一致。
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
        let is_service: i64 = tx.query_row(
            "SELECT is_service FROM sku WHERE sku_id=?1",
            [*sku_id],
            |r| r.get(0),
        )?;
        if is_service == 0 {
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
                - COALESCE((SELECT SUM(r.amount_cents)
                            FROM refund_record r
                            JOIN payment_record p2 ON p2.pay_id=r.pay_id
                            JOIN sales_order o2 ON o2.order_no=r.order_no
                            WHERE o2.shift_id=?1 AND o2.cashier_id=o.cashier_id
                              AND p2.pay_channel='CASH' AND r.status='COMPLETED'),0)
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
        // 初始店仓库存行（库存为投影，SKU 建档即建 0 行）
        c.execute(
            "INSERT INTO inv_stock(sku_id,warehouse_type,qty,version) VALUES (1,'STORE',0,1)",
            [],
        )
        .unwrap();
        c
    }

    fn start(c: &Connection, no: &str) {
        start_inner(c, no, true);
    }

    /// 未收款订单（作废场景）：没有 SUCCESS 流水。
    fn start_unpaid(c: &Connection, no: &str) {
        start_inner(c, no, false);
    }

    fn start_inner(c: &Connection, no: &str, paid: bool) {
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
        // 已确认收款：commit_order 现在校验收款覆盖应付
        if paid {
            tx.execute(
                "INSERT INTO payment_record(order_no,pay_channel,pay_mode,attempt_no,amount_cents,out_trade_no,status,created_at)
                 VALUES (?1,'CASH','',1,2000,?2,'SUCCESS',?3)",
                params![no, format!("{no}P1"), "2026-11-17T00:00:00Z"],
            )
            .unwrap();
        }
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
        assert!(cancel_order(&c.unchecked_transaction().unwrap(), "O2").is_err());
    }

    #[test]
    fn pending_order_can_void_and_paid_order_cannot() {
        let c = setup();
        start_unpaid(&c, "O3");
        {
            let tx = c.unchecked_transaction().unwrap();
            void_order(&tx, "O3").unwrap();
            tx.commit().unwrap();
        }
        let status: String = c
            .query_row(
                "SELECT status FROM sales_order WHERE order_no='O3'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "VOIDED");
    }

    #[test]
    fn pending_with_paid_money_cannot_void() {
        let c = setup();
        start(&c, "O6");
        let tx = c.unchecked_transaction().unwrap();
        let err = void_order(&tx, "O6").unwrap_err();
        assert!(err.to_string().contains("已确认收款"));
    }

    #[test]
    fn void_rejects_paid_order_without_touching_stock() {
        let c = setup();
        start(&c, "O4");
        {
            let tx = c.unchecked_transaction().unwrap();
            commit_order(&tx, "O4", 1).unwrap();
            tx.commit().unwrap();
        }
        {
            let tx = c.unchecked_transaction().unwrap();
            let err = void_order(&tx, "O4").unwrap_err();
            assert!(err.to_string().contains("退货"));
        }
        let q: i64 = c
            .query_row("SELECT qty FROM inv_stock WHERE sku_id=1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(q, -2, "已支付订单作废不得回滚库存（保持 commit 后的 -2）");
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
