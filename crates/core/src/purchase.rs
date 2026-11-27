//! 采购入库与盘点（§10）。验收通过才产生 PURCHASE_IN 事件；
//! 盘点走"任务 → 实盘 → 差异 → 审批 ADJUST"，不覆盖库存。

use crate::error::{CoreError, Result};
use crate::inventory::{apply_event, TransType};
use crate::time::format_utc;
use chrono::Utc;
use rusqlite::{params, Connection};
use serde::Serialize;

/// 创建采购单（DRAFT）并写入明细，汇总金额。
pub fn create_purchase_order(
    conn: &Connection,
    po_no: &str,
    supplier_id: i64,
    items: &[(i64, i64, i64)], // (sku_id, ordered_qty, unit_cost_cents)
) -> Result<()> {
    if items.is_empty() {
        return Err(CoreError::Validation("采购单至少需要一条明细".into()));
    }
    // 先校验再写库：负数数量/成本会直接产生负库存与负总额，
    // 溢出则让金额失真（i64 乘法在 release 下静默回绕）。
    let mut total: i64 = 0;
    for (sku, qty, cost) in items {
        if *qty <= 0 {
            return Err(CoreError::Validation(format!(
                "SKU {sku} 订购数量必须大于 0"
            )));
        }
        if *cost < 0 {
            return Err(CoreError::Validation(format!("SKU {sku} 采购单价不能为负")));
        }
        let line = qty
            .checked_mul(*cost)
            .ok_or(CoreError::Money(format!("SKU {sku} 采购金额溢出")))?;
        total = total
            .checked_add(line)
            .ok_or(CoreError::Money("采购单总额溢出".into()))?;
    }
    let supplier_exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM supplier WHERE supplier_id=?1 AND status=1",
        [supplier_id],
        |r| r.get(0),
    )?;
    if supplier_exists == 0 {
        return Err(CoreError::Validation(format!(
            "供应商 {supplier_id} 不存在或已停用"
        )));
    }
    // 表头 + 明细同一事务：中途失败不留半张采购单。
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO purchase_order(po_no,supplier_id,status,total_cents,created_at) VALUES (?1,?2,'DRAFT',?3,?4)",
        params![po_no, supplier_id, total, format_utc(&Utc::now())],
    )?;
    for (sku, qty, cost) in items {
        tx.execute(
            "INSERT INTO purchase_order_item(po_no,sku_id,ordered_qty,unit_cost_cents) VALUES (?1,?2,?3,?4)",
            params![po_no, sku, qty, cost],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// 验收（§10.2）：逐行录实收，差异记 purchase_receipt_diff；
/// 验收通过才产生 PURCHASE_IN 事件。验收前 inv_stock 不变。
pub fn receive_purchase(
    conn: &Connection,
    po_no: &str,
    received: &[(i64, i64)], // (item_id, received_qty)
    operator_id: i64,
) -> Result<()> {
    let status: String = conn.query_row(
        "SELECT status FROM purchase_order WHERE po_no=?1",
        [po_no],
        |r| r.get(0),
    )?;
    if status == "RECEIVED" || status == "CLOSED" {
        return Err(CoreError::Validation("采购单已验收，不可重复验收".into()));
    }
    let tx = conn.unchecked_transaction()?;
    // 收货单、差异、库存事件、状态同一事务：任一步失败都不会留下孤儿收货单。
    let receipt_id = {
        tx.execute(
            "INSERT INTO purchase_receipt(po_no,received_at,operator_id) VALUES (?1,?2,?3)",
            params![po_no, format_utc(&Utc::now()), operator_id],
        )?;
        tx.last_insert_rowid()
    };
    for (item_id, got) in received {
        if *got < 0 {
            return Err(CoreError::Validation("收货数量不能为负".into()));
        }
        let (sku_id, ordered, already): (i64, i64, i64) = tx.query_row(
            "SELECT sku_id,ordered_qty,received_qty FROM purchase_order_item WHERE item_id=?1 AND po_no=?2",
            params![item_id, po_no],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let new_received = already + got;
        let diff = new_received - ordered;
        tx.execute(
            "INSERT INTO purchase_receipt_diff(receipt_id,sku_id,ordered_qty,received_qty,diff_qty)
             VALUES (?1,?2,?3,?4,?5)",
            params![receipt_id, sku_id, ordered, new_received, diff],
        )?;
        tx.execute(
            "UPDATE purchase_order_item SET received_qty=?1 WHERE item_id=?2",
            params![new_received, item_id],
        )?;
        if *got > 0 {
            apply_event(
                &tx,
                sku_id,
                *got,
                TransType::PURCHASE_IN,
                Some(po_no),
                Some(operator_id),
                true,
            )?;
        }
    }
    // 部分收货不得把整单标 RECEIVED，否则剩余数量再也收不进来。
    let remaining: i64 = tx.query_row(
        "SELECT COUNT(*) FROM purchase_order_item
         WHERE po_no=?1 AND received_qty < ordered_qty",
        [po_no],
        |r| r.get(0),
    )?;
    let status = if remaining == 0 {
        "RECEIVED"
    } else {
        "PARTIAL_RECEIVED"
    };
    tx.execute(
        "UPDATE purchase_order SET status=?1 WHERE po_no=?2",
        params![status, po_no],
    )?;
    tx.commit()?;
    Ok(())
}

/// 盘点：创建任务时写入账面快照。
///
/// 账面口径（§盘点规则）= 期初 + 入库 − 出库 − 已确认报损 ± 调整，
/// 等价于 inv_stock.qty（它由 inv_trans 各事件投影而来）。
/// 未入账报损先从账面扣减：再减去 locked_qty（待处理报损冻结量），
/// 让盘点员按「扣除待处理报损后的应有数」去实盘。
pub fn create_stocktake(
    conn: &Connection,
    task_no: &str,
    skus: &[i64],
    operator_id: i64,
    source: &str,
    period: Option<&str>,
) -> Result<()> {
    if skus.is_empty() {
        return Err(CoreError::Validation("盘点商品不能为空".into()));
    }
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO stocktake(task_no,status,operator_id,source,period,created_at)
         VALUES (?1,'DRAFT',?2,?3,?4,?5)",
        params![task_no, operator_id, source, period, format_utc(&Utc::now())],
    )?;
    let id = tx.last_insert_rowid();
    for &sku in skus {
        let book: i64 = tx.query_row(
            "SELECT COALESCE(qty,0) - COALESCE(locked_qty,0)
             FROM inv_stock WHERE sku_id=?1 AND warehouse_type='STORE'",
            [sku],
            |r| r.get(0),
        )?;
        tx.execute(
            "INSERT INTO stocktake_item(stocktake_id,sku_id,book_qty) VALUES (?1,?2,?3)",
            params![id, sku, book],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// 月度盘点的默认范围：在售（spu.status=1）且非服务类（is_service=0）的实物商品，
/// 且有库存余额或被冻结（待处理报损）。只取「在库未售 + 冻结库存」，零库存无冻结的不纳入。
pub fn stocktake_auto_scope(conn: &Connection) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        "SELECT i.sku_id FROM inv_stock i
         JOIN sku s ON s.sku_id=i.sku_id
         JOIN spu p ON p.spu_id=s.spu_id
         WHERE p.status=1 AND s.is_service=0
           AND (COALESCE(i.qty,0) <> 0 OR COALESCE(i.locked_qty,0) <> 0)
         ORDER BY i.sku_id",
    )?;
    let rows = stmt.query_map([], |r| r.get::<_, i64>(0))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// 账面构成分解（供明细核对）：按 inv_trans 事件类型分项汇总，
/// 期初(PURCHASE_IN/ref=INITIAL) + 入库(PURCHASE_IN) − 出库(SALE/RETURN/PURCHASE_RETURN/TRANSFER_OUT)
/// − 已确认报损(LOSS) ± 调整(ADJUST/VOID/TRANSFER_IN)，再减去待处理报损冻结 locked_qty。
#[derive(Debug, Serialize)]
pub struct BookBreakdown {
    pub opening: i64,
    pub purchase_in: i64,
    pub outbound: i64,
    pub confirmed_loss: i64,
    pub adjustment: i64,
    pub pending_loss: i64,
    pub book_qty: i64,
}

pub fn book_breakdown(conn: &Connection, sku_id: i64) -> Result<BookBreakdown> {
    let mut opening = 0i64;
    let mut purchase_in = 0i64;
    let mut outbound = 0i64;
    let mut confirmed_loss = 0i64;
    let mut adjustment = 0i64;
    let mut stmt = conn.prepare(
        "SELECT trans_type, COALESCE(ref_order_no,''), change_qty FROM inv_trans
         WHERE sku_id=?1 AND warehouse_type='STORE'",
    )?;
    let rows = stmt.query_map([sku_id], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?))
    })?;
    for row in rows {
        let (kind, reference, qty) = row?;
        match kind.as_str() {
            crate::constants::TRANS_PURCHASE_IN if reference == crate::constants::TRANS_INITIAL => {
                opening += qty
            }
            crate::constants::TRANS_PURCHASE_IN => purchase_in += qty,
            crate::constants::TRANS_SALE
            | crate::constants::TRANS_RETURN
            | crate::constants::TRANS_PURCHASE_RETURN
            | crate::constants::TRANS_TRANSFER_OUT => outbound += qty,
            crate::constants::TRANS_LOSS => confirmed_loss += qty,
            crate::constants::TRANS_ADJUST
            | crate::constants::TRANS_VOID
            | crate::constants::TRANS_TRANSFER_IN => adjustment += qty,
            _ => adjustment += qty,
        }
    }
    let pending_loss: i64 = conn.query_row(
        "SELECT COALESCE(locked_qty,0) FROM inv_stock WHERE sku_id=?1 AND warehouse_type='STORE'",
        [sku_id],
        |r| r.get(0),
    )?;
    // outbound / confirmed_loss 流水本身已是负数，直接相加即为「扣减后」结果。
    let book_qty = opening + purchase_in + outbound + confirmed_loss + adjustment - pending_loss;
    Ok(BookBreakdown {
        opening,
        purchase_in,
        outbound,
        confirmed_loss,
        adjustment,
        pending_loss,
        book_qty,
    })
}

/// 实盘录入。仅 DRAFT / DIFF_REVIEW 可改数；已审批任务不得回写。
pub fn record_count(conn: &Connection, task_no: &str, counts: &[(i64, i64)]) -> Result<()> {
    let (task_id, status): (i64, String) = conn.query_row(
        "SELECT stocktake_id, status FROM stocktake WHERE task_no=?1",
        [task_no],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if !matches!(status.as_str(), "DRAFT" | "DIFF_REVIEW") {
        return Err(CoreError::Validation(format!(
            "盘点单状态 {status} 不可录入实盘数"
        )));
    }
    let tx = conn.unchecked_transaction()?;
    for (sku, counted) in counts {
        let affected = tx.execute(
            "UPDATE stocktake_item SET counted_qty=?1, diff_qty=?1 - book_qty, reason='COUNT'
             WHERE stocktake_id=?2 AND sku_id=?3",
            params![counted, task_id, sku],
        )?;
        if affected == 0 {
            return Err(CoreError::Validation(format!(
                "盘点单不含 SKU {sku}，拒绝录入"
            )));
        }
    }
    tx.execute(
        "UPDATE stocktake SET status='DIFF_REVIEW' WHERE stocktake_id=?1",
        [task_id],
    )?;
    tx.commit()?;
    Ok(())
}

/// 审批调账（§10.4）：产生 ADJUST 事件，不覆盖库存。
/// 只有 DIFF_REVIEW 可审批；ADJUSTED 重复审批必须报错（否则重复调账）。
pub fn approve_stocktake(conn: &Connection, task_no: &str, approver_id: i64) -> Result<()> {
    let (task_id, status): (i64, String) = conn.query_row(
        "SELECT stocktake_id, status FROM stocktake WHERE task_no=?1",
        [task_no],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if status != "DIFF_REVIEW" {
        return Err(CoreError::Validation(format!(
            "盘点单状态 {status} 不可审批（需先录入实盘数）"
        )));
    }
    let items: Vec<(i64, i64)> = {
        let mut stmt = conn.prepare(
            "SELECT sku_id, COALESCE(counted_qty, book_qty) FROM stocktake_item WHERE stocktake_id=?1",
        )?;
        let rows = stmt.query_map([task_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        let mut v = Vec::new();
        for row in rows {
            v.push(row?);
        }
        v
    };
    let tx = conn.unchecked_transaction()?;
    for (sku, counted) in items {
        // 调账基准必须是审批时的实时库存，而不是建单快照：
        // 盘点期间发生的销售/入库若按快照调账，会把这段时间的变动再抹一遍。
        let current: i64 = tx.query_row(
            "SELECT COALESCE(qty,0) FROM inv_stock WHERE sku_id=?1 AND warehouse_type='STORE'",
            [sku],
            |r| r.get(0),
        )?;
        let diff = counted - current;
        if diff != 0 {
            apply_event(
                &tx,
                sku,
                diff, // 带符号差值：盘盈为正、盘亏为负
                TransType::ADJUST,
                Some(task_no),
                Some(approver_id),
                true,
            )?;
        }
        // 覆盖快照差异为审批实际差异，供审计复核（book_qty 保留建单快照）。
        tx.execute(
            "UPDATE stocktake_item SET diff_qty=?1 WHERE stocktake_id=?2 AND sku_id=?3",
            params![diff, task_id, sku],
        )?;
    }
    let affected = tx.execute(
        "UPDATE stocktake SET status='ADJUSTED', approver_id=?1, finished_at=?2
         WHERE stocktake_id=?3 AND status='DIFF_REVIEW'",
        params![approver_id, format_utc(&Utc::now()), task_id],
    )?;
    if affected != 1 {
        return Err(CoreError::Validation("盘点单已被处理，拒绝重复审批".into()));
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{open_in_memory, run_migrations};

    fn setup() -> Connection {
        let mut c = open_in_memory().unwrap();
        run_migrations(&mut c).unwrap();
        c.execute("INSERT INTO supplier(name) VALUES ('供应商1')", [])
            .unwrap();
        c.execute(
            "INSERT INTO spu(category_id,spu_name,spu_code) VALUES (1,'商品','S1')",
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

    #[test]
    fn purchase_creates_stock_only_on_receive() {
        let c = setup();
        create_purchase_order(&c, "PO1", 1, &[(1, 100, 500)]).unwrap();
        let q: i64 = c
            .query_row(
                "SELECT COALESCE(qty,0) FROM inv_stock WHERE sku_id=1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(q, 0, "未验收不影响库存");
        let item: i64 = c
            .query_row(
                "SELECT item_id FROM purchase_order_item WHERE po_no='PO1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        receive_purchase(&c, "PO1", &[(item, 100)], 1).unwrap();
        let q: i64 = c
            .query_row("SELECT qty FROM inv_stock WHERE sku_id=1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(q, 100);
    }

    #[test]
    fn receipt_diff_recorded() {
        let c = setup();
        create_purchase_order(&c, "PO2", 1, &[(1, 100, 500)]).unwrap();
        let item: i64 = c
            .query_row(
                "SELECT item_id FROM purchase_order_item WHERE po_no='PO2'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        receive_purchase(&c, "PO2", &[(item, 90)], 1).unwrap();
        let diff: i64 = c
            .query_row("SELECT diff_qty FROM purchase_receipt_diff", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(diff, -10);
    }

    #[test]
    fn double_receive_rejected() {
        let c = setup();
        create_purchase_order(&c, "PO3", 1, &[(1, 10, 500)]).unwrap();
        let item: i64 = c
            .query_row(
                "SELECT item_id FROM purchase_order_item WHERE po_no='PO3'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        receive_purchase(&c, "PO3", &[(item, 10)], 1).unwrap();
        assert!(receive_purchase(&c, "PO3", &[(item, 10)], 1).is_err());
    }

    #[test]
    fn stocktake_adjust_via_event() {
        let c = setup();
        {
            let tx = c.unchecked_transaction().unwrap();
            apply_event(&tx, 1, 100, TransType::PURCHASE_IN, None, None, true).unwrap();
            tx.commit().unwrap();
        }
        create_stocktake(&c, "ST1", &[1], 1, crate::constants::STOCKTAKE_SOURCE_MANUAL, None).unwrap();
        let book: i64 = c
            .query_row("SELECT book_qty FROM stocktake_item", [], |r| r.get(0))
            .unwrap();
        assert_eq!(book, 100);
        record_count(&c, "ST1", &[(1, 95)]).unwrap();
        approve_stocktake(&c, "ST1", 2).unwrap();
        let q: i64 = c
            .query_row("SELECT qty FROM inv_stock WHERE sku_id=1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(q, 95);
        let t: String = c
            .query_row(
                "SELECT trans_type FROM inv_trans WHERE ref_order_no='ST1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(t, "ADJUST");
    }
}
