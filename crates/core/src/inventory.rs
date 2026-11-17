//! 库存事件模型（§10.1）：先定义事件，再定义余额。
//! inv_stock 不直接 UPDATE 数字，增减必经 inv_trans 事件投影。

use crate::error::{CoreError, Result};
use crate::time::format_utc;
use chrono::Utc;
use rusqlite::{params, Connection};

#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(non_camel_case_types)] // 事件类型字符串与文档 §10.1 一致
pub enum TransType {
    PURCHASE_IN,
    SALE,
    RETURN,
    VOID,
    PURCHASE_RETURN,
    ADJUST,
    LOSS,
    TRANSFER_IN,
    TRANSFER_OUT,
}

impl TransType {
    pub fn as_str(&self) -> &'static str {
        match self {
            TransType::PURCHASE_IN => "PURCHASE_IN",
            TransType::SALE => "SALE",
            TransType::RETURN => "RETURN",
            TransType::VOID => "VOID",
            TransType::PURCHASE_RETURN => "PURCHASE_RETURN",
            TransType::ADJUST => "ADJUST",
            TransType::LOSS => "LOSS",
            TransType::TRANSFER_IN => "TRANSFER_IN",
            TransType::TRANSFER_OUT => "TRANSFER_OUT",
        }
    }

    pub fn sign(&self) -> i64 {
        match self {
            TransType::SALE
            | TransType::PURCHASE_RETURN
            | TransType::LOSS
            | TransType::TRANSFER_OUT => -1,
            _ => 1,
        }
    }
}

/// 写库存事件并投影更新 inv_stock。必须在事务内调用。
pub fn apply_event(
    tx: &Connection,
    sku_id: i64,
    change_qty: i64,
    trans_type: TransType,
    ref_order_no: Option<&str>,
    operator_id: Option<i64>,
    allow_negative: bool,
) -> Result<i64> {
    let signed = change_qty.abs() * trans_type.sign();
    let current: i64 = tx
        .query_row(
            "SELECT qty FROM inv_stock WHERE sku_id=?1 AND warehouse_type='STORE'",
            [sku_id],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let new_qty = current + signed;
    if new_qty < 0 && !allow_negative {
        return Err(CoreError::Validation(format!(
            "SKU {sku_id} 库存不足: {current} {signed}"
        )));
    }
    tx.execute(
        "INSERT INTO inv_trans(sku_id,warehouse_type,change_qty,trans_type,ref_order_no,operator_id,created_at)
         VALUES (?1,'STORE',?2,?3,?4,?5,?6)",
        params![sku_id, signed, trans_type.as_str(), ref_order_no, operator_id, format_utc(&Utc::now())],
    )?;
    tx.execute(
        "INSERT INTO inv_stock(sku_id,warehouse_type,qty,version) VALUES (?1,'STORE',?2,1)
         ON CONFLICT(sku_id,warehouse_type) DO UPDATE SET qty=excluded.qty, version=inv_stock.version+1",
        params![sku_id, new_qty],
    )?;
    Ok(new_qty)
}

/// 读取 SKU 是否允许负库存（§5.14），默认 1。
pub fn allows_negative(conn: &Connection, sku_id: i64) -> Result<bool> {
    let v: Option<i64> = conn
        .query_row(
            "SELECT allow_negative_stock FROM sku WHERE sku_id=?1",
            [sku_id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(v.unwrap_or(1) != 0)
}

use rusqlite::OptionalExtension;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{open_in_memory, run_migrations};

    fn setup() -> Connection {
        let mut c = open_in_memory().unwrap();
        run_migrations(&mut c).unwrap();
        c.execute(
            "INSERT INTO spu(category_id,spu_name,spu_code) VALUES (1,'测试','SPU1')",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO sku(spu_id,barcode,sale_price_cents) VALUES (1,'6900000000001',1000)",
            [],
        )
        .unwrap();
        c
    }

    #[test]
    fn purchase_in_creates_stock() {
        let c = setup();
        let tx = c.unchecked_transaction().unwrap();
        let q = apply_event(&tx, 1, 100, TransType::PURCHASE_IN, None, None, false).unwrap();
        tx.commit().unwrap();
        assert_eq!(q, 100);
        let n: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM inv_trans WHERE trans_type='PURCHASE_IN'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn sale_decrements_via_event() {
        let c = setup();
        {
            let tx = c.unchecked_transaction().unwrap();
            apply_event(&tx, 1, 100, TransType::PURCHASE_IN, None, None, false).unwrap();
            apply_event(&tx, 1, 30, TransType::SALE, Some("ORD1"), None, false).unwrap();
            tx.commit().unwrap();
        }
        let q: i64 = c
            .query_row("SELECT qty FROM inv_stock WHERE sku_id=1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(q, 70);
    }

    #[test]
    fn negative_stock_blocked_when_disallowed() {
        let c = setup();
        let tx = c.unchecked_transaction().unwrap();
        assert!(apply_event(&tx, 1, 10, TransType::SALE, None, None, false).is_err());
    }

    #[test]
    fn negative_stock_allowed_by_default() {
        let c = setup();
        assert!(allows_negative(&c, 1).unwrap());
        let tx = c.unchecked_transaction().unwrap();
        let q = apply_event(&tx, 1, 10, TransType::SALE, None, None, true).unwrap();
        tx.commit().unwrap();
        assert_eq!(q, -10);
    }

    #[test]
    fn return_void_restore() {
        let c = setup();
        {
            let tx = c.unchecked_transaction().unwrap();
            apply_event(&tx, 1, 100, TransType::PURCHASE_IN, None, None, false).unwrap();
            apply_event(&tx, 1, 50, TransType::SALE, Some("O1"), None, false).unwrap();
            apply_event(&tx, 1, 20, TransType::RETURN, Some("O1"), None, false).unwrap();
            apply_event(&tx, 1, 30, TransType::VOID, Some("O1"), None, false).unwrap();
            tx.commit().unwrap();
        }
        let q: i64 = c
            .query_row("SELECT qty FROM inv_stock WHERE sku_id=1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(q, 100);
    }
}
