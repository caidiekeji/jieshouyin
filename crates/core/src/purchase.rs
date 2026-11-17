//! 采购入库与盘点（§10）。验收通过才产生 PURCHASE_IN 事件；
//! 盘点走"任务 → 实盘 → 差异 → 审批 ADJUST"，不覆盖库存。

use crate::error::{CoreError, Result};
use crate::inventory::{apply_event, TransType};
use crate::time::format_utc;
use chrono::Utc;
use rusqlite::{params, Connection};

/// 创建采购单（DRAFT）并写入明细，汇总金额。
pub fn create_purchase_order(
    conn: &Connection,
    po_no: &str,
    supplier_id: i64,
    items: &[(i64, i64, i64)], // (sku_id, ordered_qty, unit_cost_cents)
) -> Result<()> {
    let total: i64 = items.iter().map(|(_, q, c)| q * c).sum();
    conn.execute(
        "INSERT INTO purchase_order(po_no,supplier_id,status,total_cents,created_at) VALUES (?1,?2,'DRAFT',?3,?4)",
        params![po_no, supplier_id, total, format_utc(&Utc::now())],
    )?;
    for (sku, qty, cost) in items {
        conn.execute(
            "INSERT INTO purchase_order_item(po_no,sku_id,ordered_qty,unit_cost_cents) VALUES (?1,?2,?3,?4)",
            params![po_no, sku, qty, cost],
        )?;
    }
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
    let receipt_id = {
        conn.execute(
            "INSERT INTO purchase_receipt(po_no,received_at,operator_id) VALUES (?1,?2,?3)",
            params![po_no, format_utc(&Utc::now()), operator_id],
        )?;
        conn.last_insert_rowid()
    };
    let tx = conn.unchecked_transaction()?;
    for (item_id, got) in received {
        let (sku_id, ordered): (i64, i64) = tx.query_row(
            "SELECT sku_id,ordered_qty FROM purchase_order_item WHERE item_id=?1 AND po_no=?2",
            params![item_id, po_no],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let diff = got - ordered;
        tx.execute(
            "INSERT INTO purchase_receipt_diff(receipt_id,sku_id,ordered_qty,received_qty,diff_qty)
             VALUES (?1,?2,?3,?4,?5)",
            params![receipt_id, sku_id, ordered, got, diff],
        )?;
        tx.execute(
            "UPDATE purchase_order_item SET received_qty=?1 WHERE item_id=?2",
            params![got, item_id],
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
    tx.execute(
        "UPDATE purchase_order SET status='RECEIVED' WHERE po_no=?1",
        [po_no],
    )?;
    tx.commit()?;
    Ok(())
}

/// 盘点：创建任务时写入账面快照。
pub fn create_stocktake(
    conn: &Connection,
    task_no: &str,
    skus: &[i64],
    operator_id: i64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO stocktake(task_no,status,operator_id,created_at) VALUES (?1,'DRAFT',?2,?3)",
        params![task_no, operator_id, format_utc(&Utc::now())],
    )?;
    let id = conn.last_insert_rowid();
    for &sku in skus {
        let book: i64 = conn.query_row(
            "SELECT COALESCE(qty,0) FROM inv_stock WHERE sku_id=?1 AND warehouse_type='STORE'",
            [sku],
            |r| r.get(0),
        )?;
        conn.execute(
            "INSERT INTO stocktake_item(stocktake_id,sku_id,book_qty) VALUES (?1,?2,?3)",
            params![id, sku, book],
        )?;
    }
    Ok(())
}

/// 实盘录入。
pub fn record_count(conn: &Connection, task_no: &str, counts: &[(i64, i64)]) -> Result<()> {
    let task_id: i64 = conn.query_row(
        "SELECT stocktake_id FROM stocktake WHERE task_no=?1",
        [task_no],
        |r| r.get(0),
    )?;
    for (sku, counted) in counts {
        conn.execute(
            "UPDATE stocktake_item SET counted_qty=?1, diff_qty=?1 - book_qty, reason='COUNT'
             WHERE stocktake_id=?2 AND sku_id=?3",
            params![counted, task_id, sku],
        )?;
    }
    conn.execute(
        "UPDATE stocktake SET status='DIFF_REVIEW' WHERE stocktake_id=?1",
        [task_id],
    )?;
    Ok(())
}

/// 审批调账（§10.4）：产生 ADJUST 事件，不覆盖库存。
pub fn approve_stocktake(conn: &Connection, task_no: &str, approver_id: i64) -> Result<()> {
    let task_id: i64 = conn.query_row(
        "SELECT stocktake_id FROM stocktake WHERE task_no=?1",
        [task_no],
        |r| r.get(0),
    )?;
    let items: Vec<(i64, i64, i64)> = {
        let mut stmt = conn.prepare(
            "SELECT sku_id, book_qty, COALESCE(counted_qty, book_qty) FROM stocktake_item WHERE stocktake_id=?1",
        )?;
        let rows = stmt.query_map([task_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        let mut v = Vec::new();
        for row in rows {
            v.push(row?);
        }
        v
    };
    let tx = conn.unchecked_transaction()?;
    for (sku, book, counted) in items {
        let diff = counted - book;
        if diff != 0 {
            apply_event(
                &tx,
                sku,
                diff.abs(),
                TransType::ADJUST,
                Some(task_no),
                Some(approver_id),
                true,
            )?;
        }
    }
    tx.execute(
        "UPDATE stocktake SET status='ADJUSTED', approver_id=?1, finished_at=?2 WHERE stocktake_id=?3",
        params![approver_id, format_utc(&Utc::now()), task_id],
    )?;
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
        create_stocktake(&c, "ST1", &[1], 1).unwrap();
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
