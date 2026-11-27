//! 报损单（M0021）：待处理 → 已确认 / 已取消。
//!
//! 设计依据：报损不能一录就扣库存——录入与实物处置之间常有间隔，
//! 这段间隔内库存应被「冻结」而非消失。故：
//!   - 建单 PENDING：只占 locked_qty（冻结），inv_stock.qty 与 inv_trans 不变；
//!   - 确认 CONFIRMED：写 LOSS 事件真正扣 qty，并释放对应 locked_qty；
//!   - 取消 CANCELLED：释放 locked_qty，库存与流水都不变。
//! 有效可售 = qty - locked_qty；建单校验的是这个口径，避免同一批货被重复报损。

use crate::constants::{LOSS_CANCELLED, LOSS_CONFIRMED, LOSS_PENDING};
use crate::error::{CoreError, Result};
use crate::inventory::{apply_event, TransType};
use crate::time::format_utc;
use chrono::Utc;
use rusqlite::{params, Connection};

/// 创建报损单（PENDING），占用冻结库存。
/// items 为 (sku_id, qty, reason)；同一 SKU 多行会合并校验占用量。
pub fn create_loss(
    conn: &Connection,
    loss_no: &str,
    items: &[(i64, i64, Option<String>)],
    operator_id: i64,
    reason: Option<&str>,
) -> Result<()> {
    if items.is_empty() {
        return Err(CoreError::Validation("报损单至少需要一条明细".into()));
    }
    // 合并同 SKU 的报损量，防止单行拆分绕过冻结校验。
    let mut merged: std::collections::BTreeMap<i64, i64> = std::collections::BTreeMap::new();
    for (sku, qty, _) in items {
        if *qty <= 0 {
            return Err(CoreError::Validation(format!(
                "SKU {sku} 报损数量必须大于 0"
            )));
        }
        let entry = merged.entry(*sku).or_insert(0);
        *entry = entry
            .checked_add(*qty)
            .ok_or_else(|| CoreError::Validation(format!("SKU {sku} 报损数量溢出")))?;
    }
    let tx = conn.unchecked_transaction()?;
    // 按合并总量校验：同一 SKU 拆成多行时，逐行校验会各自通过却累计超库存。
    for (sku, total_qty) in &merged {
        let (qty_on_hand, locked): (i64, i64) = tx.query_row(
            "SELECT COALESCE(qty,0), COALESCE(locked_qty,0)
             FROM inv_stock WHERE sku_id=?1 AND warehouse_type='STORE'",
            [sku],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let available = qty_on_hand - locked;
        if available < *total_qty {
            return Err(CoreError::Validation(format!(
                "SKU {sku} 可报损库存不足：现有 {qty_on_hand}，已冻结 {locked}，本单合计需 {total_qty}"
            )));
        }
    }
    tx.execute(
        "INSERT INTO loss_order(loss_no,status,reason,operator_id,created_at)
         VALUES (?1,?2,?3,?4,?5)",
        params![
            loss_no,
            LOSS_PENDING,
            reason,
            operator_id,
            format_utc(&Utc::now())
        ],
    )?;
    let loss_id = tx.last_insert_rowid();
    for (sku, qty, line_reason) in items {
        tx.execute(
            "INSERT INTO loss_order_item(loss_id,sku_id,qty,reason) VALUES (?1,?2,?3,?4)",
            params![loss_id, sku, qty, line_reason],
        )?;
        // 冻结：inv_stock 行已存在（上面 query_row 成功），直接累加。
        tx.execute(
            "UPDATE inv_stock SET locked_qty=locked_qty+?1, version=version+1
             WHERE sku_id=?2 AND warehouse_type='STORE'",
            params![qty, sku],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// 确认报损：写 LOSS 事件真正扣减库存，释放冻结。仅 PENDING 可确认。
pub fn confirm_loss(conn: &Connection, loss_no: &str, confirmer_id: i64) -> Result<()> {
    let (loss_id, status): (i64, String) = conn.query_row(
        "SELECT loss_id, status FROM loss_order WHERE loss_no=?1",
        [loss_no],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if status != LOSS_PENDING {
        return Err(CoreError::Validation(format!(
            "报损单状态 {status} 不可确认（仅待处理可确认）"
        )));
    }
    let items: Vec<(i64, i64)> = {
        let mut stmt = conn.prepare("SELECT sku_id, qty FROM loss_order_item WHERE loss_id=?1")?;
        let rows = stmt.query_map([loss_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        let mut v = Vec::new();
        for row in rows {
            v.push(row?);
        }
        v
    };
    let tx = conn.unchecked_transaction()?;
    for (sku, qty) in &items {
        // LOSS 事件 sign=-1，传正数即扣减；allow_negative 交由 SKU/全局开关裁决，
        // 建单时已校验有效库存，正常路径不会为负。
        apply_event(
            &tx,
            *sku,
            *qty,
            TransType::LOSS,
            Some(loss_no),
            Some(confirmer_id),
            true,
        )?;
        // 释放冻结：防止 locked_qty 因重复确认被扣成负数。
        tx.execute(
            "UPDATE inv_stock SET locked_qty=MAX(locked_qty-?1,0), version=version+1
             WHERE sku_id=?2 AND warehouse_type='STORE'",
            params![qty, sku],
        )?;
    }
    let affected = tx.execute(
        "UPDATE loss_order SET status=?1, confirmer_id=?2, confirmed_at=?3
         WHERE loss_id=?4 AND status=?5",
        params![
            LOSS_CONFIRMED,
            confirmer_id,
            format_utc(&Utc::now()),
            loss_id,
            LOSS_PENDING
        ],
    )?;
    if affected != 1 {
        return Err(CoreError::Validation("报损单已被处理，拒绝重复确认".into()));
    }
    tx.commit()?;
    Ok(())
}

/// 取消报损：释放冻结，不写库存流水。仅 PENDING 可取消。
pub fn cancel_loss(conn: &Connection, loss_no: &str) -> Result<()> {
    let (loss_id, status): (i64, String) = conn.query_row(
        "SELECT loss_id, status FROM loss_order WHERE loss_no=?1",
        [loss_no],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if status != LOSS_PENDING {
        return Err(CoreError::Validation(format!(
            "报损单状态 {status} 不可取消（仅待处理可取消）"
        )));
    }
    let items: Vec<(i64, i64)> = {
        let mut stmt = conn.prepare("SELECT sku_id, qty FROM loss_order_item WHERE loss_id=?1")?;
        let rows = stmt.query_map([loss_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        let mut v = Vec::new();
        for row in rows {
            v.push(row?);
        }
        v
    };
    let tx = conn.unchecked_transaction()?;
    for (sku, qty) in &items {
        tx.execute(
            "UPDATE inv_stock SET locked_qty=MAX(locked_qty-?1,0), version=version+1
             WHERE sku_id=?2 AND warehouse_type='STORE'",
            params![qty, sku],
        )?;
    }
    let affected = tx.execute(
        "UPDATE loss_order SET status=?1 WHERE loss_id=?2 AND status=?3",
        params![LOSS_CANCELLED, loss_id, LOSS_PENDING],
    )?;
    if affected != 1 {
        return Err(CoreError::Validation("报损单已被处理，拒绝重复取消".into()));
    }
    tx.commit()?;
    Ok(())
}
