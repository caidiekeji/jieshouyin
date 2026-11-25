//! 库存事件模型（§10.1）：先定义事件，再定义余额。
//! inv_stock 不直接 UPDATE 数字，增减必经 inv_trans 事件投影。

use crate::constants::{
    TRANS_ADJUST, TRANS_LOSS, TRANS_PURCHASE_IN, TRANS_PURCHASE_RETURN, TRANS_RETURN, TRANS_SALE,
    TRANS_TRANSFER_IN, TRANS_TRANSFER_OUT, TRANS_VOID,
};
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
            TransType::PURCHASE_IN => TRANS_PURCHASE_IN,
            TransType::SALE => TRANS_SALE,
            TransType::RETURN => TRANS_RETURN,
            TransType::VOID => TRANS_VOID,
            TransType::PURCHASE_RETURN => TRANS_PURCHASE_RETURN,
            TransType::ADJUST => TRANS_ADJUST,
            TransType::LOSS => TRANS_LOSS,
            TransType::TRANSFER_IN => TRANS_TRANSFER_IN,
            TransType::TRANSFER_OUT => TRANS_TRANSFER_OUT,
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
    // change_qty 为带符号变化量（正入负出），乘以事件类型的固有方向。
    // ADJUST 等中性类型符号为 +1，由调用方传入带符号差值（盘盈 + / 盘亏 -）。
    let signed = change_qty * trans_type.sign();
    let allow_negative = allow_negative && allows_negative(tx, sku_id)?;
    if !allow_negative {
        // 预检只为给出可读错误；实际投影用原子累加，避免并发丢更新。
        let current: i64 = tx
            .query_row(
                "SELECT qty FROM inv_stock WHERE sku_id=?1 AND warehouse_type='STORE'",
                [sku_id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if current + signed < 0 {
            let name: String = tx
                .query_row(
                    "SELECT p.spu_name FROM sku s JOIN spu p ON p.spu_id=s.spu_id WHERE s.sku_id=?1",
                    [sku_id],
                    |r| r.get(0),
                )
                .unwrap_or_else(|_| format!("SKU {sku_id}"));
            return Err(CoreError::Validation(format!(
                "{} 库存不足：现有 {current}，需 {signed}",
                name
            )));
        }
    }
    tx.execute(
        "INSERT INTO inv_trans(sku_id,warehouse_type,change_qty,trans_type,ref_order_no,operator_id,created_at)
         VALUES (?1,'STORE',?2,?3,?4,?5,?6)",
        params![sku_id, signed, trans_type.as_str(), ref_order_no, operator_id, format_utc(&Utc::now())],
    )?;
    // 原子累加（不是读-算-写）：多连接/多进程下也不会互相覆盖。
    let new_qty: i64 = tx.query_row(
        "INSERT INTO inv_stock(sku_id,warehouse_type,qty,version) VALUES (?1,'STORE',?2,1)
         ON CONFLICT(sku_id,warehouse_type) DO UPDATE
         SET qty=inv_stock.qty+excluded.qty, version=inv_stock.version+1
         RETURNING qty",
        params![sku_id, signed],
        |r| r.get(0),
    )?;
    Ok(new_qty)
}

/// 读取 SKU 是否允许负库存（§5.14）。SKU 不存在直接报错，避免未知 SKU 凭空建库存行。
pub fn allows_negative(conn: &Connection, sku_id: i64) -> Result<bool> {
    let v: Option<i64> = conn
        .query_row(
            "SELECT allow_negative_stock FROM sku WHERE sku_id=?1",
            [sku_id],
            |r| r.get(0),
        )
        .optional()?;
    let sku_allow = v
        .map(|x| x != 0)
        .ok_or_else(|| CoreError::Validation(format!("SKU {sku_id} 不存在")))?;
    // 全局开关是总闸：门店关闭负库存后，SKU 级允许也不生效（否则设置页是假开关）。
    let global: Option<String> = conn
        .query_row(
            "SELECT config_value FROM system_config WHERE config_key=?1",
            [crate::constants::CFG_ALLOW_NEGATIVE_STOCK],
            |r| r.get(0),
        )
        .optional()?;
    let global_allow = global.as_deref().unwrap_or("0") != "0";
    Ok(sku_allow && global_allow)
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
