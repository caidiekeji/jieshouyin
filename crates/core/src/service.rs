//! 场景桌台（餐饮开台、棋牌包间）。
//!
//! 桌台只承载「当前未结算的购物车」：开台时登记，点单过程由前端把购物车
//! 快照存进 table_session.cart_json，结算成功后清台。桌台不参与金额计算，
//! 结算仍走统一 checkout 服务端定价与支付链路，避免第二套收银口径。

use crate::error::{CoreError, Result};
use crate::time::format_utc;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};

/// 桌台场景：餐饮用 DINING，棋牌用 CHESS。两套桌位互不混用。
pub const SCENE_DINING: &str = "DINING";
pub const SCENE_CHESS: &str = "CHESS";

pub const TABLE_FREE: &str = "FREE";
pub const TABLE_OCCUPIED: &str = "OCCUPIED";
pub const SESSION_OPEN: &str = "OPEN";
pub const SESSION_CLOSED: &str = "CLOSED";

/// 单个桌台未结算购物车 JSON 上限：只防脏数据和超大写入，不替代结算校验。
const CART_JSON_MAX_BYTES: usize = 64 * 1024;
const CART_MAX_LINES: usize = 200;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ServiceTable {
    pub table_id: i64,
    pub code: String,
    pub name: String,
    pub zone: String,
    pub seats: i64,
    pub scene: String,
    pub fee_sku_id: Option<i64>,
    pub status: String,
    pub session_id: Option<i64>,
    pub opened_at: Option<String>,
    pub cart_json: String,
}

fn validate_scene(scene: &str) -> Result<()> {
    if scene != SCENE_DINING && scene != SCENE_CHESS {
        return Err(CoreError::Validation(format!("未知桌台场景: {scene}")));
    }
    Ok(())
}

fn validate_cart(cart_json: &str) -> Result<()> {
    if cart_json.len() > CART_JSON_MAX_BYTES {
        return Err(CoreError::Validation("桌台购物车数据过大".into()));
    }
    let lines: Vec<serde_json::Value> = serde_json::from_str(cart_json)
        .map_err(|_| CoreError::Validation("桌台购物车不是合法 JSON 数组".into()))?;
    if lines.len() > CART_MAX_LINES {
        return Err(CoreError::Validation("桌台购物车行数过多".into()));
    }
    for line in lines {
        let sku_id = line.get("sku_id").and_then(|v| v.as_i64()).unwrap_or(0);
        let qty = line.get("qty").and_then(|v| v.as_i64()).unwrap_or(0);
        if sku_id <= 0 || qty <= 0 {
            return Err(CoreError::Validation("桌台购物车存在非法商品或数量".into()));
        }
    }
    Ok(())
}

/// 新建桌台/包间。编码在同一场景内唯一，房费商品仅棋牌使用。
pub fn create_table(
    conn: &Connection,
    scene: &str,
    code: &str,
    name: &str,
    zone: &str,
    seats: i64,
    fee_sku_id: Option<i64>,
) -> Result<i64> {
    validate_scene(scene)?;
    let code = code.trim();
    let name = name.trim();
    if code.is_empty() || code.chars().count() > 12 {
        return Err(CoreError::Validation("桌台编码必填且不超过 12 字".into()));
    }
    if name.is_empty() || name.chars().count() > 20 {
        return Err(CoreError::Validation("桌台名称必填且不超过 20 字".into()));
    }
    if !(1..=50).contains(&seats) {
        return Err(CoreError::Validation("桌台座位数需在 1–50".into()));
    }
    if let Some(sku_id) = fee_sku_id {
        let exists: i64 =
            conn.query_row("SELECT COUNT(*) FROM sku WHERE sku_id=?1", [sku_id], |r| {
                r.get(0)
            })?;
        if exists == 0 {
            return Err(CoreError::Validation(format!("房费商品 {sku_id} 不存在")));
        }
    }
    let now = format_utc(&Utc::now());
    conn.execute(
        "INSERT INTO service_table(code,scene,name,zone,seats,fee_sku_id,status,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,'FREE',?7)",
        params![code, scene, name, zone.trim(), seats, fee_sku_id, now],
    )
    .map_err(|e| {
        if e.to_string().contains("UNIQUE") {
            CoreError::Validation(format!("桌台编码 {code} 已存在"))
        } else {
            CoreError::Db(e)
        }
    })?;
    Ok(conn.last_insert_rowid())
}

/// 删除桌台：只允许空闲桌台，占用中必须先清台。
pub fn delete_table(conn: &Connection, table_id: i64) -> Result<()> {
    let status: String = conn
        .query_row(
            "SELECT status FROM service_table WHERE table_id=?1",
            [table_id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| CoreError::Validation(format!("桌台 {table_id} 不存在")))?;
    if status != TABLE_FREE {
        return Err(CoreError::Validation("桌台占用中，请先清台".into()));
    }
    conn.execute("DELETE FROM service_table WHERE table_id=?1", [table_id])?;
    Ok(())
}

/// 开台：登记当前购物车（棋牌可带一条房费），同一桌只允许一个 OPEN 会话。
pub fn open_table(conn: &Connection, table_id: i64, cart_json: &str) -> Result<ServiceTable> {
    validate_cart(cart_json)?;
    let status: String = conn
        .query_row(
            "SELECT status FROM service_table WHERE table_id=?1",
            [table_id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| CoreError::Validation(format!("桌台 {table_id} 不存在")))?;
    if status != TABLE_FREE {
        return Err(CoreError::Validation("桌台已开台，请直接点单".into()));
    }
    let now = format_utc(&Utc::now());
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO table_session(table_id,cart_json,opened_at,updated_at,status)
         VALUES (?1,?2,?3,?3,'OPEN')",
        params![table_id, cart_json, now],
    )
    .map_err(|e| {
        if e.to_string().contains("UNIQUE") {
            CoreError::Validation("桌台已开台".into())
        } else {
            CoreError::Db(e)
        }
    })?;
    tx.execute(
        "UPDATE service_table SET status='OCCUPIED' WHERE table_id=?1",
        [table_id],
    )?;
    tx.commit()?;
    table_view(conn, table_id)
}

/// 保存未结算购物车快照；只允许 OPEN 会话，空车也保留（开台后可以尚未点单）。
pub fn save_cart(conn: &Connection, table_id: i64, cart_json: &str) -> Result<()> {
    validate_cart(cart_json)?;
    let changed = conn.execute(
        "UPDATE table_session SET cart_json=?1, updated_at=?2
         WHERE table_id=?3 AND status='OPEN'",
        params![cart_json, format_utc(&Utc::now()), table_id],
    )?;
    if changed == 0 {
        return Err(CoreError::Validation("桌台未开台或已清台".into()));
    }
    Ok(())
}

/// 清台：关闭 OPEN 会话并释放桌台。结算成功后由收银端调用；
/// 未结算强行清台应走作废权限，命令层负责审计。
pub fn close_table(conn: &Connection, table_id: i64) -> Result<()> {
    let now = format_utc(&Utc::now());
    let tx = conn.unchecked_transaction()?;
    let changed = tx.execute(
        "UPDATE table_session SET status='CLOSED', updated_at=?1
         WHERE table_id=?2 AND status='OPEN'",
        params![now, table_id],
    )?;
    if changed == 0 {
        return Err(CoreError::Validation("桌台未开台".into()));
    }
    tx.execute(
        "UPDATE service_table SET status='FREE' WHERE table_id=?1",
        [table_id],
    )?;
    tx.commit()?;
    Ok(())
}

fn table_view(conn: &Connection, table_id: i64) -> Result<ServiceTable> {
    conn.query_row(
        "SELECT t.table_id, t.code, t.name, t.zone, t.seats, t.scene, t.fee_sku_id, t.status,
                s.session_id, s.opened_at, COALESCE(s.cart_json,'[]')
         FROM service_table t
         LEFT JOIN table_session s ON s.table_id=t.table_id AND s.status='OPEN'
         WHERE t.table_id=?1",
        [table_id],
        map_table,
    )
    .optional()?
    .ok_or_else(|| CoreError::Validation(format!("桌台 {table_id} 不存在")))
}

/// 某场景全部桌台，按区域、编码排序。占用中的桌台带上未结算购物车。
pub fn list_tables(conn: &Connection, scene: &str) -> Result<Vec<ServiceTable>> {
    validate_scene(scene)?;
    let mut stmt = conn.prepare(
        "SELECT t.table_id, t.code, t.name, t.zone, t.seats, t.scene, t.fee_sku_id, t.status,
                s.session_id, s.opened_at, COALESCE(s.cart_json,'[]')
         FROM service_table t
         LEFT JOIN table_session s ON s.table_id=t.table_id AND s.status='OPEN'
         WHERE t.scene=?1
         ORDER BY t.zone, t.code",
    )?;
    let rows = stmt
        .query_map([scene], map_table)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn map_table(r: &rusqlite::Row<'_>) -> rusqlite::Result<ServiceTable> {
    Ok(ServiceTable {
        table_id: r.get(0)?,
        code: r.get(1)?,
        name: r.get(2)?,
        zone: r.get(3)?,
        seats: r.get(4)?,
        scene: r.get(5)?,
        fee_sku_id: r.get(6)?,
        status: r.get(7)?,
        session_id: r.get(8)?,
        opened_at: r.get(9)?,
        cart_json: r.get(10)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{open_in_memory, run_migrations};

    fn setup() -> Connection {
        let mut c = open_in_memory().unwrap();
        run_migrations(&mut c).unwrap();
        c
    }

    #[test]
    fn open_save_and_close_table() {
        let c = setup();
        let id = create_table(&c, SCENE_DINING, "A01", "大厅 1 号", "大厅", 4, None).unwrap();
        c.execute(
            "INSERT INTO spu(category_id,spu_name,spu_code,status) VALUES (1,'测试菜','T-1',1)",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO sku(spu_id,barcode,sale_price_cents) VALUES (1,'1001',1000)",
            [],
        )
        .unwrap();
        let t = open_table(&c, id, r#"[{"sku_id":1,"qty":2}]"#).unwrap();
        assert_eq!(t.status, TABLE_OCCUPIED);
        save_cart(&c, id, r#"[{"sku_id":1,"qty":3}]"#).unwrap();
        let t = table_view(&c, id).unwrap();
        assert!(t.cart_json.contains(r#""qty":3"#));
        close_table(&c, id).unwrap();
        let t = table_view(&c, id).unwrap();
        assert_eq!(t.status, TABLE_FREE);
        assert!(t.session_id.is_none());
    }

    #[test]
    fn cannot_open_occupied_or_delete_occupied() {
        let c = setup();
        let id = create_table(&c, SCENE_CHESS, "B01", "包间 1", "二楼", 6, None).unwrap();
        open_table(&c, id, "[]").unwrap();
        assert!(open_table(&c, id, "[]").is_err());
        assert!(delete_table(&c, id).is_err());
        close_table(&c, id).unwrap();
        delete_table(&c, id).unwrap();
    }

    #[test]
    fn cart_rejects_bad_lines_and_duplicate_code() {
        let c = setup();
        let id = create_table(&c, SCENE_DINING, "A01", "大厅 1 号", "大厅", 4, None).unwrap();
        assert!(create_table(&c, SCENE_DINING, "A01", "重复", "大厅", 4, None).is_err());
        assert!(open_table(&c, id, r#"[{"sku_id":0,"qty":1}]"#).is_err());
        assert!(open_table(&c, id, r#"[{"sku_id":1,"qty":0}]"#).is_err());
    }
}
