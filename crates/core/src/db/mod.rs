//! 数据库层：连接初始化（WAL）、Schema Migration（§11）、种子数据。

use crate::error::{CoreError, Result};
use rusqlite::{params, Connection};
use std::path::Path;

/// 打开数据库连接并启用 WAL 模式。启动顺序第一步。
pub fn open_connection(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;
         PRAGMA busy_timeout = 5000;",
    )?;
    Ok(conn)
}

/// 打开内存库（供测试使用，同样启用外键）。
pub fn open_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    Ok(conn)
}

/// 迁移定义。只向前不回头；down 仅开发调试用。
pub struct Migration {
    pub version: i32,
    pub name: &'static str,
    pub up: &'static str,
}

/// 按版本顺序执行未应用迁移，每个迁移独立事务。
pub fn run_migrations(conn: &mut Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            name    TEXT NOT NULL,
            applied_at TEXT NOT NULL
         );",
    )?;

    let applied: Vec<i32> = {
        let mut stmt = conn.prepare("SELECT version FROM schema_migrations ORDER BY version")?;
        let rows = stmt.query_map([], |r| r.get::<_, i32>(0))?;
        let mut v = Vec::new();
        for row in rows {
            v.push(row?);
        }
        v
    };

    for m in migrations::all() {
        if applied.contains(&m.version) {
            continue;
        }
        let tx = conn.transaction()?;
        tx.execute_batch(m.up)?;
        tx.execute(
            "INSERT INTO schema_migrations(version, name, applied_at) VALUES (?1, ?2, ?3)",
            params![m.version, m.name, chrono::Utc::now().to_rfc3339()],
        )?;
        tx.commit()?;
        tracing::info!(version = m.version, name = m.name, "migration applied");
    }
    Ok(())
}

/// 校验备份 schema 版本兼容性：备份版本 ≤ 当前版本方可恢复。
pub fn check_backup_compatibility(conn: &Connection, backup_version: i32) -> Result<()> {
    let current: i32 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    if backup_version > current {
        return Err(CoreError::Validation(format!(
            "备份 schema 版本 {backup_version} 高于当前版本 {current}，拒绝恢复"
        )));
    }
    Ok(())
}

pub mod migrations;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_db_applies_all_migrations() {
        let mut conn = open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();
        let count: i32 = conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 6);
        let max: i32 = conn
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(max, 6);
    }

    #[test]
    fn migrations_are_idempotent() {
        let mut conn = open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();
        run_migrations(&mut conn).unwrap();
        let count: i32 = conn
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 6);
    }

    #[test]
    fn all_spec_tables_exist() {
        let mut conn = open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();
        let tables = [
            "spu",
            "sku",
            "inv_stock",
            "inv_trans",
            "sales_order",
            "sales_order_item",
            "parked_order",
            "sales_return",
            "sales_return_item",
            "shift",
            "sync_queue",
            "system_config",
            "daily_summary",
            "license_local",
            "license_audit",
            "license_state",
            "role",
            "permission",
            "role_permission",
            "role_limit",
            "user_account",
            "operation_audit",
            "member",
            "member_asset",
            "member_asset_log",
            "coupon",
            "payment_record",
            "refund_record",
            "reconcile_record",
            "platform_cert",
            "supplier",
            "purchase_order",
            "purchase_order_item",
            "purchase_receipt",
            "purchase_receipt_diff",
            "stocktake",
            "stocktake_item",
            "promotion",
            "order_promotion",
            "master_changelog",
            "sync_watermark",
            "print_template",
            "print_retry_queue",
        ];
        for t in tables {
            let n: i32 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [t],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "缺少表 {t}");
        }
    }
}
