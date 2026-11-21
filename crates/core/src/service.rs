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
/// 棋牌最低计费时长：不足 1 小时按 1 小时，超出按分钟实算。
pub const CHESS_MIN_BILLABLE_MINUTES: i64 = 60;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ServiceTable {
    pub table_id: i64,
    pub code: String,
    pub name: String,
    pub zone: String,
    pub seats: i64,
    pub scene: String,
    pub fee_sku_id: Option<i64>,
    /// 棋牌计费覆盖：MINUTE / HOUR；空表示跟随门店配置。
    pub fee_unit: Option<String>,
    /// 最低计费分钟覆盖；空表示跟随门店配置。
    pub min_minutes: Option<i64>,
    /// 超时取整分钟覆盖；空表示跟随门店配置。
    pub round_minutes: Option<i64>,
    pub status: String,
    pub session_id: Option<i64>,
    pub opened_at: Option<String>,
    pub member_id: Option<i64>,
    pub member_phone: Option<String>,
    pub cart_json: String,
    /// 已下发后厨的购物车快照，用于只打印新增菜品。
    pub sent_json: String,
}

/// 后厨待出品行：submit_order 对比已下单快照后返回本次新增部分。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KitchenItem {
    pub sku_id: i64,
    pub category_id: i64,
    pub name: String,
    pub unit: String,
    pub qty: i64,
}

/// 棋牌计时：房费 SKU 按“每分钟单价”维护，最低 60 分钟，超出按分钟实算。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ChessFee {
    pub elapsed_minutes: i64,
    pub billable_minutes: i64,
    /// 房费 SKU 的原价：MINUTE 为每分钟单价，HOUR 为每小时单价。
    pub unit_cents: i64,
    pub fee_unit: String,
    pub fee_cents: i64,
}

fn cfg_text(conn: &Connection, key: &str, default: &str) -> String {
    conn.query_row(
        "SELECT config_value FROM system_config WHERE config_key=?1",
        [key],
        |r| r.get::<_, String>(0),
    )
    .optional()
    .ok()
    .flatten()
    .filter(|v| !v.trim().is_empty())
    .unwrap_or_else(|| default.to_string())
}

fn cfg_i64(conn: &Connection, key: &str, default: i64) -> i64 {
    cfg_text(conn, key, &default.to_string())
        .trim()
        .parse::<i64>()
        .unwrap_or(default)
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
    fee_unit: Option<&str>,
    min_minutes: Option<i64>,
    round_minutes: Option<i64>,
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
    if let Some(unit) = fee_unit {
        if unit != "MINUTE" && unit != "HOUR" {
            return Err(CoreError::Validation(
                "棋牌计费单位只能是 MINUTE 或 HOUR".into(),
            ));
        }
    }
    if let Some(min) = min_minutes {
        if !(0..=1440).contains(&min) {
            return Err(CoreError::Validation("最低计费分钟需在 0–1440".into()));
        }
    }
    if let Some(round) = round_minutes {
        if !(0..=120).contains(&round) {
            return Err(CoreError::Validation("取整分钟需在 0–120".into()));
        }
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
        "INSERT INTO service_table(code,scene,name,zone,seats,fee_sku_id,fee_unit,min_minutes,round_minutes,status,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'FREE',?10)",
        params![
            code,
            scene,
            name,
            zone.trim(),
            seats,
            fee_sku_id,
            fee_unit,
            min_minutes,
            round_minutes,
            now
        ],
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
pub fn open_table(
    conn: &Connection,
    table_id: i64,
    cart_json: &str,
    member_id: Option<i64>,
) -> Result<ServiceTable> {
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
    let bind_mode = cfg_text(conn, crate::constants::CFG_TABLE_MEMBER_BIND, "OPTIONAL");
    if member_id.is_none() && bind_mode == "REQUIRED" {
        return Err(CoreError::Validation(
            "门店设置为开台必须绑定会员，请先识别会员".into(),
        ));
    }
    if let Some(mid) = member_id {
        let exists: i64 = conn.query_row(
            "SELECT COUNT(*) FROM member WHERE member_id=?1 AND status=1",
            [mid],
            |r| r.get(0),
        )?;
        if exists == 0 {
            return Err(CoreError::Validation("绑定的会员不存在或已停用".into()));
        }
    }
    let now = format_utc(&Utc::now());
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO table_session(table_id,member_id,cart_json,sent_json,opened_at,updated_at,status)
         VALUES (?1,?2,?3,'[]',?4,?4,'OPEN')",
        params![table_id, member_id, cart_json, now],
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

/// 占用中的桌台换绑会员：门店开启“允许换绑”后才执行，命令层写审计。
pub fn rebind_member(
    conn: &Connection,
    table_id: i64,
    member_id: i64,
) -> Result<ServiceTable> {
    if cfg_text(conn, crate::constants::CFG_TABLE_ALLOW_REBIND, "0") != "1" {
        return Err(CoreError::Validation(
            "门店未开启桌台换绑会员，请先在系统配置中开启".into(),
        ));
    }
    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM member WHERE member_id=?1 AND status=1",
        [member_id],
        |r| r.get(0),
    )?;
    if exists == 0 {
        return Err(CoreError::Validation("会员不存在或已停用".into()));
    }
    let changed = conn.execute(
        "UPDATE table_session SET member_id=?1, updated_at=?2
         WHERE table_id=?3 AND status='OPEN'",
        params![member_id, format_utc(&Utc::now()), table_id],
    )?;
    if changed == 0 {
        return Err(CoreError::Validation("桌台未开台".into()));
    }
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

/// 餐饮下单：把本次新增菜品写入后厨出品清单，并推进已下单快照。
/// 只允许购物车数量不低于已下发数量；减菜必须先走 return_sent，避免厨房与账单脱节。
pub fn submit_order(conn: &Connection, table_id: i64, scene: &str) -> Result<Vec<KitchenItem>> {
    if scene != SCENE_DINING {
        return Err(CoreError::Validation("仅餐饮桌台支持厨打下单".into()));
    }
    let (cart_json, sent_json): (String, String) = conn
        .query_row(
            "SELECT s.cart_json, s.sent_json
             FROM service_table t JOIN table_session s ON s.table_id=t.table_id AND s.status='OPEN'
             WHERE t.table_id=?1 AND t.scene=?2",
            params![table_id, scene],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| CoreError::Validation("桌台未开台".into()))?;
    let cart = parse_lines(&cart_json)?;
    let sent = parse_lines(&sent_json)?;
    let cart_qty = qty_map(&cart);
    let sent_qty = qty_map(&sent);
    for (sku_id, sent_n) in &sent_qty {
        match cart_qty.get(sku_id) {
            Some(cart_n) if cart_n >= sent_n => {}
            _ => {
                return Err(CoreError::Validation(
                    "有已下单菜品被改少，请先执行退菜再下单".into(),
                ))
            }
        }
    }
    let mut additions = Vec::new();
    for (sku_id, cart_n) in &cart_qty {
        let add = cart_n - sent_qty.get(sku_id).copied().unwrap_or(0);
        if add <= 0 {
            continue;
        }
        let (name, unit, category_id): (String, String, i64) = conn
            .query_row(
                "SELECT p.spu_name, p.unit, p.category_id
                 FROM sku s JOIN spu p ON p.spu_id=s.spu_id WHERE s.sku_id=?1",
                [sku_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?
            .ok_or_else(|| CoreError::Validation(format!("菜品 {sku_id} 不存在")))?;
        additions.push(KitchenItem {
            sku_id: *sku_id,
            category_id,
            name,
            unit,
            qty: add,
        });
    }
    let ticket_json = serde_json::to_string(&additions)?;
    conn.execute(
        "UPDATE table_session SET sent_json=?1, last_ticket_json=?2, updated_at=?3
         WHERE table_id=?4 AND status='OPEN'",
        params![cart_json, ticket_json, format_utc(&Utc::now()), table_id],
    )?;
    Ok(additions)
}

/// 已下单退菜：同时从购物车与已下发快照扣减，qty 不得超过已下单数量。
pub fn return_sent(
    conn: &Connection,
    table_id: i64,
    scene: &str,
    sku_id: i64,
    qty: i64,
) -> Result<ServiceTable> {
    if scene != SCENE_DINING {
        return Err(CoreError::Validation("仅餐饮桌台支持退菜".into()));
    }
    if qty <= 0 {
        return Err(CoreError::Validation("退菜数量必须大于 0".into()));
    }
    let (cart_json, sent_json): (String, String) = conn
        .query_row(
            "SELECT s.cart_json, s.sent_json
             FROM service_table t JOIN table_session s ON s.table_id=t.table_id AND s.status='OPEN'
             WHERE t.table_id=?1 AND t.scene=?2",
            params![table_id, scene],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| CoreError::Validation("桌台未开台".into()))?;
    let mut cart = parse_lines(&cart_json)?;
    let mut sent = parse_lines(&sent_json)?;
    let sent_n = qty_map(&sent).get(&sku_id).copied().unwrap_or(0);
    if sent_n < qty {
        return Err(CoreError::Validation(format!(
            "只能退已下单的菜品，当前已下单 {sent_n} 份"
        )));
    }
    subtract_line(&mut cart, sku_id, qty)?;
    subtract_line(&mut sent, sku_id, qty)?;
    let cart_json = serde_json::to_string(&cart)?;
    let sent_json = serde_json::to_string(&sent)?;
    validate_cart(&cart_json)?;
    conn.execute(
        "UPDATE table_session SET cart_json=?1, sent_json=?2, updated_at=?3
         WHERE table_id=?4 AND status='OPEN'",
        params![cart_json, sent_json, format_utc(&Utc::now()), table_id],
    )?;
    table_view(conn, table_id)
}

/// 整桌并台：source 并入 target，目标保留开台时间与会员，源桌释放。
/// 仅餐饮支持，且两桌都必须在开台状态；已下单快照一并合并，避免厨房重复出品。
pub fn merge_tables(conn: &Connection, target_id: i64, source_id: i64) -> Result<ServiceTable> {
    if target_id == source_id {
        return Err(CoreError::Validation("不能并入同一桌台".into()));
    }
    let (t_scene, t_status): (String, String) = conn
        .query_row(
            "SELECT scene,status FROM service_table WHERE table_id=?1",
            [target_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| CoreError::Validation("目标桌台不存在".into()))?;
    let (s_scene, s_status): (String, String) = conn
        .query_row(
            "SELECT scene,status FROM service_table WHERE table_id=?1",
            [source_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| CoreError::Validation("源桌台不存在".into()))?;
    if t_scene != SCENE_DINING || s_scene != SCENE_DINING {
        return Err(CoreError::Validation("仅餐饮桌台支持并台".into()));
    }
    if t_status != TABLE_OCCUPIED || s_status != TABLE_OCCUPIED {
        return Err(CoreError::Validation("并台要求两个桌台都已开台".into()));
    }
    let (t_cart, t_sent): (String, String) = conn.query_row(
        "SELECT cart_json,sent_json FROM table_session WHERE table_id=?1 AND status='OPEN'",
        [target_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let (s_cart, s_sent): (String, String) = conn.query_row(
        "SELECT cart_json,sent_json FROM table_session WHERE table_id=?1 AND status='OPEN'",
        [source_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let merged_cart = merge_lines(&parse_lines(&t_cart)?, &parse_lines(&s_cart)?)?;
    let merged_sent = merge_lines(&parse_lines(&t_sent)?, &parse_lines(&s_sent)?)?;
    let cart_json = serde_json::to_string(&merged_cart)?;
    let sent_json = serde_json::to_string(&merged_sent)?;
    validate_cart(&cart_json)?;
    let now = format_utc(&Utc::now());
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE table_session SET cart_json=?1,sent_json=?2,updated_at=?3
         WHERE table_id=?4 AND status='OPEN'",
        params![cart_json, sent_json, now, target_id],
    )?;
    tx.execute(
        "UPDATE table_session SET status='CLOSED',updated_at=?1
         WHERE table_id=?2 AND status='OPEN'",
        params![now, source_id],
    )?;
    tx.execute(
        "UPDATE service_table SET status='FREE' WHERE table_id=?1",
        [source_id],
    )?;
    tx.commit()?;
    table_view(conn, target_id)
}

/// 转台：同场景整桌会话迁到空闲桌，计时起点、购物车、会员与已下单快照全部保留。
pub fn transfer_table(conn: &Connection, table_id: i64, target_id: i64) -> Result<ServiceTable> {
    if table_id == target_id {
        return Err(CoreError::Validation("不能转到同一桌台".into()));
    }
    let (scene, status): (String, String) = conn
        .query_row(
            "SELECT scene,status FROM service_table WHERE table_id=?1",
            [table_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| CoreError::Validation("原桌台不存在".into()))?;
    let (target_scene, target_status): (String, String) = conn
        .query_row(
            "SELECT scene,status FROM service_table WHERE table_id=?1",
            [target_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| CoreError::Validation("目标桌台不存在".into()))?;
    if status != TABLE_OCCUPIED {
        return Err(CoreError::Validation("原桌台未开台".into()));
    }
    if scene != target_scene {
        return Err(CoreError::Validation("只能转到同场景桌台".into()));
    }
    if target_status != TABLE_FREE {
        return Err(CoreError::Validation("目标桌台已被占用".into()));
    }
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE table_session SET table_id=?1, updated_at=?2 WHERE table_id=?3 AND status='OPEN'",
        params![target_id, format_utc(&Utc::now()), table_id],
    )?;
    tx.execute(
        "UPDATE service_table SET status='FREE' WHERE table_id=?1",
        [table_id],
    )?;
    tx.execute(
        "UPDATE service_table SET status='OCCUPIED' WHERE table_id=?1",
        [target_id],
    )?;
    tx.commit()?;
    table_view(conn, target_id)
}

/// 棋牌计时：最低 60 分钟，之后按实际分钟按房费 SKU 的分钟单价计费。
pub fn chess_fee(conn: &Connection, table_id: i64) -> Result<ChessFee> {
    let (scene, status, fee_sku, opened, fee_unit, min_minutes, round_minutes): (
        String,
        String,
        Option<i64>,
        Option<String>,
        Option<String>,
        Option<i64>,
        Option<i64>,
    ) = conn
        .query_row(
            "SELECT t.scene,t.status,t.fee_sku_id,s.opened_at,
                    t.fee_unit,t.min_minutes,t.round_minutes
             FROM service_table t
             LEFT JOIN table_session s ON s.table_id=t.table_id AND s.status='OPEN'
             WHERE t.table_id=?1",
            [table_id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| CoreError::Validation("桌台不存在".into()))?;
    if scene != SCENE_CHESS || status != TABLE_OCCUPIED {
        return Err(CoreError::Validation("包间未开台".into()));
    }
    let fee_sku = fee_sku.ok_or_else(|| CoreError::Validation("包间未配置房费商品".into()))?;
    let opened = opened.ok_or_else(|| CoreError::Validation("包间缺少开台时间".into()))?;
    let opened_at = chrono::DateTime::parse_from_rfc3339(&opened)
        .map_err(|_| CoreError::Validation("开台时间格式错误".into()))?
        .with_timezone(&Utc);
    let seconds = (Utc::now() - opened_at).num_seconds().max(0);
    let elapsed_minutes = (seconds + 59) / 60;
    let unit = fee_unit
        .unwrap_or_else(|| cfg_text(conn, crate::constants::CFG_CHESS_FEE_UNIT, "MINUTE"));
    let unit = if unit == "HOUR" { "HOUR" } else { "MINUTE" };
    let min_minutes = min_minutes
        .unwrap_or_else(|| cfg_i64(conn, crate::constants::CFG_CHESS_MIN_MINUTES, CHESS_MIN_BILLABLE_MINUTES))
        .max(0);
    let round_minutes = round_minutes
        .unwrap_or_else(|| cfg_i64(conn, crate::constants::CFG_CHESS_ROUND_MINUTES, 0))
        .clamp(0, 120);
    let mut billable_minutes = elapsed_minutes.max(min_minutes);
    // 超时取整：不足一个取整单位按一个单位计；0 表示按实际分钟。
    if round_minutes > 0 {
        billable_minutes = ((billable_minutes + round_minutes - 1) / round_minutes) * round_minutes;
    }
    let unit_cents: i64 = conn.query_row(
        "SELECT sale_price_cents FROM sku WHERE sku_id=?1",
        [fee_sku],
        |r| r.get(0),
    )?;
    let fee_cents = if unit == "HOUR" {
        // 小时价按分钟折算，分位四舍五入；金额仍由服务端计算。
        ((unit_cents as i128 * billable_minutes as i128) + 30) / 60
    } else {
        (unit_cents as i128 * billable_minutes as i128)
    };
    Ok(ChessFee {
        elapsed_minutes,
        billable_minutes,
        unit_cents,
        fee_unit: unit.to_string(),
        fee_cents: fee_cents as i64,
    })
}

fn parse_lines(json: &str) -> Result<Vec<serde_json::Value>> {
    serde_json::from_str(json)
        .map_err(|_| CoreError::Validation("桌台购物车不是合法 JSON 数组".into()))
}

fn qty_map(lines: &[serde_json::Value]) -> std::collections::HashMap<i64, i64> {
    let mut map = std::collections::HashMap::new();
    for line in lines {
        let sku_id = line.get("sku_id").and_then(|v| v.as_i64()).unwrap_or(0);
        let qty = line.get("qty").and_then(|v| v.as_i64()).unwrap_or(0);
        if sku_id > 0 && qty > 0 {
            *map.entry(sku_id).or_insert(0) += qty;
        }
    }
    map
}

fn subtract_line(lines: &mut Vec<serde_json::Value>, sku_id: i64, qty: i64) -> Result<()> {
    let mut remaining = qty;
    let mut out = Vec::new();
    for mut line in lines.drain(..) {
        if remaining == 0 {
            out.push(line);
            continue;
        }
        let id = line.get("sku_id").and_then(|v| v.as_i64()).unwrap_or(0);
        if id != sku_id {
            out.push(line);
            continue;
        }
        let line_qty = line.get("qty").and_then(|v| v.as_i64()).unwrap_or(0);
        let take = remaining.min(line_qty);
        remaining -= take;
        if line_qty > take {
            line["qty"] = serde_json::json!(line_qty - take);
            out.push(line);
        }
    }
    if remaining > 0 {
        return Err(CoreError::Validation("退菜数量超过购物车数量".into()));
    }
    *lines = out;
    Ok(())
}

fn merge_lines(
    left: &[serde_json::Value],
    right: &[serde_json::Value],
) -> Result<Vec<serde_json::Value>> {
    let mut out: Vec<serde_json::Value> = left.to_vec();
    for line in right {
        let sku_id = line.get("sku_id").and_then(|v| v.as_i64()).unwrap_or(0);
        let price = line
            .get("unit_price_cents")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let qty = line.get("qty").and_then(|v| v.as_i64()).unwrap_or(0);
        if sku_id <= 0 || qty <= 0 {
            continue;
        }
        if let Some(existing) = out.iter_mut().find(|v| {
            v.get("sku_id").and_then(|x| x.as_i64()) == Some(sku_id)
                && v.get("unit_price_cents").and_then(|x| x.as_i64()) == Some(price)
        }) {
            let current = existing.get("qty").and_then(|v| v.as_i64()).unwrap_or(0);
            existing["qty"] = serde_json::json!(current + qty);
        } else {
            out.push(line.clone());
        }
    }
    if out.len() > CART_MAX_LINES {
        return Err(CoreError::Validation("并台后购物车行数过多".into()));
    }
    Ok(out)
}

fn table_view(conn: &Connection, table_id: i64) -> Result<ServiceTable> {
    conn.query_row(
        "SELECT t.table_id, t.code, t.name, t.zone, t.seats, t.scene,
                t.fee_sku_id, t.fee_unit, t.min_minutes, t.round_minutes, t.status,
                s.session_id, s.opened_at, s.member_id,
                (SELECT phone FROM member WHERE member_id=s.member_id),
                COALESCE(s.cart_json,'[]'), COALESCE(s.sent_json,'[]')
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
        "SELECT t.table_id, t.code, t.name, t.zone, t.seats, t.scene,
                t.fee_sku_id, t.fee_unit, t.min_minutes, t.round_minutes, t.status,
                s.session_id, s.opened_at, s.member_id,
                (SELECT phone FROM member WHERE member_id=s.member_id),
                COALESCE(s.cart_json,'[]'), COALESCE(s.sent_json,'[]')
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
        fee_unit: r.get(7)?,
        min_minutes: r.get(8)?,
        round_minutes: r.get(9)?,
        status: r.get(10)?,
        session_id: r.get(11)?,
        opened_at: r.get(12)?,
        member_id: r.get(13)?,
        member_phone: r.get(14)?,
        cart_json: r.get(15)?,
        sent_json: r.get(16)?,
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
        let id = create_table(&c, SCENE_DINING, "A01", "大厅 1 号", "大厅", 4, None, None, None, None).unwrap();
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
        let t = open_table(&c, id, r#"[{"sku_id":1,"qty":2}]"#, None).unwrap();
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
        let id = create_table(&c, SCENE_CHESS, "B01", "包间 1", "二楼", 6, None, None, None, None).unwrap();
        open_table(&c, id, "[]", None).unwrap();
        assert!(open_table(&c, id, "[]", None).is_err());
        assert!(delete_table(&c, id).is_err());
        close_table(&c, id).unwrap();
        delete_table(&c, id).unwrap();
    }

    #[test]
    fn cart_rejects_bad_lines_and_duplicate_code() {
        let c = setup();
        let id = create_table(&c, SCENE_DINING, "A01", "大厅 1 号", "大厅", 4, None, None, None, None).unwrap();
        assert!(create_table(&c, SCENE_DINING, "A01", "重复", "大厅", 4, None, None, None, None).is_err());
        assert!(open_table(&c, id, r#"[{"sku_id":0,"qty":1}]"#, None).is_err());
        assert!(open_table(&c, id, r#"[{"sku_id":1,"qty":0}]"#, None).is_err());
    }

    fn seed_dish(c: &Connection) {
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
    }

    #[test]
    fn submit_only_new_items_and_return_sent() {
        let c = setup();
        seed_dish(&c);
        let id = create_table(&c, SCENE_DINING, "A01", "大厅 1 号", "大厅", 4, None, None, None, None).unwrap();
        open_table(&c, id, r#"[{"sku_id":1,"qty":1}]"#, None).unwrap();
        let first = submit_order(&c, id, SCENE_DINING).unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].qty, 1);
        // 已下发 1 份，再加 1 份：只打印新增的 1 份。
        save_cart(&c, id, r#"[{"sku_id":1,"qty":2}]"#).unwrap();
        let second = submit_order(&c, id, SCENE_DINING).unwrap();
        assert_eq!(second[0].qty, 1);
        // 未退菜前不允许把购物车改到低于已下发数量。
        save_cart(&c, id, r#"[{"sku_id":1,"qty":1}]"#).unwrap();
        assert!(submit_order(&c, id, SCENE_DINING).is_err());
        // 恢复购物车到与已下发一致，再退 1 份。
        save_cart(&c, id, r#"[{"sku_id":1,"qty":2}]"#).unwrap();
        // 退 1 份后购物车与已下发快照同步为 1。
        let t = return_sent(&c, id, SCENE_DINING, 1, 1).unwrap();
        assert_eq!(t.cart_json, r#"[{"sku_id":1,"qty":1}]"#);
        assert_eq!(t.sent_json, r#"[{"sku_id":1,"qty":1}]"#);
        assert!(return_sent(&c, id, SCENE_DINING, 1, 2).is_err());
    }

    #[test]
    fn merge_tables_combines_cart_and_sent_snapshot() {
        let c = setup();
        seed_dish(&c);
        let target = create_table(&c, SCENE_DINING, "A01", "大厅 1 号", "大厅", 4, None, None, None, None).unwrap();
        let source = create_table(&c, SCENE_DINING, "A02", "大厅 2 号", "大厅", 4, None, None, None, None).unwrap();
        open_table(
            &c,
            target,
            r#"[{"sku_id":1,"qty":1,"unit_price_cents":1000}]"#,
            None,
        )
        .unwrap();
        open_table(
            &c,
            source,
            r#"[{"sku_id":1,"qty":2,"unit_price_cents":1000}]"#,
            None,
        )
        .unwrap();
        submit_order(&c, source, SCENE_DINING).unwrap();
        let merged = merge_tables(&c, target, source).unwrap();
        assert!(merged.cart_json.contains(r#""qty":3"#));
        assert!(merged.sent_json.contains(r#""qty":2"#));
        assert_eq!(table_view(&c, source).unwrap().status, TABLE_FREE);
        assert!(merge_tables(&c, target, source).is_err());
    }

    #[test]
    fn chess_fee_charges_minimum_hour_then_by_minute() {
        let c = setup();
        seed_dish(&c);
        c.execute("UPDATE sku SET sale_price_cents=150", [])
            .unwrap();
        let id = create_table(&c, SCENE_CHESS, "B01", "包间 1", "二楼", 6, Some(1), None, None, None).unwrap();
        open_table(&c, id, "[]", None).unwrap();
        let fee = chess_fee(&c, id).unwrap();
        assert_eq!(fee.elapsed_minutes, 0);
        assert_eq!(fee.billable_minutes, 60);
        assert_eq!(fee.fee_cents, 60 * 150);
        let opened = format_utc(&(Utc::now() - chrono::Duration::minutes(89)));
        c.execute(
            "UPDATE table_session SET opened_at=?1 WHERE table_id=?2 AND status='OPEN'",
            params![opened, id],
        )
        .unwrap();
        let fee = chess_fee(&c, id).unwrap();
        assert_eq!(fee.billable_minutes, 90);
        assert_eq!(fee.fee_cents, 90 * 150);
    }

    #[test]
    fn chess_fee_respects_table_override_and_rounding() {
        let c = setup();
        seed_dish(&c);
        c.execute("UPDATE sku SET sale_price_cents=6000", []).unwrap();
        let id = create_table(
            &c,
            SCENE_CHESS,
            "B02",
            "包间 2",
            "二楼",
            6,
            Some(1),
            Some("HOUR"),
            Some(30),
            Some(10),
        )
        .unwrap();
        open_table(&c, id, "[]", None).unwrap();
        let fee = chess_fee(&c, id).unwrap();
        assert_eq!(fee.fee_unit, "HOUR");
        assert_eq!(fee.billable_minutes, 30);
        assert_eq!(fee.fee_cents, 3000);
        let opened = format_utc(&(Utc::now() - chrono::Duration::minutes(41)));
        c.execute(
            "UPDATE table_session SET opened_at=?1 WHERE table_id=?2 AND status='OPEN'",
            params![opened, id],
        )
        .unwrap();
        let fee = chess_fee(&c, id).unwrap();
        assert_eq!(fee.billable_minutes, 50);
        assert_eq!(fee.fee_cents, 5000);
    }
}
