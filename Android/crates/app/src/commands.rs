//! Tauri 命令层。所有命令在此注册；capabilities 仅暴露此清单。
//! 门禁统一入口按 §12.2 顺序执行。

use crate::state::{build_license_service, init_db, AppState, PendingSensitive};
use checkout_core::constants::{
    CFG_ACTIVATION_ENDPOINT, CFG_ALLOW_NEGATIVE_STOCK, CFG_BUSINESS_DAY_CUTOFF, CFG_DEVICE_NO,
    CFG_EXPENSE_LABOR_YUAN, CFG_EXPENSE_RENT_YUAN, CFG_EXPENSE_UTILITY_YUAN,
    CFG_MAX_DISCOUNT_RATIO, CFG_MICROPAY_POLL_MS, CFG_MICROPAY_TIMEOUT_S, CFG_PAY_CHANNEL_ALIPAY,
    CFG_PAY_CHANNEL_CASH, CFG_PAY_CHANNEL_SV, CFG_PAY_CHANNEL_WECHAT, CFG_POINTS_ENABLED,
    CFG_POINTS_PER_YUAN, CFG_PRINT_AUTO, CFG_PRINT_COPIES, CFG_PRINT_HOST, CFG_PRINT_PORT,
    CFG_RECEIPT_QR_BASE,
    CFG_SCAN_GAP_MS, CFG_STORE_NAME, CFG_STORE_NO, CFG_STORE_PHONE, CFG_STORE_TIMEZONE,
    CFG_TABLE_ALLOW_REBIND, COUPON_UNUSED, DEFAULT_MAX_DISCOUNT_RATIO, DEFAULT_MICROPAY_POLL_MS,
    DEFAULT_MICROPAY_TIMEOUT_S, DEFAULT_POINTS_ENABLED, DEFAULT_POINTS_PER_YUAN,
    DEFAULT_PRINT_PORT, ENT_MEMBER_BASIC, ENT_POS_CORE, ENT_REPORT_STANDARD,
    LIMIT_REFUND_MAX_CENTS, PAY_ALIPAY, PAY_CASH, PAY_MODE_MICROPAY, PAY_MODE_NATIVE,
    PAY_STORED_VALUE, PAY_WECHAT, PERM_INVENTORY_ADJUST, PERM_MEMBER_MANAGE, PERM_MEMBER_RECHARGE,
    PERM_POS_CHECKOUT, PERM_POS_REFUND, PERM_POS_SHIFT_CLOSE, PERM_POS_VOID, PERM_PRODUCT_EDIT,
    PERM_REPORT_EXPORT, PERM_REPORT_VIEW, PERM_STOCKTAKE_APPROVE, PERM_SYSTEM_CONFIG,
    PERM_USER_MANAGE, ROLE_ADMIN, ROLE_CASHIER, ROLE_MANAGER,
};
use checkout_core::error::{CoreError, Result};
/// 本地模拟支付开关（联调/开发用）。生产默认关闭：未配置渠道凭据时直接拒绝收款，
/// 而不是把任意付款码静默记成"已收款"。
const CFG_ALLOW_LOCAL_SIMULATION: &str = "pay.allow_local_simulation";
use checkout_core::industry::{IndustryOption, StoreProfile};
use checkout_core::license::{LicenseSnapshot, LicenseStatus};
use checkout_core::orderview::{OrderDetail, OrderSummary};
use checkout_core::paycreds::SecretStore;
use checkout_core::payment::{
    run_micropay, Channel as PayChannel, PayMode, PayRequest, PaymentGateway, PaymentState,
};
use checkout_core::promo::{Promotion, Scope as PromoScope};
use checkout_core::rbac::{self, Session};
use checkout_core::time::{format_utc, parse_rfc3339};
use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, Timelike, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{Emitter, Manager, State};

fn now_str() -> String {
    Utc::now().to_rfc3339()
}

static NONCE_SEQ: AtomicU64 = AtomicU64::new(0);
fn new_nonce() -> String {
    let seq = NONCE_SEQ.fetch_add(1, Ordering::SeqCst);
    format!("{}-{}", Utc::now().timestamp_nanos_opt().unwrap_or(0), seq)
}

/// 优惠券查询行：(优惠额, 门槛, 会员ID, 状态, 生效起, 生效止)。
/// 与下方对外展示用的 CouponRow 结构体区分，仅内部查询使用。
type CouponQueryRow = (
    i64,
    i64,
    Option<i64>,
    String,
    Option<String>,
    Option<String>,
);
/// 支付流水查询行：(渠道, 单号, 金额分, 储值赠送分, 储值本金分)。
type PaymentRow = (String, Option<String>, i64, Option<i64>, Option<i64>);

/// 读取 system_config 字符串值，缺失返回 None。
fn cfg_get(conn: &Connection, key: &str) -> Result<Option<String>> {
    let v: Option<String> = conn
        .query_row(
            "SELECT config_value FROM system_config WHERE config_key=?1",
            [key],
            |r| r.get(0),
        )
        .optional()?;
    Ok(v)
}

/// 读取 system_config 整数值，缺失/非法回退到默认值。
fn cfg_i64(conn: &Connection, key: &str, default: i64) -> i64 {
    cfg_get(conn, key)
        .ok()
        .flatten()
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(default)
}

/// 读取 system_config 字符串值，缺失/空白回退到默认值。
fn cfg_str(conn: &Connection, key: &str, default: &str) -> String {
    cfg_get(conn, key)
        .ok()
        .flatten()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default.to_string())
}

/// 读取 system_config f64 值，缺失/非法回退到默认值。
fn cfg_f64(conn: &Connection, key: &str, default: f64) -> f64 {
    cfg_get(conn, key)
        .ok()
        .flatten()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(default)
}

/// 积分累计是否启用（缺失按默认开启处理，老库升级不中断积分）。
fn points_enabled(conn: &Connection) -> bool {
    cfg_get(conn, CFG_POINTS_ENABLED)
        .ok()
        .flatten()
        .map(|v| v != "0")
        .unwrap_or(DEFAULT_POINTS_ENABLED == 1)
}

/// 按应付金额预估积分（报价用）。口径与 member::accrue_points 一致：整元累计，
/// 每元赠送积分数由系统配置决定；未识别会员或积分关闭返回 0。
fn estimate_points(conn: &Connection, member_id: Option<i64>, payable_cents: i64) -> i64 {
    if member_id.is_none() || !points_enabled(conn) || payable_cents <= 0 {
        return 0;
    }
    let per_yuan = cfg_i64(conn, CFG_POINTS_PER_YUAN, DEFAULT_POINTS_PER_YUAN).max(0);
    (payable_cents / 100) * per_yuan
}

/// 启动：迁移 → 许可证验证 → 管理员初始化 → 加载状态。
#[tauri::command]
pub fn bootstrap(state: State<'_, AppState>) -> Result<LicenseSnapshot> {
    let mut license = state
        .license
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    let svc = build_license_service(&state);
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    // 本地验证先行：验签、指纹、状态判定，全程离线可用。
    *license = svc.verify_on_launch(&conn)?;
    let local = license.clone();

    // 已激活且有激活码时，联网向激活服务读取最新状态（撤销/过期/续期以服务端为准）。
    // 失败分两类处理：
    // - 服务端明确拒绝（撤销/过期/设备超限）→ 本地状态同步降级，不得继续显示「已激活」；
    // - 网络不可达或服务端故障 → 保留本地缓存，前台收银照常（架构硬约束：不得因授权阻断前台）。
    if local.status != LicenseStatus::UNACTIVATED {
        if let Ok(endpoint) = resolve_activation_endpoint(&conn) {
            let store_name = cfg_get(&conn, CFG_STORE_NAME)
                .ok()
                .flatten()
                .unwrap_or_default();
            match svc.refresh_online(&conn, &endpoint, &store_name, &store_name) {
                Ok(refreshed) => *license = refreshed,
                Err(e) => {
                    if matches!(
                        e,
                        CoreError::License(checkout_core::error::LicenseError::OnlineRejected(_))
                    ) {
                        *license = svc.mark_revoked(&conn, &e.to_string())?;
                    }
                    // 其余错误（网络不可达等）保留本地缓存状态，不影响启动与前台收银
                }
            }
        }
    }
    // 重启后恢复当前班次：否则开单会落到 shift_id=0，交班汇总不到这些单。
    let open_shift: Option<i64> = conn
        .query_row(
            "SELECT shift_id FROM shift WHERE status='OPEN' ORDER BY shift_id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()?;
    *state
        .current_shift
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))? = open_shift.unwrap_or(0);
    Ok(license.clone())
}

/// 首次初始化管理员。
#[tauri::command]
pub fn init_admin(username: String, password: String, state: State<'_, AppState>) -> Result<()> {
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    rbac::ensure_admin(&conn, &username, &password)
}

/// user_account 是否为空（空 → 前端强制显示初始化管理员表单，§12.3）。
#[tauri::command]
pub fn has_users(state: State<'_, AppState>) -> Result<bool> {
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM user_account", [], |r| r.get(0))?;
    Ok(count > 0)
}

/// 按权限推导所需 entitlement：会员能力走 member.basic，
/// 报表/配置走 report.standard，其余后台能力走基础 pos.core，
/// 避免"授权降级时连会员识别都不可用"。
fn entitlement_for(permission: &str) -> &'static str {
    if permission.starts_with("member.") {
        ENT_MEMBER_BASIC
    } else if permission.starts_with("report.")
        || permission == PERM_SYSTEM_CONFIG
        || permission == PERM_USER_MANAGE
    {
        ENT_REPORT_STANDARD
    } else {
        ENT_POS_CORE
    }
}

/// 后台管理命令统一门禁（§12.2 顺序）：按权限选 entitlement → 会话 → RBAC 权限。
/// 返回 (operator_id, conn)；license/session 锁在返回前释放，调用方继续持有 conn 做业务写入。
fn admin_guard<'a>(
    state: &'a AppState,
    permission: &str,
) -> Result<(i64, std::sync::MutexGuard<'a, rusqlite::Connection>)> {
    let license = state
        .license
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    let session = state
        .session
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    let operator_id = {
        let s = session.as_ref().ok_or(CoreError::NotAuthenticated)?;
        checkout_core::rbac::guard(
            &conn,
            &checkout_core::rbac::GuardRequest {
                entitlement: entitlement_for(permission),
                permission,
                session: s,
                license: &license,
                sensitive_auth: None,
                current_nonce: "",
                now: parse_rfc3339(&now_str())?,
                amount_cents: 0,
                limit_key: None,
            },
        )?;
        s.user_id
    };
    let _session = session;
    drop(license);
    Ok((operator_id, conn))
}

/// 桌台列表：餐饮/棋牌场景各自独立，占用桌带未结算购物车快照。
#[tauri::command]
pub fn list_service_tables(
    scene: String,
    state: State<'_, AppState>,
) -> Result<Vec<checkout_core::service::ServiceTable>> {
    let (_op, conn) = admin_guard(&state, PERM_POS_CHECKOUT)?;
    checkout_core::service::list_tables(&conn, &scene)
}

/// 新建桌台/包间（管理动作）：编码场景内唯一，座位 1–50。
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn create_service_table(
    scene: String,
    code: String,
    name: String,
    zone: String,
    seats: i64,
    fee_sku_id: Option<i64>,
    fee_unit: Option<String>,
    min_minutes: Option<i64>,
    round_minutes: Option<i64>,
    state: State<'_, AppState>,
) -> Result<i64> {
    let (_op, conn) = admin_guard(&state, PERM_PRODUCT_EDIT)?;
    checkout_core::service::create_table(
        &conn,
        &scene,
        &code,
        &name,
        &zone,
        seats,
        fee_sku_id,
        fee_unit.as_deref(),
        min_minutes,
        round_minutes,
    )
}

#[tauri::command]
pub fn delete_service_table(table_id: i64, state: State<'_, AppState>) -> Result<()> {
    let (_op, conn) = admin_guard(&state, PERM_PRODUCT_EDIT)?;
    checkout_core::service::delete_table(&conn, table_id)
}

/// 开台：购物车快照随会话保存；开台时已识别的会员一并绑定，恢复桌台时沿用会员价。
#[tauri::command]
pub fn open_service_table(
    table_id: i64,
    cart_json: String,
    member_id: Option<i64>,
    state: State<'_, AppState>,
) -> Result<checkout_core::service::ServiceTable> {
    let (_op, conn) = admin_guard(&state, PERM_POS_CHECKOUT)?;
    checkout_core::service::open_table(&conn, table_id, &cart_json, member_id)
}

/// 换绑会员：需门店开启“允许换绑”，操作写审计。
#[tauri::command]
pub fn rebind_service_table_member(
    table_id: i64,
    member_id: i64,
    state: State<'_, AppState>,
) -> Result<checkout_core::service::ServiceTable> {
    let (operator_id, conn) = admin_guard(&state, PERM_POS_CHECKOUT)?;
    let table = checkout_core::service::rebind_member(&conn, table_id, member_id)?;
    checkout_core::rbac::audit_operation(
        &conn,
        "table_rebind_member",
        Some(operator_id),
        None,
        serde_json::json!({"table_id": table_id, "member_id": member_id}),
        &now_str(),
    )?;
    Ok(table)
}

/// 点单过程中保存购物车快照；同一桌只保留最新一份。
#[tauri::command]
pub fn save_service_table_cart(
    table_id: i64,
    cart_json: String,
    state: State<'_, AppState>,
) -> Result<()> {
    let (_op, conn) = admin_guard(&state, PERM_POS_CHECKOUT)?;
    checkout_core::service::save_cart(&conn, table_id, &cart_json)
}

/// 清台：结算成功后释放；force=true 表示未结算强清，需作废权限并留审计。
#[tauri::command]
pub fn close_service_table(table_id: i64, force: bool, state: State<'_, AppState>) -> Result<()> {
    let (operator_id, conn) = admin_guard(
        &state,
        if force {
            PERM_POS_VOID
        } else {
            PERM_POS_CHECKOUT
        },
    )?;
    checkout_core::service::close_table(&conn, table_id)?;
    if force {
        checkout_core::rbac::audit_operation(
            &conn,
            "clear_table_without_checkout",
            Some(operator_id),
            None,
            serde_json::json!({"table_id": table_id}),
            &now_str(),
        )?;
    }
    Ok(())
}

/// 餐饮下单结果：厨打先于"已下单"快照推进，打印失败时不推进并直接把错误抛给前台。
#[derive(Debug, Serialize)]
pub struct KitchenSubmitResult {
    pub items: Vec<checkout_core::service::KitchenItem>,
    pub print_message: String,
}

/// 餐饮下单：计算新增菜品 → 打印厨打 → 打印全成功后才推进已下单快照。
/// 顺序不能颠倒：先记"已下单"再打印，一旦打印机掉线，后厨没收到菜、账单却认为已做，
/// 只有把快照推进放在打印成功之后，重试时这几道菜才会仍被算作新增。
#[tauri::command]
pub fn submit_service_table_order(
    table_id: i64,
    scene: String,
    state: State<'_, AppState>,
) -> Result<KitchenSubmitResult> {
    let (_op, conn) = admin_guard(&state, PERM_POS_CHECKOUT)?;
    let (items, cart_json) = checkout_core::service::prepare_order(&conn, table_id, &scene)?;
    if items.is_empty() {
        return Ok(KitchenSubmitResult {
            items,
            print_message: "本次没有新增菜品，无需厨打".into(),
        });
    }
    let code: String = conn
        .query_row(
            "SELECT code FROM service_table WHERE table_id=?1",
            [table_id],
            |r| r.get(0),
        )
        .map_err(|_| CoreError::Validation("桌台不存在".into()))?;
    let print_message = print_kitchen_items(&conn, &code, &items)?;
    checkout_core::service::commit_order(&conn, table_id, &cart_json, &items)?;
    Ok(KitchenSubmitResult {
        items,
        print_message,
    })
}

/// 已下单退菜：需作废权限，写审计；M0017 前的退菜按购物车行删除处理。
#[tauri::command]
pub fn return_service_table_line(
    table_id: i64,
    scene: String,
    sku_id: i64,
    qty: i64,
    reason: String,
    state: State<'_, AppState>,
) -> Result<checkout_core::service::ServiceTable> {
    let (operator_id, conn) = admin_guard(&state, PERM_POS_VOID)?;
    let table = checkout_core::service::return_sent(&conn, table_id, &scene, sku_id, qty)?;
    checkout_core::rbac::audit_operation(
        &conn,
        "table_return_dish",
        Some(operator_id),
        None,
        serde_json::json!({"table_id": table_id, "sku_id": sku_id, "qty": qty, "reason": reason}),
        &now_str(),
    )?;
    Ok(table)
}

/// 并台：source 并入 target，仅餐饮且两桌都已开台。
#[tauri::command]
pub fn merge_service_tables(
    target_id: i64,
    source_id: i64,
    state: State<'_, AppState>,
) -> Result<checkout_core::service::ServiceTable> {
    let (operator_id, conn) = admin_guard(&state, PERM_POS_CHECKOUT)?;
    let table = checkout_core::service::merge_tables(&conn, target_id, source_id)?;
    checkout_core::rbac::audit_operation(
        &conn,
        "table_merge",
        Some(operator_id),
        None,
        serde_json::json!({"target_id": target_id, "source_id": source_id}),
        &now_str(),
    )?;
    Ok(table)
}

/// 转台：整桌会话迁到同场景空闲桌，保留开台时间、购物车、会员与已下单快照。
#[tauri::command]
pub fn transfer_service_table(
    table_id: i64,
    target_id: i64,
    state: State<'_, AppState>,
) -> Result<checkout_core::service::ServiceTable> {
    let (operator_id, conn) = admin_guard(&state, PERM_POS_CHECKOUT)?;
    let table = checkout_core::service::transfer_table(&conn, table_id, target_id)?;
    checkout_core::rbac::audit_operation(
        &conn,
        "table_transfer",
        Some(operator_id),
        None,
        serde_json::json!({"table_id": table_id, "target_id": target_id}),
        &now_str(),
    )?;
    Ok(table)
}

/// 棋牌计时取费：最低 60 分钟，超出后按分钟折算，金额由后端计算。
#[tauri::command]
pub fn chess_table_fee(
    table_id: i64,
    state: State<'_, AppState>,
) -> Result<checkout_core::service::ChessFee> {
    let (_op, conn) = admin_guard(&state, PERM_POS_CHECKOUT)?;
    checkout_core::service::chess_fee(&conn, table_id)
}

/// 厨打：打印该桌最后一次“下单”新增的出品行，复用当前网络打印机。
/// 任何一张没发出去（未配置/网络失败）都返回 Err：后厨看不到单就是漏菜，不能报成功。
#[tauri::command]
pub fn print_kitchen_ticket(table_id: i64, state: State<'_, AppState>) -> Result<String> {
    let (_op, conn) = admin_guard(&state, PERM_POS_CHECKOUT)?;
    let (code, ticket_json): (String, String) = conn
        .query_row(
            "SELECT t.code, COALESCE(s.last_ticket_json,'[]')
             FROM service_table t
             JOIN table_session s ON s.table_id=t.table_id AND s.status='OPEN'
             WHERE t.table_id=?1 AND t.scene='DINING'",
            [table_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| CoreError::Validation("桌台未开台或无厨打内容".into()))?;
    let items: Vec<checkout_core::service::KitchenItem> = serde_json::from_str(&ticket_json)?;
    if items.is_empty() {
        return Ok("本次没有新增菜品，无需厨打".into());
    }
    print_kitchen_items(&conn, &code, &items)
}

/// 按档口路由渲染并发送厨打，返回可读结果；任何一组失败即返回 Err。
fn print_kitchen_items(
    conn: &Connection,
    code: &str,
    items: &[checkout_core::service::KitchenItem],
) -> Result<String> {
    use checkout_core::escpos::{render_receipt, Template};
    use serde_json::json;
    let main_host = cfg_get(conn, CFG_PRINT_HOST)?.unwrap_or_default();
    let main_port = cfg_get(conn, CFG_PRINT_PORT)?
        .and_then(|s| s.trim().parse::<u16>().ok())
        .unwrap_or(DEFAULT_PRINT_PORT);
    // 档口路由：分类 → 独立打印机；未配置或未命中分类时回退主打印机。
    let routes: Vec<serde_json::Value> =
        cfg_get(conn, checkout_core::constants::CFG_KITCHEN_ROUTES)?
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
    let route_of = |category_id: i64| -> (String, String, u16) {
        for r in &routes {
            if r.get("category_id").and_then(|v| v.as_i64()) == Some(category_id) {
                let label = r
                    .get("label")
                    .and_then(|v| v.as_str())
                    .unwrap_or("档口")
                    .to_string();
                let host = r
                    .get("host")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let port = r
                    .get("port")
                    .and_then(|v| v.as_u64())
                    .map(|p| if p <= 65535 { p as u16 } else { main_port })
                    .unwrap_or(main_port);
                if !host.is_empty() {
                    return (label, host, port);
                }
            }
        }
        ("后厨".to_string(), main_host.clone(), main_port)
    };
    let mut groups: std::collections::BTreeMap<
        String,
        (
            String,
            String,
            u16,
            Vec<&checkout_core::service::KitchenItem>,
        ),
    > = std::collections::BTreeMap::new();
    for item in items {
        let (label, host, port) = route_of(item.category_id);
        groups
            .entry(format!("{label}|{host}|{port}"))
            .or_insert((label, host, port, Vec::new()))
            .3
            .push(item);
    }
    let mut sent = Vec::new();
    let mut failed = Vec::new();
    for (_, (label, host, port, group)) in groups {
        if host.trim().is_empty() {
            failed.push(format!("{label}未配置打印机"));
            continue;
        }
        let item_text = group
            .iter()
            .map(|i| format!("{}  x{} {}", i.name, i.qty, i.unit))
            .collect::<Vec<_>>()
            .join("\n");
        let total_qty: i64 = group.iter().map(|i| i.qty).sum();
        let template = Template {
            template_id: "kitchen_default".into(),
            paper_width_mm: 58,
            sections: json!({
                "header": [
                    {"type":"text","content":"后厨出品单","align":"center","bold":true,"size":2},
                    {"type":"text","content":"{{station}} · 桌台 {{table_code}}","align":"center","bold":true},
                    {"type":"text","content":"{{created_at}}","align":"center"}
                ],
                "items": [
                    {"type":"divider","char":"-"},
                    {"type":"text","content":"{{items}}","size":1}
                ],
                "summary": [
                    {"type":"divider","char":"-"},
                    {"type":"text","content":"合计 {{total_qty}} 份","align":"right","bold":true}
                ],
                "footer": [
                    {"type":"feed","lines":3},
                    {"type":"cut","mode":"partial"}
                ]
            }),
        };
        let mut vars = std::collections::HashMap::new();
        vars.insert("station".into(), label.clone());
        vars.insert("table_code".into(), code.to_string());
        vars.insert("created_at".into(), now_str());
        vars.insert("items".into(), item_text);
        vars.insert("total_qty".into(), total_qty.to_string());
        let bytes = render_receipt(&template, &vars)?;
        match checkout_core::printer::send_bytes(&host, port, &bytes) {
            Ok(()) => sent.push(label),
            Err(e) => failed.push(format!("{label}发送失败: {e}")),
        }
    }
    // 只要有一组没发出去，后厨就会漏菜：返回 Err 让前台明确"下单未生效"并重试。
    if !failed.is_empty() {
        let detail = failed.join("；");
        let prefix = if sent.is_empty() {
            "厨打未发送".to_string()
        } else {
            format!("厨打部分失败（已发送：{}）", sent.join("、"))
        };
        return Err(CoreError::Validation(format!(
            "{prefix}：{detail}。请检查打印机配置后重试"
        )));
    }
    Ok(format!("厨打已发送：{}", sent.join("、")))
}

/// 读取厨打档口路由（分类 → 打印机）。
#[tauri::command]
pub fn get_kitchen_routes(state: State<'_, AppState>) -> Result<Vec<serde_json::Value>> {
    let (_op, conn) = admin_guard(&state, PERM_PRODUCT_EDIT)?;
    let raw = cfg_get(&conn, checkout_core::constants::CFG_KITCHEN_ROUTES)?.unwrap_or_default();
    Ok(serde_json::from_str(&raw).unwrap_or_default())
}

/// 保存厨打档口路由。空数组表示全部回退主打印机。
#[tauri::command]
pub fn set_kitchen_routes(routes_json: String, state: State<'_, AppState>) -> Result<()> {
    let (_op, conn) = admin_guard(&state, PERM_PRODUCT_EDIT)?;
    let routes: Vec<serde_json::Value> = serde_json::from_str(&routes_json)
        .map_err(|_| CoreError::Validation("厨打路由必须是 JSON 数组".into()))?;
    if routes.len() > 50 {
        return Err(CoreError::Validation("厨打路由最多 50 条".into()));
    }
    for r in &routes {
        let category_id = r
            .get("category_id")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| CoreError::Validation("每条路由必须指定分类".into()))?;
        let exists: i64 = conn.query_row(
            "SELECT COUNT(*) FROM category WHERE category_id=?1",
            [category_id],
            |row| row.get(0),
        )?;
        if exists == 0 {
            return Err(CoreError::Validation(format!("分类 {category_id} 不存在")));
        }
        let host = r.get("host").and_then(|v| v.as_str()).unwrap_or("").trim();
        if host.is_empty()
            || host.len() > 64
            || !host
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        {
            return Err(CoreError::Validation(
                "档口打印机地址只能是 IP/主机名".into(),
            ));
        }
        let port = r.get("port").and_then(|v| v.as_u64()).unwrap_or(9100);
        if port == 0 || port > 65535 {
            return Err(CoreError::Validation("档口打印机端口非法".into()));
        }
    }
    let value = serde_json::to_string(&routes)?;
    conn.execute(
        "INSERT INTO system_config(config_key,config_value,updated_at) VALUES (?1,?2,?3)
         ON CONFLICT(config_key) DO UPDATE SET config_value=excluded.config_value, updated_at=excluded.updated_at",
        rusqlite::params![checkout_core::constants::CFG_KITCHEN_ROUTES, value, now_str()],
    )?;
    Ok(())
}

/// 技师档案：新建需商品编辑权限，比例以万分比保存（1200=12%）。
#[tauri::command]
pub fn create_technician(
    name: String,
    phone: Option<String>,
    commission_bps: i64,
    commission_mode: String,
    fixed_cents: i64,
    state: State<'_, AppState>,
) -> Result<i64> {
    let (_op, conn) = admin_guard(&state, PERM_PRODUCT_EDIT)?;
    checkout_core::technician::create_technician(
        &conn,
        &name,
        phone.as_deref(),
        commission_bps,
        &commission_mode,
        fixed_cents,
    )
}

#[tauri::command]
pub fn set_technician_rule(
    technician_id: i64,
    category_id: i64,
    mode: String,
    bps: i64,
    fixed_cents: i64,
    state: State<'_, AppState>,
) -> Result<()> {
    let (_op, conn) = admin_guard(&state, PERM_PRODUCT_EDIT)?;
    checkout_core::technician::set_rule(&conn, technician_id, category_id, &mode, bps, fixed_cents)
}

#[tauri::command]
pub fn delete_technician_rule(
    technician_id: i64,
    category_id: i64,
    state: State<'_, AppState>,
) -> Result<()> {
    let (_op, conn) = admin_guard(&state, PERM_PRODUCT_EDIT)?;
    checkout_core::technician::delete_rule(&conn, technician_id, category_id)
}

#[tauri::command]
pub fn list_technician_rules(
    technician_id: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<checkout_core::technician::TechnicianRule>> {
    let (_op, conn) = admin_guard(&state, PERM_PRODUCT_EDIT)?;
    checkout_core::technician::list_rules(&conn, technician_id)
}

/// 日排班：on_duty=true 在班，false 休息。日期为营业日 YYYY-MM-DD。
#[tauri::command]
pub fn set_technician_shift(
    technician_id: i64,
    business_date: String,
    on_duty: bool,
    state: State<'_, AppState>,
) -> Result<()> {
    let (_op, conn) = admin_guard(&state, PERM_PRODUCT_EDIT)?;
    checkout_core::technician::set_shift(&conn, technician_id, &business_date, on_duty)
}

#[tauri::command]
pub fn list_technicians(
    business_date: String,
    state: State<'_, AppState>,
) -> Result<Vec<checkout_core::technician::Technician>> {
    let (_op, conn) = admin_guard(&state, PERM_POS_CHECKOUT)?;
    checkout_core::technician::list_technicians(&conn, &business_date)
}

#[tauri::command]
pub fn technician_commission(
    from_date: String,
    to_date: String,
    state: State<'_, AppState>,
) -> Result<Vec<checkout_core::technician::TechnicianCommission>> {
    let (_op, conn) = admin_guard(&state, PERM_REPORT_VIEW)?;
    checkout_core::technician::commission_report(&conn, &from_date, &to_date)
}

#[tauri::command]
pub fn get_license(state: State<'_, AppState>) -> Result<LicenseSnapshot> {
    Ok(state
        .license
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?
        .clone())
}

#[tauri::command]
pub fn activate(token: String, state: State<'_, AppState>) -> Result<LicenseSnapshot> {
    ensure_production_key()?;
    let svc = build_license_service(&state);
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    let snap = svc.activate(&conn, &token)?;
    *state
        .license
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))? = snap.clone();
    Ok(snap)
}

#[tauri::command]
pub fn renew_license(token: String, state: State<'_, AppState>) -> Result<LicenseSnapshot> {
    let svc = build_license_service(&state);
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    let snap = svc.renew(&conn, &token)?;
    *state
        .license
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))? = snap.clone();
    Ok(snap)
}

#[tauri::command]
pub fn login(username: String, password: String, state: State<'_, AppState>) -> Result<Session> {
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    let now = parse_rfc3339(&now_str())?;
    let session = rbac::login(&conn, &username, &password, now)?;
    *state
        .session
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))? = Some(session.clone());
    Ok(session)
}

#[tauri::command]
pub fn logout(state: State<'_, AppState>) -> Result<()> {
    *state
        .session
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))? = None;
    Ok(())
}

/// 当前登录用户修改自己的密码：user_id 取自会话，不允许改他人密码。
#[tauri::command]
pub fn change_password(
    old_password: String,
    new_password: String,
    state: State<'_, AppState>,
) -> Result<()> {
    let user_id = {
        let session = state
            .session
            .lock()
            .map_err(|e| CoreError::Other(e.to_string()))?;
        session
            .as_ref()
            .map(|s| s.user_id)
            .ok_or_else(|| CoreError::PermissionDenied("未登录".into()))?
    };
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    rbac::change_password(&conn, user_id, &old_password, &new_password)
}

/// 当前会话：仅用于界面按角色显示菜单，未登录或已过期返回 null。
/// 真正的权限判定仍在各业务命令中完成，这里不构成安全边界。
#[tauri::command]
pub fn current_session(state: State<'_, AppState>) -> Result<Option<Session>> {
    let now = parse_rfc3339(&now_str())?;
    let session = state
        .session
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    Ok(session.as_ref().filter(|s| s.expires_at >= now).cloned())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProductHit {
    pub sku_id: i64,
    pub spu_id: i64,
    pub spu_name: String,
    pub barcode: String,
    /// 由规格快照生成的展示串（如 颜色:黑 / 尺码:M），非规格商品为空。
    pub spec_text: String,
    pub price_cents: i64,
    /// 会员价：识别会员后按此价展示（结算时由后端与促销取优）。
    pub member_price_cents: Option<i64>,
    pub is_weighed: bool,
    /// 分类名与一级分组名：行业场景面板按分组分档（如奶粉/纸尿裤）。
    pub category_name: String,
    pub category_group: String,
    pub unit: String,
    /// 商品图片：本地绝对路径（前端 convertFileSrc）或在线地址。
    pub image_path: Option<String>,
    pub image_url: Option<String>,
}

/// 商品查询共用实现：扫码搜索、场景面板、收银点选共用同一口径。
fn query_products(
    conn: &rusqlite::Connection,
    data_dir: &std::path::Path,
    keyword: &str,
    limit: i64,
    category_group: &str,
    category_id: Option<i64>,
) -> Result<Vec<ProductHit>> {
    let pattern = format!("%{keyword}%");
    let mut stmt = conn.prepare(
        "SELECT s.sku_id, p.spu_id, p.spu_name, COALESCE(s.barcode,''), s.sale_price_cents,
                s.is_weighed, s.member_price_cents, s.image_path, s.image_url,
                COALESCE(c.name,'未分类'), p.unit,
                COALESCE(pc.name, c.name, '未分类'), COALESCE(s.spec_json,'')
         FROM sku s JOIN spu p ON p.spu_id=s.spu_id
         LEFT JOIN category c ON c.category_id=p.category_id
         LEFT JOIN category pc ON pc.category_id=c.parent_id
         WHERE p.status=1
           AND (s.barcode LIKE ?1 OR s.instore_code LIKE ?1 OR p.spu_name LIKE ?1)
           AND (?4 = '' OR COALESCE(pc.name, c.name, '') = ?4)
           AND (?5 IS NULL OR p.category_id=?5 OR c.parent_id=?5 OR pc.parent_id=?5)
         ORDER BY (s.barcode = ?2) DESC LIMIT ?3",
    )?;
    use rusqlite::params;
    let rows = stmt.query_map(
        params![pattern, keyword, limit, category_group, category_id],
        |r| {
            let spec_json: String = r.get(12)?;
            Ok(ProductHit {
                sku_id: r.get(0)?,
                spu_id: r.get(1)?,
                spu_name: r.get(2)?,
                barcode: r.get(3)?,
                spec_text: checkout_core::catalog_ext::spec_text_from_json(&spec_json),
                price_cents: r.get(4)?,
                is_weighed: r.get::<_, i64>(5)? != 0,
                member_price_cents: r.get(6)?,
                category_name: r.get(9)?,
                category_group: r.get(11)?,
                unit: r.get(10)?,
                image_path: r.get(7)?,
                image_url: r.get(8)?,
            })
        },
    )?;
    // 本地图片库内可能存文件名，统一解析为绝对路径供前端 convertFileSrc 展示。
    let img_dir = data_dir.join("images");
    let mut out = Vec::new();
    for row in rows {
        let mut hit = row?;
        hit.image_path = hit
            .image_path
            .as_ref()
            .map(|f| img_dir.join(f).to_string_lossy().to_string());
        out.push(hit);
    }
    Ok(out)
}

/// 商品识别（F-01）：条码精确匹配，模糊搜索兜底。停用商品不参与识别。
/// limit 供行业场景面板一次装载常用商品；仍限制在上限内，避免界面拖库。
#[tauri::command]
pub fn lookup_product(
    keyword: String,
    limit: Option<i64>,
    category_group: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<ProductHit>> {
    let limit = limit.unwrap_or(20).clamp(1, 500);
    let group = category_group.unwrap_or_default();
    let group = group.trim().to_string();
    // 商品检索虽只读，但会暴露完整价表与毛利敏感信息：必须登录且有收银权限。
    let (_operator_id, conn) = admin_guard(&state, PERM_POS_CHECKOUT)?;
    query_products(&conn, &state.data_dir, &keyword, limit, &group, None)
}

/// 收银点选区：按一级分类装载在售商品，店员无需输入关键字即可点选。
#[tauri::command]
pub fn list_pos_products(
    category_id: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<ProductHit>> {
    let (_operator_id, conn) = admin_guard(&state, PERM_POS_CHECKOUT)?;
    query_products(&conn, &state.data_dir, "", 500, "", category_id)
}

/// 收银分类页签：只返回启用分类，排除行业切换后停用的旧分类树。
#[tauri::command]
pub fn list_pos_categories(
    state: State<'_, AppState>,
) -> Result<Vec<checkout_core::product::Category>> {
    let (_operator_id, conn) = admin_guard(&state, PERM_POS_CHECKOUT)?;
    checkout_core::product::list_categories(&conn)
}

/// 批量取 SKU 会员价：识别会员后刷新购物车行的会员价（前台按会员价展示）。
#[derive(Debug, Serialize)]
pub struct SkuMemberPrice {
    pub sku_id: i64,
    pub member_price_cents: Option<i64>,
}

#[tauri::command]
pub fn member_prices(sku_ids: Vec<i64>, state: State<'_, AppState>) -> Result<Vec<SkuMemberPrice>> {
    let (_operator_id, conn) = admin_guard(&state, PERM_POS_CHECKOUT)?;
    let mut out = Vec::with_capacity(sku_ids.len());
    for sku_id in sku_ids {
        let mp: Option<i64> = conn
            .query_row(
                "SELECT member_price_cents FROM sku WHERE sku_id=?1",
                [sku_id],
                |r| r.get(0),
            )
            .optional()?;
        out.push(SkuMemberPrice {
            sku_id,
            member_price_cents: mp,
        });
    }
    Ok(out)
}

/// 识别顾客付款码（微信 18–19 位 / 支付宝 26–30 位纯数字）。
/// 扫码枪扫到时前端据此走收款而非商品识别；判定规则由后端单点给出，
/// 避免前后端各写一套导致同一串码在两处得到不同结论。
#[tauri::command]
pub fn detect_payment_code(code: String) -> Result<Option<String>> {
    Ok(checkout_core::payment::detect_payment_code(&code).map(|c| c.as_str().to_string()))
}

/// 称重码解析（SCN-02）。
#[tauri::command]
pub fn parse_weigh_barcode(code: String) -> Result<checkout_core::scanner::WeighedCode> {
    checkout_core::scanner::parse_weighed_code(&code)
        .ok_or_else(|| CoreError::Validation("称重店内码非法（校验位或前缀错误）".into()))
}

/// 业务日切点小时（§14.0）：从 system_config 读 "HH:MM"，取小时部分。
fn business_cutoff_hour(conn: &Connection) -> u32 {
    cfg_get(conn, CFG_BUSINESS_DAY_CUTOFF)
        .ok()
        .flatten()
        .and_then(|s| {
            s.split(':')
                .next()
                .and_then(|h| h.trim().parse::<u32>().ok())
        })
        .unwrap_or(0)
}

/// 当前营业日（§14.0）：UTC 先转门店时区（system_config.store_timezone），再按 cutoff 减一天。
/// 所有"今天/当日"判断、落库与查询都必须走这里，否则跨时区/凌晨单会落到错误的营业日。
fn business_date_for(conn: &Connection, utc: &DateTime<Utc>) -> Result<String> {
    let tz_name = cfg_get(conn, CFG_STORE_TIMEZONE)?
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Asia/Shanghai".to_string());
    let tz: chrono_tz::Tz = tz_name
        .parse()
        .map_err(|_| CoreError::Validation(format!("门店时区配置非法: {tz_name}")))?;
    let local = utc.with_timezone(&tz);
    Ok(checkout_core::order::business_date_at(
        local.date_naive(),
        local.hour(),
        business_cutoff_hour(conn),
    ))
}

/// 会员等级（§8.2 会员价取优需要 level_id）。
fn member_level(conn: &Connection, member_id: i64) -> Result<Option<i32>> {
    conn.query_row(
        "SELECT level_id FROM member WHERE member_id=?1 AND status=1",
        [member_id],
        |r| r.get::<_, Option<i32>>(0),
    )
    .optional()
    .map(|o| o.flatten())
    .map_err(Into::into)
}

/// 装载启用且在有效期内的促销规则（§8）。
fn load_active_promotions(conn: &Connection, now: &str) -> Result<Vec<Promotion>> {
    let mut stmt = conn.prepare(
        "SELECT promotion_id, scope, priority, exclusive, stackable, condition_json, action_json
         FROM promotion WHERE status=1
           AND (valid_from IS NULL OR valid_from <= ?1)
           AND (valid_until IS NULL OR valid_until >= ?1)
         ORDER BY priority, promotion_id",
    )?;
    let rows = stmt.query_map([now], |r| {
        let pid: i64 = r.get(0)?;
        let scope_s: String = r.get(1)?;
        let scope = match scope_s.as_str() {
            "SINGLE_ITEM" => PromoScope::SINGLE_ITEM,
            _ => PromoScope::ORDER_LEVEL,
        };
        let cond_s: String = r.get(5)?;
        let act_s: String = r.get(6)?;
        // 坏 JSON 的规则直接跳过：不能当成“无门槛无优惠”的规则参与互斥判断，
        // 也不能因为一条脏配置让整单无法结算。
        let (Ok(cond), Ok(act)) = (
            serde_json::from_str::<serde_json::Value>(&cond_s),
            serde_json::from_str::<serde_json::Value>(&act_s),
        ) else {
            return Ok(None);
        };
        Ok(Some(Promotion {
            promotion_id: pid,
            scope,
            priority: r.get(2)?,
            exclusive: r.get::<_, i64>(3)? != 0,
            stackable: r.get::<_, i64>(4)? != 0,
            min_qty: cond.get("min_qty").and_then(|v| v.as_i64()).unwrap_or(0),
            min_amount_cents: cond
                .get("min_amount_cents")
                .and_then(|v| v.as_i64())
                .unwrap_or(0),
            discount_rate: act.get("discount_rate").and_then(|v| v.as_f64()),
            discount_cents: act.get("discount_cents").and_then(|v| v.as_i64()),
            target_skus: cond
                .get("target_skus")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_i64()).collect())
                .unwrap_or_default(),
        }))
    })?;
    let mut out = Vec::new();
    for row in rows {
        if let Some(p) = row? {
            out.push(p);
        }
    }
    Ok(out)
}

/// 消费一次性二次授权凭据：校验权限匹配、未过期、nonce 一致，随后作废。
fn consume_sensitive(
    state: &AppState,
    permission: &str,
    nonce: Option<String>,
    now: DateTime<Utc>,
) -> Result<rbac::SensitiveAuth> {
    // 敏感操作必须提供授权凭据，禁止省略 nonce 绕过二次校验（§12.4）。
    let nonce = nonce.ok_or(CoreError::SensitiveAuthRequired)?;
    let mut pending = state
        .pending_sensitive
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    // 只清理已过期项，不动其他仍在等待消费的授权。
    pending.retain(|p| p.auth.expires_at > now);
    let idx = pending
        .iter()
        .position(|p| p.permission == permission && p.auth.nonce == nonce)
        .ok_or(CoreError::SensitiveAuthRequired)?;
    let slot = pending.remove(idx); // 一次性消费
    Ok(slot.auth)
}

#[derive(Debug, Deserialize)]
pub struct CartLineInput {
    pub sku_id: i64,
    pub qty: i64,
    /// 养生服务行指定技师；服务端校验技师在册，结算时按该技师比例计提。
    pub technician_id: Option<i64>,
    /// 棋牌计时房费行：服务端按桌台当前计费规则重算，不信任客户端单价。
    pub is_chess_fee: Option<bool>,
}

/// 服务端权威单价：一律取 sku.sale_price_cents，不信任客户端传价
/// （否则改包/旧缓存即可任意改价）。SKU 不存在直接拒绝。
fn sku_sale_price(conn: &Connection, sku_id: i64) -> Result<i64> {
    use rusqlite::OptionalExtension;
    let row: Option<i64> = conn
        .query_row(
            "SELECT s.sale_price_cents FROM sku s JOIN spu p ON p.spu_id=s.spu_id
             WHERE s.sku_id=?1 AND p.status=1",
            [sku_id],
            |r| r.get(0),
        )
        .optional()?;
    row.ok_or_else(|| CoreError::Validation(format!("SKU {sku_id} 不存在或已停用，拒绝下单")))
}

#[derive(Debug, Deserialize)]
pub struct PaymentInput {
    pub channel: String,
    pub amount_cents: i64,
    pub tendered_cents: Option<i64>,
    pub change_cents: Option<i64>,
    pub auth_code: Option<String>,
    /// MICROPAY（默认，收银员扫顾客码）/ NATIVE（顾客扫屏幕码）。
    pub mode: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CheckoutResult {
    pub order_no: String,
    pub status: String,
    /// Native 支付二维码内容（MICROPAY/现金/储值为 None）。
    pub code_url: Option<String>,
    /// 本单会员实际累计的积分（未识别会员/未累计为 0），供收银台回显。
    pub points_earned: i64,
}

/// 订单金额试算结果（只读）。原价 / 优惠 / 应付 + 券可用性提示。
struct AmountPlan {
    cart_lines: Vec<checkout_core::promo::CartLine>,
    applied: Vec<checkout_core::promo::AppliedPromo>,
    total: i64,
    discount: i64,
    payable: i64,
    /// 抹零额（分），已并入 discount；单独返回仅用于界面明示。
    round_off_cents: i64,
    /// 券不可用等提示（不参与金额试算，界面据此提示）。
    coupon_note: Option<String>,
    /// 券不可用原因码（机器可判）：NOT_FOUND / DISABLED / EXPIRED / NOT_STARTED / MEMBER_MISMATCH / BELOW_THRESHOLD。
    /// 券生效或未用券为 None；前端据此区分「券本身无效」与「未达门槛」，只有前者才清除券码。
    coupon_reason: Option<&'static str>,
    /// 未达用券门槛时的差额（分），供收银台做凑单引导；仅 BELOW_THRESHOLD 时有值。
    coupon_shortfall_cents: Option<i64>,
}

/// 计算订单金额（只读，不改任何状态）。
/// 报价接口与 checkout 共用此函数，保证「界面显示的应收」与「实际收款口径」完全一致；
/// 否则前端只按会员价相加会漏掉促销与券，收款金额对不上账。
fn plan_amounts(
    conn: &Connection,
    lines: &[CartLineInput],
    member_id: Option<i64>,
    coupon_code: Option<&str>,
    table_id: Option<i64>,
    now: &str,
) -> Result<AmountPlan> {
    // 1. 行 + 会员价装载（§8.2 会员价参与单品取优）
    let mut cart_lines: Vec<checkout_core::promo::CartLine> = Vec::with_capacity(lines.len());
    let mut chess_fee_seen = false;
    for l in lines {
        if l.is_chess_fee.unwrap_or(false) {
            if chess_fee_seen {
                return Err(CoreError::Validation("棋牌房费行只能有一条".into()));
            }
            chess_fee_seen = true;
            let tid =
                table_id.ok_or_else(|| CoreError::Validation("棋牌房费缺少桌台信息".into()))?;
            let fee = checkout_core::service::chess_fee(conn, tid)?;
            let fee_sku: Option<i64> = conn
                .query_row(
                    "SELECT fee_sku_id FROM service_table WHERE table_id=?1",
                    [tid],
                    |r| r.get(0),
                )
                .optional()?
                .flatten();
            if fee_sku != Some(l.sku_id) {
                return Err(CoreError::Validation("棋牌房费商品与包间配置不一致".into()));
            }
            let mut cl = checkout_core::promo::CartLine::new(l.sku_id, 1, fee.fee_cents)?;
            cl.technician_id = None;
            cart_lines.push(cl);
            continue;
        }
        let mut cl =
            checkout_core::promo::CartLine::new(l.sku_id, l.qty, sku_sale_price(conn, l.sku_id)?)?;
        if member_id.is_some() {
            let mp: Option<i64> = conn
                .query_row(
                    "SELECT member_price_cents FROM sku WHERE sku_id=?1",
                    [l.sku_id],
                    |r| r.get(0),
                )
                .optional()?
                .flatten();
            cl.member_price_cents = mp;
        }
        if let Some(tid) = l.technician_id {
            let (is_service, tech_ok): (i64, i64) = conn.query_row(
                "SELECT (SELECT COUNT(*) FROM sku WHERE sku_id=?1 AND is_service=1),
                        (SELECT COUNT(*) FROM technician WHERE technician_id=?2 AND status=1)",
                params![l.sku_id, tid],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            if is_service == 0 {
                return Err(CoreError::Validation("只有服务项目可以指定技师".into()));
            }
            if tech_ok == 0 {
                return Err(CoreError::Validation("指定的技师不存在或已停用".into()));
            }
            cl.technician_id = Some(tid);
        }
        cart_lines.push(cl);
    }
    // 2. 促销计算（§8）
    let level = member_id
        .map(|mid| member_level(conn, mid))
        .transpose()?
        .flatten();
    let promos = load_active_promotions(conn, now)?;
    let max_ratio = cfg_f64(conn, CFG_MAX_DISCOUNT_RATIO, DEFAULT_MAX_DISCOUNT_RATIO);
    let promo_result =
        checkout_core::promo::calc_promotions(&mut cart_lines, level, &promos, max_ratio)?;
    // 3. 券（只读试算；状态迁移由 checkout 负责）
    let mut coupon_discount: i64 = 0;
    let mut coupon_note: Option<String> = None;
    let mut coupon_reason: Option<&'static str> = None;
    let mut coupon_shortfall_cents: Option<i64> = None;
    if let Some(code) = coupon_code.map(str::trim).filter(|c| !c.is_empty()) {
        let coupon: Option<CouponQueryRow> = conn
            .query_row(
                "SELECT discount_cents,min_order_cents,member_id,status,valid_from,valid_until
                 FROM coupon WHERE coupon_code=?1",
                [code],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                    ))
                },
            )
            .optional()?;
        match coupon {
            None => {
                coupon_note = Some("优惠券不存在".into());
                coupon_reason = Some("NOT_FOUND");
            }
            Some((amount, min_order, coupon_member, status, valid_from, valid_until)) => {
                let expired = valid_until.as_deref().map(|v| v < now).unwrap_or(false);
                let not_started = valid_from.as_deref().map(|v| v > now).unwrap_or(false);
                // 专属券必须识别对应会员才可用；通用券（member_id 为 NULL）任何人可用。
                // 报价与结算共用本函数，归属校验同时覆盖两端，避免「专属券被陌生人核销」。
                let owner_mismatch = coupon_member.is_some() && coupon_member != member_id;
                if status != COUPON_UNUSED {
                    coupon_note = Some("优惠券不可用".into());
                    coupon_reason = Some("DISABLED");
                } else if expired {
                    coupon_note = Some("优惠券已过期".into());
                    coupon_reason = Some("EXPIRED");
                } else if not_started {
                    coupon_note = Some("优惠券尚未生效".into());
                    coupon_reason = Some("NOT_STARTED");
                } else if owner_mismatch {
                    coupon_note = Some("此券为会员专属券，请识别对应会员后使用".into());
                    coupon_reason = Some("MEMBER_MISMATCH");
                } else {
                    let after_promo: i64 = cart_lines.iter().map(|l| l.payable()).sum();
                    if after_promo < min_order {
                        coupon_note = Some("订单金额未达用券门槛".into());
                        coupon_reason = Some("BELOW_THRESHOLD");
                        // 差额按促销后金额计算，与门槛判定同口径，供界面提示「再买 X 元」
                        coupon_shortfall_cents = Some(min_order - after_promo);
                    } else {
                        coupon_discount = amount.min(after_promo);
                    }
                }
            }
        }
    }
    let total: i64 = cart_lines.iter().map(|l| l.line_total_cents).sum();
    let mut discount: i64 = promo_result.total_discount_cents + coupon_discount;
    // 抹零在促销与券之后作用于最终应收，并计入 discount，
    // 保证 order::begin_order 的 `pay = total - discount` 恒等式不变。
    let round_mode: String = conn
        .query_row(
            "SELECT config_value FROM system_config WHERE config_key=?1",
            [checkout_core::constants::CFG_ROUND_OFF_MODE],
            |r| r.get(0),
        )
        .unwrap_or_else(|_| "OFF".to_string());
    let round_off = checkout_core::money::round_off_cents(
        (total - discount).max(0),
        &round_mode,
    );
    discount += round_off;
    Ok(AmountPlan {
        applied: promo_result.applied,
        cart_lines,
        total,
        discount,
        payable: total - discount,
        round_off_cents: round_off,
        coupon_note,
        coupon_reason,
        coupon_shortfall_cents,
    })
}

/// 收银台报价：返回后端口径的应收金额，界面「合计/待收」以此为准。
#[derive(Debug, Serialize)]
pub struct OrderQuote {
    pub total_cents: i64,
    pub discount_cents: i64,
    pub payable_cents: i64,
    /// 抹零额（分），已包含在 discount_cents 中。
    pub round_off_cents: i64,
    /// 券不可用等提示（不阻断报价）。
    pub coupon_note: Option<String>,
    /// 券不可用原因码：NOT_FOUND / DISABLED / EXPIRED / NOT_STARTED / MEMBER_MISMATCH / BELOW_THRESHOLD；券生效或未用券为 None。
    pub coupon_reason: Option<String>,
    /// 未达门槛差额（分），仅 BELOW_THRESHOLD 时有值，供收银台凑单引导。
    pub coupon_shortfall_cents: Option<i64>,
    /// 本单识别会员后预计累计的积分（未识别会员/积分关闭为 0）。
    pub points_earned: i64,
}

#[tauri::command]
pub fn quote_order(
    lines: Vec<CartLineInput>,
    member_id: Option<i64>,
    coupon_code: Option<String>,
    table_id: Option<i64>,
    state: State<'_, AppState>,
) -> Result<OrderQuote> {
    let license = state
        .license
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    license.gate(ENT_POS_CORE)?;
    let session = state
        .session
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    let session = session.as_ref().ok_or(CoreError::NotAuthenticated)?;
    let now_dt = parse_rfc3339(&now_str())?;
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    // 报价接口同样要登录 + pos.checkout：否则未登录即可批量探测商品价格与促销结果。
    checkout_core::rbac::guard(
        &conn,
        &checkout_core::rbac::GuardRequest {
            entitlement: ENT_POS_CORE,
            permission: PERM_POS_CHECKOUT,
            session,
            license: &license,
            sensitive_auth: None,
            current_nonce: "",
            now: now_dt,
            amount_cents: 0,
            limit_key: None,
        },
    )?;
    let _ = session;
    drop(license);
    let plan = plan_amounts(
        &conn,
        &lines,
        member_id,
        coupon_code.as_deref(),
        table_id,
        &now_str(),
    )?;
    let points_earned = estimate_points(&conn, member_id, plan.payable);
    Ok(OrderQuote {
        total_cents: plan.total,
        discount_cents: plan.discount,
        payable_cents: plan.payable,
        round_off_cents: plan.round_off_cents,
        coupon_note: plan.coupon_note,
        coupon_reason: plan.coupon_reason.map(ToString::to_string),
        coupon_shortfall_cents: plan.coupon_shortfall_cents,
        points_earned,
    })
}

/// 收银结算（F-05/F-06）。现金本地闭环；储值走领域扣款；扫码渠道走网关状态机。
/// 生成未被占用的订单号：极小概率撞号时重试，避免主键冲突把整单收银打回。
fn generate_unique_order_no(conn: &Connection, store_no: &str, device_no: &str) -> Result<String> {
    for _ in 0..8 {
        let cand = checkout_core::order::generate_order_no(store_no, device_no);
        let used: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sales_order WHERE order_no=?1",
            [&cand],
            |r| r.get(0),
        )?;
        if used == 0 {
            return Ok(cand);
        }
    }
    Err(CoreError::Other("订单号连续冲突，请重试".into()))
}

/// 促销引擎计算行/整单优惠并落 order_promotion；tx A → 支付 → tx B。
/// 授权门禁只校验 entitlement，任何非 UNACTIVATED 状态不阻断 pos.core。
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn checkout(
    lines: Vec<CartLineInput>,
    payments: Vec<PaymentInput>,
    member_id: Option<i64>,
    coupon_code: Option<String>,
    table_code: Option<String>,
    table_id: Option<i64>,
    state: State<'_, AppState>,
) -> Result<CheckoutResult> {
    let license = state
        .license
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    license.gate(ENT_POS_CORE)?;
    if member_id.is_some() {
        license.gate(ENT_MEMBER_BASIC)?;
    }
    let session = state
        .session
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    let session = session.as_ref().ok_or(CoreError::NotAuthenticated)?;
    let now_dt = parse_rfc3339(&now_str())?;
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    // 开单权限校验：此前只查 entitlement，任何登录账号都能开单，pos.checkout 形同虚设。
    checkout_core::rbac::guard(
        &conn,
        &checkout_core::rbac::GuardRequest {
            entitlement: ENT_POS_CORE,
            permission: PERM_POS_CHECKOUT,
            session,
            license: &license,
            sensitive_auth: None,
            current_nonce: "",
            now: now_dt,
            amount_cents: 0,
            limit_key: None,
        },
    )?;
    let cashier_id = session.user_id;
    let _ = session;
    drop(license);

    if lines.is_empty() {
        return Err(CoreError::Validation("购物车为空".into()));
    }

    // 渠道开关校验（设置页支付渠道开关）
    for p in &payments {
        let key = match p.channel.as_str() {
            PAY_CASH => CFG_PAY_CHANNEL_CASH,
            PAY_STORED_VALUE => CFG_PAY_CHANNEL_SV,
            PAY_WECHAT => CFG_PAY_CHANNEL_WECHAT,
            PAY_ALIPAY => CFG_PAY_CHANNEL_ALIPAY,
            other => return Err(CoreError::Validation(format!("未知支付渠道: {other}"))),
        };
        if cfg_get(&conn, key)?.map(|v| v == "0").unwrap_or(false) {
            return Err(CoreError::Validation(format!(
                "支付渠道 {} 已在设置中停用",
                p.channel
            )));
        }
        // 单笔支付自洽校验：负数/零元拆分、现金找零不等于实收都会让渠道层先收钱后报错。
        if p.amount_cents <= 0 {
            return Err(CoreError::Validation("单笔支付金额必须大于 0".into()));
        }
        if p.channel == PAY_CASH {
            let tendered = p.tendered_cents.unwrap_or(p.amount_cents);
            let change = p.change_cents.unwrap_or(0);
            if tendered < p.amount_cents || change < 0 || tendered - change != p.amount_cents {
                return Err(CoreError::Validation(
                    "现金支付：实收 - 找零必须等于本笔金额".into(),
                ));
            }
        } else if p.change_cents.unwrap_or(0) != 0 {
            return Err(CoreError::Validation("非现金支付不允许找零".into()));
        }
        // 扫码支付（MICROPAY）必须携带顾客付款码：没有付款码就没有扣款依据，
        // 不能让订单凭空走完收款流程。
        let is_scan_channel = p.channel == PAY_WECHAT || p.channel == PAY_ALIPAY;
        if is_scan_channel
            && p.mode.as_deref() != Some(PAY_MODE_NATIVE)
            && p.auth_code
                .as_deref()
                .map(str::trim)
                .unwrap_or("")
                .is_empty()
        {
            return Err(CoreError::Validation(
                "扫码支付需要顾客付款码：请用扫码枪扫描顾客付款码".into(),
            ));
        }
    }
    // 一个订单只能有一笔 Native（顾客扫屏幕码）：多笔 Native 只会展示最后一个码，
    // 其余永远等不到支付，订单会挂在 PENDING 且现金部分已收。
    let native_count = payments
        .iter()
        .filter(|p| p.mode.as_deref() == Some(PAY_MODE_NATIVE))
        .count();
    if native_count > 1 {
        return Err(CoreError::Validation(
            "一笔订单只支持一个 Native 扫码支付，请拆单".into(),
        ));
    }
    if native_count == 1 && payments.len() > 1 {
        return Err(CoreError::Validation(
            "Native 扫码支付不支持与其他支付方式混合，请拆单".into(),
        ));
    }

    // 金额试算（与报价接口共用同一口径，§8）
    let now = now_str();
    // 券过期标记：只在结算路径落库，报价接口保持只读
    let coupon_key = coupon_code
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty());
    if let Some(code) = coupon_key {
        checkout_core::member::expire_lazy(&conn, code, &now)?;
    }
    let plan = plan_amounts(
        &conn,
        &lines,
        member_id,
        coupon_code.as_deref(),
        table_id,
        &now,
    )?;
    if let Some(note) = &plan.coupon_note {
        return Err(CoreError::Validation(note.clone()));
    }
    // 券核销（§8.3 第 3 层，§13.7 状态机）：试算已校验可用性，
    // 锁定与订单落库必须同一事务，否则落库失败会留下永久 LOCKED 的券。
    let cart_lines = plan.cart_lines;
    let total = plan.total;
    let discount = plan.discount;
    let payable = plan.payable;
    if payable < 0 {
        return Err(CoreError::Validation("优惠后应付金额为负".into()));
    }
    let paid: i64 = payments.iter().map(|p| p.amount_cents).sum();
    if paid != payable {
        return Err(CoreError::Validation(format!(
            "支付金额与应付不一致: 应付 {payable}, 实付 {paid}"
        )));
    }
    if paid > payable {
        // 多收的钱没有对应科目，直接吞掉会与应收对不上账。
        return Err(CoreError::Validation(format!(
            "支付金额超出应收: 应付 {payable}, 实付 {paid}"
        )));
    }

    let store_no = cfg_str(&conn, CFG_STORE_NO, "STORE");
    let device_no = cfg_str(&conn, CFG_DEVICE_NO, "POS1");
    let order_no = generate_unique_order_no(&conn, &store_no, &device_no)?;
    let biz = business_date_for(&conn, &now_dt)?;
    let shift_id: i64 = *state
        .current_shift
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;

    // tx A：落库 PENDING_PAYMENT + 促销明细 + 全部支付流水 PENDING。
    // 支付流水先记 PENDING 是崩溃恢复的锚点（§23）：任何时刻断电，
    // 恢复扫描都能凭 out_trade_no 向渠道查单收敛，不丢单不重收。
    {
        let tx = conn.unchecked_transaction()?;
        checkout_core::order::begin_order(
            &tx,
            &order_no,
            "POS1",
            cashier_id,
            shift_id,
            member_id,
            &cart_lines,
            total,
            discount,
            &biz,
            &now,
        )?;
        if let Some(tc) = table_code
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
        {
            if tc.chars().count() > 12 {
                return Err(CoreError::Validation("桌台编码过长".into()));
            }
            let exists: i64 = tx.query_row(
                "SELECT COUNT(*) FROM service_table WHERE code=?1 AND status='OCCUPIED'",
                [tc],
                |r| r.get(0),
            )?;
            if exists == 0 {
                return Err(CoreError::Validation(format!("桌台 {tc} 不存在或未开台")));
            }
            tx.execute(
                "UPDATE sales_order SET table_code=?1 WHERE order_no=?2",
                rusqlite::params![tc, order_no],
            )?;
        }
        if let Some(code) = coupon_key {
            checkout_core::member::transition_coupon(&tx, code, "LOCKED")?;
        }
        for applied in &plan.applied {
            tx.execute(
                "INSERT INTO order_promotion(order_no,promotion_id,discount_cents,detail_json)
                 VALUES (?1,?2,?3,?4)",
                rusqlite::params![
                    order_no,
                    applied.promotion_id,
                    applied.discount_cents,
                    serde_json::to_string(&applied.detail)?
                ],
            )?;
        }
        for (i, p) in payments.iter().enumerate() {
            let channel_str = p.channel.as_str();
            let out_trade_no = checkout_core::payment::make_out_trade_no(&order_no, i as u32 + 1);
            let net = if channel_str == PAY_CASH {
                p.tendered_cents.unwrap_or(p.amount_cents) - p.change_cents.unwrap_or(0)
            } else {
                p.amount_cents
            };
            let mode = p.mode.as_deref().unwrap_or(
                if channel_str == PAY_CASH || channel_str == PAY_STORED_VALUE {
                    ""
                } else {
                    PAY_MODE_MICROPAY
                },
            );
            tx.execute(
                "INSERT INTO payment_record(order_no,pay_channel,pay_mode,attempt_no,amount_cents,cash_tendered_cents,change_cents,out_trade_no,status,created_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'PENDING',?9)",
                rusqlite::params![
                    order_no,
                    channel_str,
                    mode,
                    i as i64 + 1,
                    net,
                    p.tendered_cents,
                    p.change_cents,
                    out_trade_no,
                    now
                ],
            )?;
        }
        // 券码记到订单（Native/崩溃恢复的 finalize 需要）
        if let Some(code) = coupon_code.as_deref() {
            tx.execute(
                "UPDATE sales_order SET coupon_code=?1 WHERE order_no=?2",
                rusqlite::params![code, order_no],
            )?;
        }
        tx.commit()?;
    }

    // Native 存在与否决定储值扣款时点（§PAY-08：外部先行、储值后扣）
    let has_native = payments.iter().any(|p| {
        p.mode.as_deref() == Some(PAY_MODE_NATIVE)
            && (p.channel == PAY_WECHAT || p.channel == PAY_ALIPAY)
    });

    // 支付执行：每个渠道的「扣款 + 流水置成功」各自成事务，避免半状态。
    // 任一渠道失败：已成功的渠道必须显式回滚，并把订单号与逐笔结果返回给操作员；
    // 不能只抛一个"扫码支付未完成"就结束——那会让后续渠道不执行、订单挂 PENDING，
    // 操作员重试还会重复收款。
    let mut native_code_url: Option<String> = None;
    let pay_result: Result<()> = (|| {
        for (i, p) in payments.iter().enumerate() {
            let channel = match p.channel.as_str() {
                PAY_CASH => PayChannel::CASH,
                PAY_STORED_VALUE => PayChannel::STORED_VALUE,
                PAY_WECHAT => PayChannel::WECHAT,
                PAY_ALIPAY => PayChannel::ALIPAY,
                other => return Err(CoreError::Validation(format!("未知支付渠道: {other}"))),
            };
            let out_trade_no = checkout_core::payment::make_out_trade_no(&order_no, i as u32 + 1);
            let net = if channel == PayChannel::CASH {
                p.tendered_cents.unwrap_or(p.amount_cents) - p.change_cents.unwrap_or(0)
            } else {
                p.amount_cents
            };

            match channel {
                PayChannel::STORED_VALUE => {
                    if has_native {
                        // 储值后扣：等 Native 成功后在 finalize 中扣款
                        continue;
                    }
                    let mid = member_id
                        .ok_or_else(|| CoreError::Validation("储值支付需先识别会员".into()))?;
                    // 扣款与流水置成功必须同一事务：否则储值已扣、流水仍 PENDING，
                    // 崩溃恢复会把已付订单当作未付再次收银。
                    let tx = conn.unchecked_transaction()?;
                    let d = checkout_core::member::deduct(&tx, mid, &order_no, net)?;
                    let n = tx.execute(
                        "UPDATE payment_record SET status='SUCCESS', sv_bonus_cents=?1, sv_principal_cents=?2, paid_at=?3
                         WHERE order_no=?4 AND out_trade_no=?5",
                        rusqlite::params![d.bonus_cents, d.principal_cents, now, order_no, out_trade_no],
                    )?;
                    if n != 1 {
                        return Err(CoreError::Other("储值支付流水状态异常".into()));
                    }
                    tx.commit()?;
                }
                PayChannel::WECHAT | PayChannel::ALIPAY => {
                    let is_native = p.mode.as_deref() == Some(PAY_MODE_NATIVE);
                    let mode = if is_native {
                        PayMode::NATIVE
                    } else {
                        PayMode::MICROPAY
                    };
                    let req = PayRequest {
                        out_trade_no: out_trade_no.clone(),
                        amount_cents: net,
                        auth_code: p.auth_code.clone(),
                        mode,
                        channel,
                    };
                    if is_native {
                        // Native：预下单拿 code_url 即返回，顾客扫码后由 poll/confirm 收敛
                        let outcome = state.gateway.native_precreate(&req)?;
                        native_code_url = outcome.code_url;
                        continue;
                    }
                    let poll = cfg_i64(&conn, CFG_MICROPAY_POLL_MS, DEFAULT_MICROPAY_POLL_MS as i64)
                        as u64;
                    let timeout = cfg_i64(
                        &conn,
                        CFG_MICROPAY_TIMEOUT_S,
                        DEFAULT_MICROPAY_TIMEOUT_S as i64,
                    ) as u64;
                    let out = run_micropay(&state.gateway, &req, poll, timeout, &mut |ms| {
                        std::thread::sleep(std::time::Duration::from_millis(ms));
                    })?;
                    match out.state {
                        PaymentState::SUCCESS => {
                            conn.execute(
                                "UPDATE payment_record SET status='SUCCESS', trade_no=?1, paid_at=?2
                                 WHERE order_no=?3 AND out_trade_no=?4",
                                rusqlite::params![out.trade_no, now, order_no, out_trade_no],
                            )?;
                        }
                        other => {
                            // 明确失败/关闭 → 本笔置 CLOSED；由外层统一回滚已成功的渠道
                            let _ = conn.execute(
                                "UPDATE payment_record SET status='CLOSED' WHERE order_no=?1 AND out_trade_no=?2",
                                rusqlite::params![order_no, out_trade_no],
                            );
                            return Err(CoreError::Validation(format!(
                                "扫码支付未完成: {other:?}"
                            )));
                        }
                    }
                }
                PayChannel::CASH => {
                    // 现金在收银台已实际收取：必须把本笔 PENDING 流水收敛为 SUCCESS，
                    // 否则 commit_order 只统计 SUCCESS，现金单永远无法完成。
                    // 复用 core 的同名函数，使这一不变式可被回归测试直接覆盖。
                    let tx = conn.unchecked_transaction()?;
                    checkout_core::order::confirm_cash_payment(
                        &tx,
                        &order_no,
                        &out_trade_no,
                        &now,
                    )?;
                    tx.commit()?;
                }
            }
        }
        Ok(())
    })();
    if let Err(e) = pay_result {
        let rollback = rollback_partial_payments(&conn, &state.gateway, &order_no)?;
        return Err(CoreError::Validation(format!(
            "支付未完成（订单号 {order_no}）：{e}。{rollback}"
        )));
    }

    if has_native {
        // 券保持 LOCKED，等 finalize 收敛为 REDEEMED（取消则回 UNUSED）
        return Ok(CheckoutResult {
            order_no,
            status: "PENDING_PAYMENT".into(),
            code_url: native_code_url,
            points_earned: 0,
        });
    }

    let points_earned = finalize_order_assets(&conn, &order_no)?;
    auto_print_receipt(&conn, &order_no);

    Ok(CheckoutResult {
        order_no,
        status: "COMPLETED".into(),
        code_url: None,
        points_earned,
    })
}

/// 支付中途失败的收口：
/// - 储值：按原扣减构成原路补回，流水置 CLOSED；
/// - 渠道：按真实金额发起原路退款，只有渠道确认 SUCCESS 才置 CLOSED；
/// - 现金：钱已在钱箱，流水置 CLOSED 并明确要求退还顾客。
///
/// 全部渠道都已关闭且不需要人工处理时，订单转 CANCELED 并释放券；
/// 否则保留 PENDING_PAYMENT 与成功流水，交给对账/人工继续处理。
fn rollback_partial_payments(
    conn: &Connection,
    gateway: &checkout_core::payment::RoutingGateway,
    order_no: &str,
) -> Result<String> {
    use checkout_core::money::cents_to_yuan;
    use checkout_core::payment::Channel;

    let rows: Vec<PaymentRow> = {
        let mut stmt = conn.prepare(
            "SELECT pay_channel, out_trade_no, amount_cents, sv_bonus_cents, sv_principal_cents
             FROM payment_record WHERE order_no=?1 AND status='SUCCESS' ORDER BY pay_id",
        )?;
        let rows = stmt.query_map([order_no], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()?
    };
    let member_id: Option<i64> = conn
        .query_row(
            "SELECT member_id FROM sales_order WHERE order_no=?1",
            [order_no],
            |r| r.get(0),
        )
        .optional()?
        .flatten();

    let mut notes: Vec<String> = Vec::new();
    let mut all_closed = true;
    for (channel, out_trade_no, amount, sv_bonus, sv_principal) in rows {
        let out_trade_no = out_trade_no.unwrap_or_default();
        match channel.as_str() {
            PAY_CASH => {
                conn.execute(
                    "UPDATE payment_record SET status='CLOSED' WHERE order_no=?1 AND out_trade_no=?2",
                    rusqlite::params![order_no, out_trade_no],
                )?;
                notes.push(format!(
                    "现金 {} 元已收，请从钱箱退还顾客",
                    cents_to_yuan(amount)
                ));
            }
            PAY_STORED_VALUE => match member_id {
                Some(mid) => {
                    let tx = conn.unchecked_transaction()?;
                    checkout_core::member::reverse_sale(
                        &tx,
                        mid,
                        order_no,
                        checkout_core::member::Deduction {
                            bonus_cents: sv_bonus.unwrap_or(0),
                            principal_cents: sv_principal.unwrap_or(0),
                        },
                    )?;
                    tx.execute(
                            "UPDATE payment_record SET status='CLOSED' WHERE order_no=?1 AND out_trade_no=?2",
                            rusqlite::params![order_no, out_trade_no],
                        )?;
                    tx.commit()?;
                    notes.push(format!("储值 {} 元已退回会员账户", cents_to_yuan(amount)));
                }
                None => {
                    all_closed = false;
                    notes.push("储值支付缺少会员信息，需人工退回".into());
                }
            },
            _ => {
                let ch = if channel == PAY_WECHAT {
                    Channel::WECHAT
                } else {
                    Channel::ALIPAY
                };
                let refund_no = format!("{out_trade_no}:ROLLBACK");
                match gateway
                    .of_channel(ch)
                    .refund(&out_trade_no, &refund_no, amount)
                {
                    Ok(PaymentState::SUCCESS) => {
                        conn.execute(
                            "UPDATE payment_record SET status='CLOSED' WHERE order_no=?1 AND out_trade_no=?2",
                            rusqlite::params![order_no, out_trade_no],
                        )?;
                        notes.push(format!("{channel} {} 元已原路退回", cents_to_yuan(amount)));
                    }
                    Ok(other) => {
                        all_closed = false;
                        notes.push(format!(
                            "{channel} 退款状态 {other:?}，资金可能已收，请人工核对"
                        ));
                    }
                    Err(e) => {
                        all_closed = false;
                        notes.push(format!("{channel} 自动退款失败（{e}），请人工处理"));
                    }
                }
            }
        }
    }

    if all_closed {
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE sales_order SET status='CANCELED' WHERE order_no=?1 AND status='PENDING_PAYMENT'",
            [order_no],
        )?;
        let coupon: Option<String> = tx
            .query_row(
                "SELECT coupon_code FROM sales_order WHERE order_no=?1",
                [order_no],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        if let Some(code) = coupon {
            // 券可能已 LOCKED，取消订单必须释放回 UNUSED
            let _ = checkout_core::member::transition_coupon(&tx, &code, COUPON_UNUSED);
        }
        tx.commit()?;
        notes.push("已全额回滚，订单已取消，可直接重新收款".into());
    } else {
        notes.push("订单保留待处理状态，请按对账/人工退款流程收口".into());
    }

    if notes.is_empty() {
        return Ok("没有已成功的支付需要回滚".into());
    }
    Ok(format!("处理结果：{}", notes.join("；")))
}

/// 订单资产收口（§13.3/§13.4/§13.7 + 订单提交）：在全部渠道支付确认后执行。
/// - PENDING 储值流水补扣款（外部先行、储值后扣）
/// - 积分累积（赠送金抵扣部分不计分）
/// - 券 UNUSED→REDEEMED
/// - commit_order（库存扣减 + 订单完成）
fn finalize_order_assets(conn: &Connection, order_no: &str) -> Result<i64> {
    let (member_id, total, discount, status, coupon_code): (
        Option<i64>,
        i64,
        i64,
        String,
        Option<String>,
    ) = conn.query_row(
        "SELECT member_id, total_cents, discount_cents, status, coupon_code
         FROM sales_order WHERE order_no=?1",
        [order_no],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
    )?;
    if status != "PENDING_PAYMENT" {
        // 幂等：已完成的订单直接返回，不再重复累计积分
        return Ok(0);
    }
    let now = now_str();
    // 整个资产收口（储值补扣、积分、券、订单提交）必须原子：
    // 任何一步失败都不能留下"已扣储值但订单未完成"的半状态。
    let tx = conn.unchecked_transaction()?;

    // PENDING 储值流水补扣（与流水置成功同事务，见 member::append_asset_log 约定）
    let pending_sv: Vec<(i64, i64, String)> = {
        let mut stmt = tx.prepare(
            "SELECT pay_id, amount_cents, out_trade_no FROM payment_record
             WHERE order_no=?1 AND pay_channel='STORED_VALUE' AND status='PENDING'",
        )?;
        let rows = stmt.query_map([order_no], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()?
    };
    let mut bonus_used: i64 = 0;
    for (_pay_id, amount, out_trade_no) in pending_sv {
        let mid = member_id.ok_or_else(|| CoreError::Validation("储值支付需先识别会员".into()))?;
        let d = checkout_core::member::deduct(&tx, mid, order_no, amount)?;
        bonus_used += d.bonus_cents;
        tx.execute(
            "UPDATE payment_record SET status='SUCCESS', sv_bonus_cents=?1, sv_principal_cents=?2, paid_at=?3
             WHERE order_no=?4 AND out_trade_no=?5",
            rusqlite::params![d.bonus_cents, d.principal_cents, now, order_no, out_trade_no],
        )?;
    }

    // 积分累积（§13.4）：受「启用会员积分」开关控制；赠送金抵扣部分不计分。
    let mut points_earned: i64 = 0;
    if let Some(mid) = member_id {
        if points_enabled(&tx) {
            let per_yuan = cfg_i64(&tx, CFG_POINTS_PER_YUAN, DEFAULT_POINTS_PER_YUAN).max(0);
            let payable = total - discount;
            points_earned = checkout_core::member::accrue_points(
                &tx,
                mid,
                order_no,
                payable - bonus_used,
                per_yuan,
            )?;
        }
    }

    // 券 UNUSED→REDEEMED + 订单提交
    if let Some(code) = coupon_code.as_deref() {
        checkout_core::member::transition_coupon(&tx, code, "REDEEMED")?;
    }
    let cashier: i64 = tx.query_row(
        "SELECT cashier_id FROM sales_order WHERE order_no=?1",
        [order_no],
        |r| r.get(0),
    )?;
    checkout_core::order::commit_order(&tx, order_no, cashier)?;
    tx.commit()?;
    Ok(points_earned)
}

#[tauri::command]
pub fn park_cart(
    cart_json: String,
    remark: Option<String>,
    state: State<'_, AppState>,
) -> Result<i64> {
    let (cashier, conn) = admin_guard(&state, PERM_POS_CHECKOUT)?;
    checkout_core::order::park_cart(
        &conn,
        &cart_json,
        cashier,
        None,
        remark.as_deref(),
        &now_str(),
    )
}

#[tauri::command]
pub fn list_parked(state: State<'_, AppState>) -> Result<Vec<serde_json::Value>> {
    let (_operator_id, conn) = admin_guard(&state, PERM_POS_CHECKOUT)?;
    let mut stmt = conn.prepare(
        "SELECT park_id, cart_json, status, created_at FROM parked_order WHERE status='PENDING' ORDER BY park_id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(serde_json::json!({
            "park_id": r.get::<_, i64>(0)?,
            "cart_json": r.get::<_, String>(1)?,
            "status": r.get::<_, String>(2)?,
            "created_at": r.get::<_, String>(3)?,
        }))
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

#[tauri::command]
pub fn resume_cart(park_id: i64, state: State<'_, AppState>) -> Result<String> {
    let (_operator_id, conn) = admin_guard(&state, PERM_POS_CHECKOUT)?;
    checkout_core::order::resume_cart(&conn, park_id, &now_str())
}

#[derive(Debug, Serialize)]
pub struct DailyReport {
    pub order_count: i64,
    pub total_sales_cents: i64,
    pub refund_cents: i64,
    pub discount_cents: i64,
}

#[tauri::command]
pub fn daily_report(date: String, state: State<'_, AppState>) -> Result<DailyReport> {
    // 日营业报表属敏感财务数据：需要报表查看权限，不能只凭"已登录"。
    let (_operator_id, conn) = admin_guard(&state, PERM_REPORT_VIEW)?;
    let d = checkout_core::report::generate_daily_summary(&conn, &date, &now_str())?;
    Ok(DailyReport {
        order_count: d.order_count,
        total_sales_cents: d.total_sales_cents,
        refund_cents: d.refund_cents,
        discount_cents: d.discount_cents,
    })
}

/// 打印：用订单真实数据渲染 ESC/POS 字节流并发往网络打印机，失败入重打队列，不阻断（§7）。
#[tauri::command]
pub async fn print_receipt(order_no: String, state: State<'_, AppState>) -> Result<String> {
    let (_operator_id, conn) = admin_guard(&state, PERM_POS_CHECKOUT)?;
    // 「结算后自动打印小票」关闭时不打印，含手动重打入口
    if cfg_get(&conn, CFG_PRINT_AUTO).ok().flatten().as_deref() != Some("1") {
        return Err(CoreError::Validation("结算后自动打印小票已关闭".into()));
    }
    print_receipt_inner(&conn, order_no.trim())
}

/// 读取「结算后自动打印小票」开关，供收银界面决定重打入口是否可见。
#[tauri::command]
pub fn get_print_auto(state: State<'_, AppState>) -> Result<bool> {
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    Ok(cfg_get(&conn, CFG_PRINT_AUTO).ok().flatten().as_deref() == Some("1"))
}

/// 按「结算后自动打印小票」开关打印；失败不阻断收银（内部已入重打队列）。
fn auto_print_receipt(conn: &Connection, order_no: &str) {
    if cfg_get(conn, CFG_PRINT_AUTO).ok().flatten().as_deref() != Some("1") {
        return;
    }
    // 未配置打印机地址：不做自动打印，也不污染重打队列
    let host = cfg_get(conn, CFG_PRINT_HOST)
        .ok()
        .flatten()
        .unwrap_or_default();
    if host.trim().is_empty() {
        return;
    }
    let _ = print_receipt_inner(conn, order_no);
}

/// 生成并发送小票（手动重打与自动打印共用同一路径）。
fn print_receipt_inner(conn: &Connection, order_no: &str) -> Result<String> {
    use checkout_core::escpos::{enqueue_retry, render_receipt, Template};
    use checkout_core::money::cents_to_yuan;
    let content: String = conn.query_row(
        "SELECT content_json FROM print_template WHERE template_id='receipt_default'",
        [],
        |r| r.get(0),
    )?;
    let template = Template {
        template_id: "receipt_default".into(),
        paper_width_mm: 58,
        sections: serde_json::from_str(&content)?,
    };

    // 订单真实数据
    let (total, discount, pay, created, member_id, table_code): (
        i64,
        i64,
        i64,
        String,
        Option<i64>,
        Option<String>,
    ) = conn.query_row(
        "SELECT total_cents,discount_cents,pay_cents,created_at,member_id,table_code
             FROM sales_order WHERE order_no=?1",
        [order_no],
        |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
            ))
        },
    )?;
    let member_phone: String = match member_id {
        Some(mid) => conn
            .query_row(
                "SELECT COALESCE(phone,'') FROM member WHERE member_id=?1",
                [mid],
                |r| r.get(0),
            )
            .unwrap_or_default(),
        None => String::new(),
    };
    let channel_lines: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT pay_channel,amount_cents,cash_tendered_cents,change_cents
             FROM payment_record WHERE order_no=?1 AND status='SUCCESS' ORDER BY pay_id",
        )?;
        let rows = stmt.query_map([order_no], |r| {
            let ch: String = r.get(0)?;
            let amt: i64 = r.get(1)?;
            let change: Option<i64> = r.get(3)?;
            let line = if ch == PAY_CASH {
                format!(
                    "{} 收{} 找{}",
                    ch,
                    cents_to_yuan(r.get::<_, Option<i64>>(2)?.unwrap_or(amt)),
                    cents_to_yuan(change.unwrap_or(0))
                )
            } else {
                format!("{} {}", ch, cents_to_yuan(amt))
            };
            Ok(line)
        })?;
        let mut v = Vec::new();
        for row in rows {
            v.push(row?);
        }
        v
    };
    let change_total: i64 = conn.query_row(
        "SELECT COALESCE(SUM(change_cents),0) FROM payment_record
         WHERE order_no=?1 AND status='SUCCESS' AND pay_channel=?2",
        rusqlite::params![order_no, PAY_CASH],
        |r| r.get(0),
    )?;
    let items: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT COALESCE(p.spu_name,'商品'),i.qty,i.line_total_cents-i.discount_cents
             FROM sales_order_item i
             JOIN sku s ON s.sku_id=i.sku_id JOIN spu p ON p.spu_id=s.spu_id
             WHERE i.order_no=?1 AND i.is_gift=0 ORDER BY i.item_id",
        )?;
        let rows = stmt.query_map([order_no], |r| {
            let name: String = r.get(0)?;
            let qty: i64 = r.get(1)?;
            let amt: i64 = r.get(2)?;
            Ok(format!("{name} x{qty} {}", cents_to_yuan(amt)))
        })?;
        let mut v = Vec::new();
        for row in rows {
            v.push(row?);
        }
        v
    };

    let mut vars = std::collections::HashMap::new();
    let store_name = cfg_get(conn, CFG_STORE_NAME)?.unwrap_or_default();
    vars.insert(
        "store_name".into(),
        if store_name.trim().is_empty() {
            "Checkout-R".into()
        } else {
            store_name
        },
    );
    vars.insert(
        "store_phone".into(),
        cfg_get(conn, CFG_STORE_PHONE)?.unwrap_or_default(),
    );
    vars.insert("order_no".into(), order_no.to_string());
    vars.insert("created_at".into(), created);
    vars.insert("member_phone".into(), member_phone);
    vars.insert("table_code".into(), table_code.unwrap_or_default());
    vars.insert("subtotal".into(), cents_to_yuan(total));
    vars.insert("discount_total".into(), cents_to_yuan(discount));
    vars.insert("pay_total".into(), cents_to_yuan(pay));
    vars.insert("pay_channel_lines".into(), channel_lines.join("\n"));
    vars.insert("items".into(), items.join("\n"));
    vars.insert("change".into(), cents_to_yuan(change_total));
    // 二维码承载真实可访问的订单链接；未配置查询页时退化为单号文本，
    // 不再输出 checkout-r:// 这种谁也打不开的伪链接。
    let qr_base = cfg_get(conn, CFG_RECEIPT_QR_BASE)?.unwrap_or_default();
    let qr_content = if qr_base.trim().is_empty() {
        order_no.to_string()
    } else {
        format!("{}/{order_no}", qr_base.trim_end_matches('/'))
    };
    vars.insert("qrcode_pay_url".into(), qr_content);
    let bytes = match render_receipt(&template, &vars) {
        Ok(b) => b,
        Err(e) => {
            enqueue_retry(conn, order_no, "receipt_default", &e.to_string())?;
            return Ok(format!("小票生成失败，已入重打队列: {e}"));
        }
    };
    // 送到网络打印机（IP:9100）；未配置地址直接提示，发送失败才入重打队列。
    // 份数 print_copies（1–5，默认 1）：逐份循环发送，任何一份失败即中止并入队。
    let host = cfg_get(conn, CFG_PRINT_HOST)?.unwrap_or_default();
    if host.trim().is_empty() {
        return Ok("未配置打印机地址，无法打印（请在设置中填写打印机 IP）".into());
    }
    let port = cfg_get(conn, CFG_PRINT_PORT)?
        .and_then(|s| s.trim().parse::<u16>().ok())
        .unwrap_or(DEFAULT_PRINT_PORT);
    let copies: u16 = cfg_get(conn, CFG_PRINT_COPIES)?
        .and_then(|s| s.trim().parse::<u16>().ok())
        .unwrap_or(1)
        .clamp(1, 5);
    for _ in 0..copies {
        if let Err(e) = checkout_core::printer::send_bytes(&host, port, &bytes) {
            enqueue_retry(conn, order_no, "receipt_default", &e.to_string())?;
            return Ok(format!("打印失败，已入重打队列: {e}"));
        }
    }
    Ok(if copies > 1 { format!("小票已打印 ×{copies}") } else { "小票已打印".into() })
}

/// 小票模板：读取当前模板（纸张宽度 + 原始 JSON），供设置页编辑与预览。
#[derive(Debug, Serialize)]
pub struct ReceiptTemplateRow {
    pub paper_width_mm: i32,
    pub content_json: String,
}

#[tauri::command]
pub fn get_receipt_template(state: State<'_, AppState>) -> Result<ReceiptTemplateRow> {
    let (_op, conn) = admin_guard(&state, PERM_SYSTEM_CONFIG)?;
    let (paper_width_mm, content_json): (i32, String) = conn.query_row(
        "SELECT paper_width_mm,content_json FROM print_template WHERE template_id='receipt_default'",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok(ReceiptTemplateRow {
        paper_width_mm,
        content_json,
    })
}

/// 小票模板：校验 JSON 结构后保存（仅店长及以上）。
#[tauri::command]
pub fn save_receipt_template(
    content_json: String,
    paper_width_mm: i32,
    state: State<'_, AppState>,
) -> Result<String> {
    use checkout_core::escpos::Template;
    let (_op, conn) = admin_guard(&state, PERM_SYSTEM_CONFIG)?;
    if !(58..=80).contains(&paper_width_mm) {
        return Err(CoreError::Validation("纸张宽度只支持 58 或 80 毫米".into()));
    }
    // 先解析并展开全部块，确保每个块的 type/字段合法，避免保存后打印才失败
    let sections: serde_json::Value = serde_json::from_str(&content_json)
        .map_err(|e| CoreError::Validation(format!("模板 JSON 格式错误: {e}")))?;
    let template = Template {
        template_id: "receipt_default".into(),
        paper_width_mm,
        sections,
    };
    template
        .iter_blocks()
        .map_err(|e| CoreError::Validation(format!("模板块定义错误: {e}")))?;
    conn.execute(
        "UPDATE print_template SET paper_width_mm=?1,content_json=?2,updated_at=?3
         WHERE template_id='receipt_default'",
        rusqlite::params![paper_width_mm, content_json, now_str()],
    )?;
    Ok("小票模板已保存".into())
}

/// 小票示例变量：预览与测试打印共用同一份数据口径（假单号/示例商品/示例金额）。
fn sample_receipt_vars(conn: &Connection) -> Result<std::collections::HashMap<String, String>> {
    let mut vars = std::collections::HashMap::new();
    let name = cfg_get(conn, CFG_STORE_NAME)?.unwrap_or_default();
    vars.insert(
        "store_name".into(),
        if name.trim().is_empty() {
            "示例门店".into()
        } else {
            name
        },
    );
    vars.insert(
        "store_phone".into(),
        cfg_get(conn, CFG_STORE_PHONE)?.unwrap_or_default(),
    );
    vars.insert("order_no".into(), "STOREPOS0000000000000001".into());
    vars.insert("created_at".into(), now_str());
    vars.insert("member_phone".into(), "138****8888".into());
    vars.insert("table_code".into(), "A01".into());
    vars.insert("subtotal".into(), "12.00".into());
    vars.insert("discount_total".into(), "2.00".into());
    vars.insert("pay_total".into(), "10.00".into());
    vars.insert("pay_channel_lines".into(), "WECHAT 10.00".into());
    vars.insert("items".into(), "示例商品 x1 10.00".into());
    vars.insert("change".into(), "0.00".into());
    vars.insert(
        "qrcode_pay_url".into(),
        "https://example.com/order/STOREPOS0000000000000001".into(),
    );
    Ok(vars)
}

/// 小票模板预览：用示例数据渲染成纯文本，不发送打印机、不入重打队列。
#[tauri::command]
pub fn preview_receipt(content_json: String, state: State<'_, AppState>) -> Result<String> {
    use checkout_core::escpos::{render_vars, Block, Template};
    let (_op, conn) = admin_guard(&state, PERM_SYSTEM_CONFIG)?;
    let sections: serde_json::Value = serde_json::from_str(&content_json)
        .map_err(|e| CoreError::Validation(format!("模板 JSON 格式错误: {e}")))?;
    let paper_width_mm: i32 = conn.query_row(
        "SELECT paper_width_mm FROM print_template WHERE template_id='receipt_default'",
        [],
        |r| r.get(0),
    )?;
    let template = Template {
        template_id: "receipt_default".into(),
        paper_width_mm,
        sections,
    };
    let vars = sample_receipt_vars(&conn)?;
    let chars_per_line = if paper_width_mm >= 80 { 48 } else { 32 };
    let mut lines = Vec::new();
    for block in template.iter_blocks()? {
        match block {
            Block::Text { content, .. } => lines.push(render_vars(&content, &vars)?),
            Block::Divider { char } => {
                let c = char.chars().next().unwrap_or('-');
                lines.push(std::iter::repeat_n(c, chars_per_line).collect());
            }
            Block::Barcode { content, .. } => {
                lines.push(format!("[条码] {}", render_vars(&content, &vars)?))
            }
            Block::Qrcode { content, .. } => {
                lines.push(format!("[二维码] {}", render_vars(&content, &vars)?))
            }
            Block::Feed { lines: n } => {
                for _ in 0..n {
                    lines.push(String::new());
                }
            }
            Block::Cut { .. } | Block::Kick { .. } => {}
        }
    }
    Ok(lines.join("\n"))
}

/// 打印测试页：用小票模板示例数据渲染真实字节发送到打印机，不入重打队列。
/// 与预览同一份示例变量；地址未配置直接报错，发送失败原样抛出——测试就是要看到失败原因。
#[tauri::command]
pub fn test_print(state: State<'_, AppState>) -> Result<String> {
    use checkout_core::escpos::{render_receipt, Template};
    let (_op, conn) = admin_guard(&state, PERM_SYSTEM_CONFIG)?;
    let (paper_width_mm, content_json): (i32, String) = conn.query_row(
        "SELECT paper_width_mm,content_json FROM print_template WHERE template_id='receipt_default'",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let sections: serde_json::Value = serde_json::from_str(&content_json)
        .map_err(|e| CoreError::Validation(format!("模板 JSON 格式错误: {e}")))?;
    let template = Template {
        template_id: "receipt_default".into(),
        paper_width_mm,
        sections,
    };
    template
        .iter_blocks()
        .map_err(|e| CoreError::Validation(format!("模板块定义错误: {e}")))?;
    let vars = sample_receipt_vars(&conn)?;
    let bytes = render_receipt(&template, &vars)?;
    let host = cfg_get(&conn, CFG_PRINT_HOST)?.unwrap_or_default();
    if host.trim().is_empty() {
        return Err(CoreError::Validation("未配置打印机地址，请先填写打印机 IP".into()));
    }
    let port = cfg_get(&conn, CFG_PRINT_PORT)?
        .and_then(|s| s.trim().parse::<u16>().ok())
        .unwrap_or(DEFAULT_PRINT_PORT);
    checkout_core::printer::send_bytes(&host, port, &bytes)?;
    Ok(format!("测试页已发送到 {host}:{port}"))
}

/// 交班汇总。
#[tauri::command]
pub fn shift_report(
    shift_id: i64,
    state: State<'_, AppState>,
) -> Result<Vec<checkout_core::order::ShiftSummary>> {
    let (_op, conn) = admin_guard(&state, PERM_POS_SHIFT_CLOSE)?;
    checkout_core::order::shift_summary(&conn, shift_id)
}

/// 商品档案创建（F-11/F-12）：SPU/SKU 分离，需 product.edit 权限。
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn create_product(
    name: String,
    barcode: Option<String>,
    instore_code: Option<String>,
    category_id: i64,
    unit: String,
    sale_price_cents: i64,
    cost_price_cents: i64,
    member_price_cents: Option<i64>,
    is_weighed: bool,
    is_service: bool,
    initial_qty: i64,
    alert_qty: i64,
    state: State<'_, AppState>,
) -> Result<i64> {
    let (operator_id, conn) = admin_guard(&state, PERM_PRODUCT_EDIT)?;
    checkout_core::product::create_product_full(
        &conn,
        category_id,
        &name,
        barcode.as_deref(),
        instore_code.as_deref(),
        &unit,
        sale_price_cents,
        cost_price_cents,
        member_price_cents,
        is_weighed,
        is_service,
        initial_qty,
        alert_qty,
        Some(operator_id),
    )
}

#[tauri::command]
pub fn list_products(
    keyword: String,
    limit: i64,
    include_disabled: Option<bool>,
    state: State<'_, AppState>,
) -> Result<Vec<checkout_core::product::ProductRow>> {
    let (_operator_id, conn) = admin_guard(&state, PERM_PRODUCT_EDIT)?;
    let mut rows = checkout_core::product::list_products(
        &conn,
        &keyword,
        limit,
        include_disabled.unwrap_or(false),
    )?;
    // 本地图片：库内是文件名，这里解析成绝对路径供前端展示
    let img_dir = state.data_dir.join("images");
    for r in &mut rows {
        r.image_path = r
            .image_path
            .as_ref()
            .map(|f| img_dir.join(f).to_string_lossy().to_string());
    }
    Ok(rows)
}

#[tauri::command]
pub fn update_product(
    sku_id: i64,
    sale_price_cents: i64,
    alert_qty: i64,
    member_price_cents: Option<i64>,
    instore_code: Option<String>,
    state: State<'_, AppState>,
) -> Result<()> {
    let (_operator_id, conn) = admin_guard(&state, PERM_PRODUCT_EDIT)?;
    checkout_core::product::update_product_full(
        &conn,
        sku_id,
        sale_price_cents,
        alert_qty,
        member_price_cents,
        instore_code.as_deref(),
    )
}

/// 商品档案全量编辑：与新增表单同字段范围，供设置/商品页「编辑」复用新增的完整表单。
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn update_product_profile(
    sku_id: i64,
    category_id: i64,
    name: String,
    barcode: Option<String>,
    instore_code: Option<String>,
    unit: String,
    sale_price_cents: i64,
    cost_price_cents: i64,
    member_price_cents: Option<i64>,
    is_weighed: bool,
    alert_qty: i64,
    state: State<'_, AppState>,
) -> Result<()> {
    let (_operator_id, conn) = admin_guard(&state, PERM_PRODUCT_EDIT)?;
    checkout_core::product::update_product_profile(
        &conn,
        sku_id,
        category_id,
        &name,
        barcode.as_deref(),
        instore_code.as_deref(),
        &unit,
        sale_price_cents,
        cost_price_cents,
        member_price_cents,
        is_weighed,
        alert_qty,
    )
}

#[tauri::command]
pub fn set_product_service(
    sku_id: i64,
    is_service: bool,
    state: State<'_, AppState>,
) -> Result<()> {
    let (_operator_id, conn) = admin_guard(&state, PERM_PRODUCT_EDIT)?;
    checkout_core::product::set_product_service(&conn, sku_id, is_service)
}

#[tauri::command]
pub fn set_product_status(spu_id: i64, status: i64, state: State<'_, AppState>) -> Result<()> {
    let (_operator_id, conn) = admin_guard(&state, PERM_PRODUCT_EDIT)?;
    checkout_core::product::set_product_status(&conn, spu_id, status)
}

#[tauri::command]
pub fn list_categories(
    state: State<'_, AppState>,
) -> Result<Vec<checkout_core::product::Category>> {
    let (_operator_id, conn) = admin_guard(&state, PERM_PRODUCT_EDIT)?;
    checkout_core::product::list_categories(&conn)
}

#[tauri::command]
pub fn create_category(
    parent_id: Option<i64>,
    name: String,
    state: State<'_, AppState>,
) -> Result<i64> {
    let (_operator_id, conn) = admin_guard(&state, PERM_PRODUCT_EDIT)?;
    checkout_core::product::create_category(&conn, parent_id, &name)
}

#[tauri::command]
pub fn update_category(
    category_id: i64,
    parent_id: Option<i64>,
    name: String,
    state: State<'_, AppState>,
) -> Result<()> {
    let (_operator_id, conn) = admin_guard(&state, PERM_PRODUCT_EDIT)?;
    checkout_core::product::update_category(&conn, category_id, parent_id, &name)
}

#[tauri::command]
pub fn delete_category(category_id: i64, state: State<'_, AppState>) -> Result<()> {
    let (_operator_id, conn) = admin_guard(&state, PERM_PRODUCT_EDIT)?;
    checkout_core::product::delete_category(&conn, category_id)
}

/// 库存调整（inventory.adjust，F-13/F-14）：带符号差值，原因入 inv_trans 审计。
#[tauri::command]
pub fn adjust_stock(
    sku_id: i64,
    change_qty: i64,
    reason: String,
    state: State<'_, AppState>,
) -> Result<i64> {
    let (operator_id, conn) = admin_guard(&state, PERM_INVENTORY_ADJUST)?;
    if change_qty == 0 {
        return Err(CoreError::Validation("调整数量不能为 0".into()));
    }
    if reason.trim().is_empty() {
        return Err(CoreError::Validation("库存调整必须填写原因".into()));
    }
    let tx = conn.unchecked_transaction()?;
    let new_qty =
        checkout_core::product::adjust_stock(&tx, sku_id, change_qty, &reason, Some(operator_id))?;
    // 调整原因必须留审计：inv_trans 只存单据号，原因不进 ref_order_no。
    checkout_core::rbac::audit_operation(
        &tx,
        "adjust_stock",
        Some(operator_id),
        None,
        serde_json::json!({"sku_id": sku_id, "change_qty": change_qty, "reason": reason}),
        &now_str(),
    )?;
    tx.commit()?;
    Ok(new_qty)
}

// ===== 采购入库与盘点（§10）=====

#[derive(Debug, Deserialize)]
pub struct PurchaseItemInput {
    pub sku_id: i64,
    pub ordered_qty: i64,
    pub unit_cost_cents: i64,
}

#[derive(Debug, Deserialize)]
pub struct ReceiveItemInput {
    pub item_id: i64,
    pub received_qty: i64,
}

#[derive(Debug, Serialize)]
pub struct PurchaseOrderRow {
    pub po_no: String,
    pub supplier_name: String,
    pub status: String,
    pub total_cents: i64,
    pub created_at: String,
    pub item_count: i64,
}

#[derive(Debug, Serialize)]
pub struct PurchaseItemRow {
    pub item_id: i64,
    pub sku_id: i64,
    pub product_name: String,
    pub ordered_qty: i64,
    pub received_qty: Option<i64>,
    pub unit_cost_cents: i64,
}

#[derive(Debug, Serialize)]
pub struct SupplierRow {
    pub supplier_id: i64,
    pub name: String,
    pub contact: Option<String>,
    pub phone: Option<String>,
}

fn gen_no(prefix: &str) -> String {
    format!("{}{}", prefix, chrono::Utc::now().timestamp_millis())
}

#[tauri::command]
pub fn create_supplier(
    name: String,
    contact: Option<String>,
    phone: Option<String>,
    state: State<'_, AppState>,
) -> Result<i64> {
    let (_operator_id, conn) = admin_guard(&state, "purchase.create")?;
    if name.trim().is_empty() {
        return Err(CoreError::Validation("供应商名不能为空".into()));
    }
    conn.execute(
        "INSERT INTO supplier(name,contact,phone) VALUES (?1,?2,?3)",
        rusqlite::params![name.trim(), contact, phone],
    )?;
    Ok(conn.last_insert_rowid())
}

#[tauri::command]
pub fn list_suppliers(state: State<'_, AppState>) -> Result<Vec<SupplierRow>> {
    let (_operator_id, conn) = admin_guard(&state, "purchase.create")?;
    let mut stmt = conn.prepare(
        "SELECT supplier_id, name, contact, phone FROM supplier WHERE status=1 ORDER BY supplier_id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(SupplierRow {
            supplier_id: r.get(0)?,
            name: r.get(1)?,
            contact: r.get(2)?,
            phone: r.get(3)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// 创建采购单（DRAFT）：验收前不影响库存（§10.2）。
#[tauri::command]
pub fn create_purchase_order(
    supplier_id: i64,
    items: Vec<PurchaseItemInput>,
    state: State<'_, AppState>,
) -> Result<String> {
    let (_operator_id, conn) = admin_guard(&state, "purchase.create")?;
    if items.is_empty() {
        return Err(CoreError::Validation("采购单明细不能为空".into()));
    }
    let po_no = gen_no("PO");
    let rows: Vec<(i64, i64, i64)> = items
        .into_iter()
        .map(|i| (i.sku_id, i.ordered_qty, i.unit_cost_cents))
        .collect();
    checkout_core::purchase::create_purchase_order(&conn, &po_no, supplier_id, &rows)?;
    Ok(po_no)
}

#[tauri::command]
pub fn list_purchase_orders(state: State<'_, AppState>) -> Result<Vec<PurchaseOrderRow>> {
    let (_operator_id, conn) = admin_guard(&state, "purchase.create")?;
    let mut stmt = conn.prepare(
        "SELECT o.po_no, COALESCE(s.name,''), o.status, o.total_cents, o.created_at,
                (SELECT COUNT(*) FROM purchase_order_item i WHERE i.po_no=o.po_no)
         FROM purchase_order o LEFT JOIN supplier s ON s.supplier_id=o.supplier_id
         ORDER BY o.created_at DESC LIMIT 100",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(PurchaseOrderRow {
            po_no: r.get(0)?,
            supplier_name: r.get(1)?,
            status: r.get(2)?,
            total_cents: r.get(3)?,
            created_at: r.get(4)?,
            item_count: r.get(5)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// 采购单明细（含已收数量），用于验收录入。
#[tauri::command]
pub fn get_purchase_items(
    po_no: String,
    state: State<'_, AppState>,
) -> Result<Vec<PurchaseItemRow>> {
    let (_operator_id, conn) = admin_guard(&state, "purchase.create")?;
    let mut stmt = conn.prepare(
        "SELECT i.item_id, i.sku_id, COALESCE(p.spu_name,''), i.ordered_qty, i.received_qty, i.unit_cost_cents
         FROM purchase_order_item i
         JOIN sku s ON s.sku_id=i.sku_id
         JOIN spu p ON p.spu_id=s.spu_id
         WHERE i.po_no=?1 ORDER BY i.item_id",
    )?;
    let rows = stmt.query_map([&po_no], |r| {
        Ok(PurchaseItemRow {
            item_id: r.get(0)?,
            sku_id: r.get(1)?,
            product_name: r.get(2)?,
            ordered_qty: r.get(3)?,
            received_qty: r.get(4)?,
            unit_cost_cents: r.get(5)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// 验收入库：逐行录实收，差异留痕；通过后产生 PURCHASE_IN 事件。
#[tauri::command]
pub fn receive_purchase(
    po_no: String,
    received: Vec<ReceiveItemInput>,
    state: State<'_, AppState>,
) -> Result<()> {
    let (operator_id, conn) = admin_guard(&state, "purchase.create")?;
    let rows: Vec<(i64, i64)> = received
        .into_iter()
        .map(|r| (r.item_id, r.received_qty))
        .collect();
    checkout_core::purchase::receive_purchase(&conn, &po_no, &rows, operator_id)
}

#[derive(Debug, Serialize)]
pub struct StocktakeRow {
    pub task_no: String,
    pub status: String,
    pub created_at: String,
    pub item_count: i64,
}

#[derive(Debug, Serialize)]
pub struct StocktakeItemRow {
    pub item_id: i64,
    pub sku_id: i64,
    pub product_name: String,
    pub book_qty: i64,
    pub counted_qty: Option<i64>,
    pub diff_qty: Option<i64>,
}

#[tauri::command]
pub fn list_stocktakes(state: State<'_, AppState>) -> Result<Vec<StocktakeRow>> {
    let (_operator_id, conn) = admin_guard(&state, "purchase.create")?;
    let mut stmt = conn.prepare(
        "SELECT t.task_no, t.status, t.created_at,
                (SELECT COUNT(*) FROM stocktake_item i WHERE i.stocktake_id=t.stocktake_id)
         FROM stocktake t ORDER BY t.created_at DESC LIMIT 100",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(StocktakeRow {
            task_no: r.get(0)?,
            status: r.get(1)?,
            created_at: r.get(2)?,
            item_count: r.get(3)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// 盘点任务明细（账面/实盘/差异）。
#[tauri::command]
pub fn get_stocktake_items(
    task_no: String,
    state: State<'_, AppState>,
) -> Result<Vec<StocktakeItemRow>> {
    let (_operator_id, conn) = admin_guard(&state, "purchase.create")?;
    let mut stmt = conn.prepare(
        "SELECT i.id, i.sku_id, COALESCE(p.spu_name,''), i.book_qty,
                i.counted_qty, i.diff_qty
         FROM stocktake_item i
         JOIN sku s ON s.sku_id=i.sku_id
         JOIN spu p ON p.spu_id=s.spu_id
         JOIN stocktake t ON t.stocktake_id=i.stocktake_id
         WHERE t.task_no=?1 ORDER BY i.id",
    )?;
    let rows = stmt.query_map([&task_no], |r| {
        Ok(StocktakeItemRow {
            item_id: r.get(0)?,
            sku_id: r.get(1)?,
            product_name: r.get(2)?,
            book_qty: r.get(3)?,
            counted_qty: r.get(4)?,
            diff_qty: r.get(5)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// 创建盘点任务：写入账面快照。
#[tauri::command]
pub fn create_stocktake(sku_ids: Vec<i64>, state: State<'_, AppState>) -> Result<String> {
    let (operator_id, conn) = admin_guard(&state, "purchase.create")?;
    if sku_ids.is_empty() {
        return Err(CoreError::Validation("盘点商品不能为空".into()));
    }
    let task_no = gen_no("ST");
    checkout_core::purchase::create_stocktake(&conn, &task_no, &sku_ids, operator_id)?;
    Ok(task_no)
}

/// 实盘录入（盘点差异 → DIFF_REVIEW 待审批）。
#[tauri::command]
pub fn record_count(
    task_no: String,
    counts: Vec<ReceiveItemInput>,
    state: State<'_, AppState>,
) -> Result<()> {
    let (_operator_id, conn) = admin_guard(&state, "purchase.create")?;
    let rows: Vec<(i64, i64)> = counts
        .into_iter()
        .map(|c| (c.item_id, c.received_qty))
        .collect();
    checkout_core::purchase::record_count(&conn, &task_no, &rows)
}

/// 盘点审批（stocktake.approve）：差异产生 ADJUST 事件，不覆盖库存。
#[tauri::command]
pub fn approve_stocktake(task_no: String, state: State<'_, AppState>) -> Result<()> {
    let (approver_id, conn) = admin_guard(&state, PERM_STOCKTAKE_APPROVE)?;
    checkout_core::purchase::approve_stocktake(&conn, &task_no, approver_id)
}

// ===== 会员管理（§13 四账）=====

#[derive(Debug, Serialize)]
pub struct MemberRow {
    pub member_id: i64,
    pub phone: Option<String>,
    pub member_code: String,
    pub name: Option<String>,
    pub status: i64,
    pub principal_cents: i64,
    pub bonus_cents: i64,
    pub points: i64,
    pub growth: i64,
}

const MEMBER_ASSET_SQL: &str = "COALESCE((SELECT balance FROM member_asset a WHERE a.member_id=m.member_id AND a.asset_type='PRINCIPAL'),0)";

#[tauri::command]
pub fn create_member(
    phone: Option<String>,
    name: Option<String>,
    state: State<'_, AppState>,
) -> Result<i64> {
    let (_operator_id, conn) = admin_guard(&state, PERM_MEMBER_MANAGE)?;
    if let Some(p) = &phone {
        if p.trim().is_empty() {
            return Err(CoreError::Validation("手机号不能为空".into()));
        }
    }
    let code = format!("M{}", new_nonce().replace('-', ""));
    conn.execute(
        "INSERT INTO member(phone,member_code,name) VALUES (?1,?2,?3)",
        rusqlite::params![phone, code, name],
    )?;
    Ok(conn.last_insert_rowid())
}

#[tauri::command]
pub fn list_members(
    keyword: String,
    limit: i64,
    state: State<'_, AppState>,
) -> Result<Vec<MemberRow>> {
    let (_operator_id, conn) = admin_guard(&state, PERM_MEMBER_MANAGE)?;
    let pattern = format!("%{keyword}%");
    let mut stmt = conn.prepare(&format!(
        "SELECT m.member_id, m.phone, m.member_code, m.name, m.status,
                {MEMBER_ASSET_SQL},
                COALESCE((SELECT balance FROM member_asset a WHERE a.member_id=m.member_id AND a.asset_type='BONUS'),0),
                COALESCE((SELECT balance FROM member_asset a WHERE a.member_id=m.member_id AND a.asset_type='POINTS'),0),
                COALESCE((SELECT balance FROM member_asset a WHERE a.member_id=m.member_id AND a.asset_type='GROWTH'),0)
         FROM member m
         WHERE (?1 = '' OR m.phone LIKE ?2 OR m.name LIKE ?2 OR m.member_code LIKE ?2)
         ORDER BY m.member_id DESC LIMIT ?3"
    ))?;
    let rows = stmt.query_map(rusqlite::params![keyword, pattern, limit], |r| {
        Ok(MemberRow {
            member_id: r.get(0)?,
            phone: r.get(1)?,
            member_code: r.get(2)?,
            name: r.get(3)?,
            status: r.get(4)?,
            principal_cents: r.get(5)?,
            bonus_cents: r.get(6)?,
            points: r.get(7)?,
            growth: r.get(8)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// 会员充值（§13.3）：本金 += 实充，赠送 += 赠送额；幂等流水，操作人入账。
/// 充值是一笔收款：必须落 payment_record（现金/扫码）才能进交班与对账。
/// `request_id` 由客户端生成（双击/重试复用同一值 → 幂等，不重复充值）。
#[tauri::command]
pub fn recharge_member(
    member_id: i64,
    principal_cents: i64,
    bonus_cents: i64,
    pay_channel: Option<String>,
    request_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<()> {
    let (operator_id, conn) = admin_guard(&state, PERM_MEMBER_RECHARGE)?;
    if principal_cents <= 0 || bonus_cents < 0 {
        return Err(CoreError::Validation("充值金额非法".into()));
    }
    let channel = pay_channel
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .unwrap_or(PAY_CASH);
    // 扫码充值尚未接入真实渠道收款流程：允许它等于"不扫顾客的码就把钱记成已收"。
    // 先只放行现金；接入真实 Native/付款码充值后再放开对应渠道。
    if channel != PAY_CASH {
        return Err(CoreError::Validation(
            "扫码充值暂未接入渠道收款，请使用现金充值".into(),
        ));
    }
    let now = now_str();
    // 幂等指纹：同一 request_id 但金额/会员/渠道不同，说明是客户端复用错了 ID，
    // 必须报冲突，不能直接返回成功把前一笔充值冒充成本次结果。
    let fingerprint = format!("{member_id}:{principal_cents}:{bonus_cents}:{channel}");
    let recharge_no = match request_id
        .as_deref()
        .map(str::trim)
        .filter(|r| !r.is_empty())
    {
        Some(rid) => {
            // 幂等：同一 request_id 已入账则直接成功返回，绝不二次充值。
            let existing: Option<(i64, String, Option<String>)> = conn
                .query_row(
                    "SELECT amount_cents, pay_channel, request_fingerprint
                     FROM payment_record WHERE out_trade_no=?1",
                    [rid],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?;
            if let Some((amount, ch, fp)) = existing {
                let same = match fp.as_deref().filter(|s| !s.is_empty()) {
                    Some(stored) => stored == fingerprint,
                    // 老库无指纹：退化为金额+渠道比对
                    None => amount == principal_cents && ch == channel,
                };
                if !same {
                    return Err(CoreError::Validation(
                        "request_id 已被另一笔充值使用，请刷新后重试".into(),
                    ));
                }
                return Ok(());
            }
            format!("RC{rid}")
        }
        None => gen_no("RC"),
    };
    let out_trade_no = request_id
        .as_deref()
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .map(|r| r.to_string())
        .unwrap_or_else(|| format!("{recharge_no}P1"));
    // 入账、收款流水、审计同事务：钱、账、审计三者不拆开。
    let tx = conn.unchecked_transaction()?;
    checkout_core::member::recharge(&tx, member_id, &recharge_no, principal_cents, bonus_cents)?;
    if let Err(e) = tx.execute(
        "INSERT INTO payment_record(order_no,pay_channel,pay_mode,attempt_no,amount_cents,out_trade_no,status,paid_at,created_at,request_fingerprint)
         VALUES (?1,?2,'',1,?3,?4,'SUCCESS',?5,?5,?6)",
        rusqlite::params![recharge_no, channel, principal_cents, out_trade_no, now, fingerprint],
    ) {
        if e.to_string().contains("UNIQUE") {
            // 并发双击：另一请求已用同一 request_id 入账，本事务回滚（资产流水幂等键亦防重）
            return Ok(());
        }
        return Err(e.into());
    }
    checkout_core::rbac::audit_operation(
        &tx,
        "recharge_member",
        Some(operator_id),
        None,
        serde_json::json!({
            "member_id": member_id,
            "recharge_no": recharge_no,
            "principal_cents": principal_cents,
            "bonus_cents": bonus_cents,
            "pay_channel": channel
        }),
        &now,
    )?;
    tx.commit()?;
    Ok(())
}

// ===== 促销规则管理（§8）=====

#[derive(Debug, Serialize)]
pub struct PromotionRow {
    pub promotion_id: i64,
    pub name: String,
    pub scope: String,
    pub priority: i32,
    pub exclusive: bool,
    pub stackable: bool,
    pub min_qty: i64,
    pub min_amount_cents: i64,
    pub discount_rate: Option<f64>,
    pub discount_cents: Option<i64>,
    pub target_skus: Vec<i64>,
    pub valid_from: Option<String>,
    pub valid_until: Option<String>,
    pub status: i64,
}

fn row_to_promotion_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<PromotionRow> {
    let condition: String = r.get(8)?;
    let action: String = r.get(9)?;
    let cond: serde_json::Value = serde_json::from_str(&condition).unwrap_or(serde_json::json!({}));
    let act: serde_json::Value = serde_json::from_str(&action).unwrap_or(serde_json::json!({}));
    Ok(PromotionRow {
        promotion_id: r.get(0)?,
        name: r.get(1)?,
        scope: r.get(2)?,
        priority: r.get(3)?,
        exclusive: r.get::<_, i64>(4)? != 0,
        stackable: r.get::<_, i64>(5)? != 0,
        min_qty: cond.get("min_qty").and_then(|v| v.as_i64()).unwrap_or(0),
        min_amount_cents: cond
            .get("min_amount_cents")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
        target_skus: cond
            .get("target_skus")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_i64()).collect())
            .unwrap_or_default(),
        discount_rate: act.get("discount_rate").and_then(|v| v.as_f64()),
        discount_cents: act.get("discount_cents").and_then(|v| v.as_i64()),
        valid_from: r.get(10)?,
        valid_until: r.get(11)?,
        status: r.get(12)?,
    })
}

#[tauri::command]
pub fn list_promotions(state: State<'_, AppState>) -> Result<Vec<PromotionRow>> {
    let (_operator_id, conn) = admin_guard(&state, PERM_REPORT_VIEW)?;
    let mut stmt = conn.prepare(
        "SELECT promotion_id, name, scope, priority, exclusive, stackable,
                NULL, NULL, condition_json, action_json, valid_from, valid_until, status
         FROM promotion ORDER BY priority, promotion_id",
    )?;
    let rows = stmt.query_map([], row_to_promotion_row)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// 新建促销规则（§8）：折扣率与满减二选一；目标 SKU 空数组 = 全部商品。
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn create_promotion(
    name: String,
    scope: String,
    priority: i32,
    exclusive: bool,
    stackable: bool,
    min_qty: i64,
    min_amount_cents: i64,
    discount_rate: Option<f64>,
    discount_cents: Option<i64>,
    target_skus: Vec<i64>,
    valid_from: Option<String>,
    valid_until: Option<String>,
    state: State<'_, AppState>,
) -> Result<i64> {
    let (_operator_id, conn) = admin_guard(&state, PERM_PRODUCT_EDIT)?;
    if name.trim().is_empty() {
        return Err(CoreError::Validation("促销名称不能为空".into()));
    }
    if scope != "SINGLE_ITEM" && scope != "ORDER_LEVEL" {
        return Err(CoreError::Validation(
            "scope 仅允许 SINGLE_ITEM / ORDER_LEVEL".into(),
        ));
    }
    if discount_rate.is_none() == discount_cents.is_none() {
        return Err(CoreError::Validation("折扣率与满减额必须二选一".into()));
    }
    let promo_type = if discount_rate.is_some() {
        "DISCOUNT"
    } else {
        "FULL_CUT"
    };
    let condition = serde_json::json!({
        "min_qty": min_qty,
        "min_amount_cents": min_amount_cents,
        "target_skus": target_skus,
    });
    let action = serde_json::json!({
        "discount_rate": discount_rate,
        "discount_cents": discount_cents,
    });
    conn.execute(
        "INSERT INTO promotion(name,promo_type,scope,priority,exclusive,stackable,condition_json,action_json,valid_from,valid_until)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        rusqlite::params![
            name.trim(),
            promo_type,
            scope,
            priority,
            exclusive as i64,
            stackable as i64,
            condition.to_string(),
            action.to_string(),
            valid_from,
            valid_until
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

#[tauri::command]
pub fn set_promotion_status(
    promotion_id: i64,
    status: i64,
    state: State<'_, AppState>,
) -> Result<()> {
    let (_operator_id, conn) = admin_guard(&state, PERM_PRODUCT_EDIT)?;
    if status != 0 && status != 1 {
        return Err(CoreError::Validation("status 仅允许 0/1".into()));
    }
    let n = conn.execute(
        "UPDATE promotion SET status=?1 WHERE promotion_id=?2",
        rusqlite::params![status, promotion_id],
    )?;
    if n == 0 {
        return Err(CoreError::Validation(format!("促销 {promotion_id} 不存在")));
    }
    Ok(())
}

// ===== 订单作废 / 退货（§5.5 / §5.6）=====

#[derive(Debug, Deserialize)]
pub struct ReturnItemInput {
    pub order_item_id: i64,
    pub qty: i64,
}

/// 退货明细按原单行优惠还原应退金额（§8.6）。
/// 退款试算与实际退款共用此函数，保证「界面显示的应退金额」与「实际退款金额」同口径。
/// order_no 参与行归属校验：明细必须属于该订单，避免跨单串行。
fn calc_refund_amounts(
    conn: &Connection,
    order_no: &str,
    items: &[ReturnItemInput],
) -> Result<(Vec<i64>, i64)> {
    let mut refunds: Vec<i64> = Vec::new();
    for it in items {
        let (sku_id, qty, returned, line_total, discount): (i64, i64, i64, i64, i64) = conn
            .query_row(
                "SELECT sku_id,qty,returned_qty,line_total_cents,discount_cents
                 FROM sales_order_item WHERE item_id=?1 AND order_no=?2",
                rusqlite::params![it.order_item_id, order_no],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .map_err(|_| {
                CoreError::Validation(format!(
                    "退货明细 {} 不属于订单 {order_no}",
                    it.order_item_id
                ))
            })?;
        let line = checkout_core::promo::CartLine {
            sku_id,
            qty,
            unit_price_cents: if qty > 0 { line_total / qty } else { 0 },
            line_total_cents: line_total,
            discount_cents: discount,
            member_price_cents: None,
            technician_id: None,
            is_gift: false,
            applied_promos: Vec::new(),
        };
        refunds.push(checkout_core::promo::refund_for_line(
            &line, it.qty, returned,
        )?);
    }
    let total: i64 = refunds.iter().sum();
    Ok((refunds, total))
}

/// 退款金额试算：只算不退，供退款页在二次授权前给出确切金额。
#[tauri::command]
pub fn preview_refund(
    order_no: String,
    items: Vec<ReturnItemInput>,
    state: State<'_, AppState>,
) -> Result<i64> {
    let license = state
        .license
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    license.gate(ENT_POS_CORE)?;
    let session = state
        .session
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    let session = session.as_ref().ok_or(CoreError::NotAuthenticated)?;
    let now_dt = parse_rfc3339(&now_str())?;
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    checkout_core::rbac::guard(
        &conn,
        &checkout_core::rbac::GuardRequest {
            entitlement: ENT_POS_CORE,
            permission: PERM_POS_REFUND,
            session,
            license: &license,
            sensitive_auth: None,
            current_nonce: "",
            now: now_dt,
            amount_cents: 0,
            limit_key: None,
        },
    )?;
    let _ = session;
    drop(license);
    if items.is_empty() {
        return Err(CoreError::Validation("请先选择退货明细".into()));
    }
    // 预演与正式退货同口径：非已支付完成订单不允许报价
    let status: String = conn.query_row(
        "SELECT status FROM sales_order WHERE order_no=?1",
        [order_no.trim()],
        |r| r.get(0),
    )?;
    if !matches!(status.as_str(), "COMPLETED" | "PARTIAL_REFUND") {
        return Err(CoreError::Validation(format!("订单状态 {status} 不可退货")));
    }
    let (_, total) = calc_refund_amounts(&conn, order_no.trim(), &items)?;
    Ok(total)
}

/// 订单作废（pos.void）：仅待支付单或当日现金单；扫码已支付单只能退款。
/// 需二次授权（批准后取得一次性 nonce，作废消费即失效），并写操作审计。
#[tauri::command]
pub fn void_order_cmd(
    order_no: String,
    sensitive_nonce: Option<String>,
    state: State<'_, AppState>,
) -> Result<()> {
    let now_dt = parse_rfc3339(&now_str())?;
    let auth = consume_sensitive(&state, PERM_POS_VOID, sensitive_nonce, now_dt)?;
    let license = state
        .license
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    let session = state
        .session
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    let session = session.as_ref().ok_or(CoreError::NotAuthenticated)?;
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    checkout_core::rbac::guard(
        &conn,
        &checkout_core::rbac::GuardRequest {
            entitlement: ENT_POS_CORE,
            permission: PERM_POS_VOID,
            session,
            license: &license,
            sensitive_auth: Some(&auth),
            current_nonce: &auth.nonce,
            now: now_dt,
            amount_cents: 0,
            limit_key: None,
        },
    )?;
    let operator_id = session.user_id;
    let approver_id = auth.approver_id;
    let _ = session;
    drop(license);
    {
        let tx = conn.unchecked_transaction()?;
        checkout_core::order::void_order(&tx, &order_no)?;
        // 待支付单作废：释放锁定券、关闭未完成支付流水，避免券卡死与流水悬空。
        release_order_coupon(&tx, &order_no)?;
        tx.execute(
            "UPDATE payment_record SET status='CLOSED' WHERE order_no=?1 AND status IN ('PENDING','USERPAYING')",
            [&order_no],
        )?;
        checkout_core::rbac::audit_operation(
            &tx,
            "void_order",
            Some(operator_id),
            Some(approver_id),
            serde_json::json!({"order_no": order_no}),
            &now_str(),
        )?;
        tx.commit()?;
    }
    Ok(())
}

/// 订单可退行明细（退货录入）：带原价优惠，供按优惠还原退款额。
#[derive(Debug, Serialize)]
pub struct OrderItemRow {
    pub item_id: i64,
    pub sku_id: i64,
    pub product_name: String,
    pub qty: i64,
    pub returned_qty: i64,
    pub unit_price_cents: i64,
    pub line_total_cents: i64,
    pub discount_cents: i64,
}

#[tauri::command]
pub fn get_order_items(order_no: String, state: State<'_, AppState>) -> Result<Vec<OrderItemRow>> {
    let (_operator_id, conn) = admin_guard(&state, PERM_POS_REFUND)?;
    let mut stmt = conn.prepare(
        "SELECT i.item_id, i.sku_id, COALESCE(p.spu_name,''), i.qty, i.returned_qty,
                i.unit_price_cents, i.line_total_cents, i.discount_cents
         FROM sales_order_item i
         JOIN sku s ON s.sku_id=i.sku_id
         JOIN spu p ON p.spu_id=s.spu_id
         WHERE i.order_no=?1 ORDER BY i.item_id",
    )?;
    let rows = stmt.query_map([&order_no], |r| {
        Ok(OrderItemRow {
            item_id: r.get(0)?,
            sku_id: r.get(1)?,
            product_name: r.get(2)?,
            qty: r.get(3)?,
            returned_qty: r.get(4)?,
            unit_price_cents: r.get(5)?,
            line_total_cents: r.get(6)?,
            discount_cents: r.get(7)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// 退货（pos.refund + refund.max_cents 限额）：按原单行优惠还原退款额（§8.6），
/// 渠道占比分摊写 refund_record，储值按镜像回补，积分扣回，二次授权审计。
#[tauri::command]
pub fn refund_order(
    order_no: String,
    items: Vec<ReturnItemInput>,
    sensitive_nonce: Option<String>,
    state: State<'_, AppState>,
) -> Result<i64> {
    let now_dt = parse_rfc3339(&now_str())?;
    let auth = consume_sensitive(&state, PERM_POS_REFUND, sensitive_nonce, now_dt)?;
    let license = state
        .license
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    let session = state
        .session
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    let session = session.as_ref().ok_or(CoreError::NotAuthenticated)?;
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;

    if items.is_empty() {
        return Err(CoreError::Validation("退货明细不能为空".into()));
    }

    // 按原单行优惠还原每行退款额（§8.6），并累加限额校验金额
    let (refunds, total_refund) = calc_refund_amounts(&conn, order_no.trim(), &items)?;
    let mut rows: Vec<(i64, i64, i64)> = Vec::new();
    for it in &items {
        let sku_id: i64 = conn.query_row(
            "SELECT sku_id FROM sales_order_item WHERE item_id=?1",
            [it.order_item_id],
            |r| r.get(0),
        )?;
        rows.push((it.order_item_id, sku_id, it.qty));
    }

    checkout_core::rbac::guard(
        &conn,
        &checkout_core::rbac::GuardRequest {
            entitlement: ENT_POS_CORE,
            permission: PERM_POS_REFUND,
            session,
            license: &license,
            sensitive_auth: Some(&auth),
            current_nonce: &auth.nonce,
            now: now_dt,
            amount_cents: total_refund,
            limit_key: Some(LIMIT_REFUND_MAX_CENTS),
        },
    )?;
    let operator_id = session.user_id;
    let approver_id = auth.approver_id;
    let _ = session;
    drop(license);

    let member_id: Option<i64> = conn
        .query_row(
            "SELECT member_id FROM sales_order WHERE order_no=?1",
            [&order_no],
            |r| r.get(0),
        )
        .optional()?
        .flatten();

    // 渠道占比原始份额 + 原单储值扣减构成（事务外读，领域函数自带事务不可嵌套）
    let shares: Vec<(PayChannel, i64)> = {
        let mut stmt = conn.prepare(
            "SELECT pay_channel, amount_cents FROM payment_record
             WHERE order_no=?1 AND status='SUCCESS' ORDER BY pay_id",
        )?;
        let rs = stmt.query_map([&order_no], |r| {
            let ch: String = r.get(0)?;
            let amt: i64 = r.get(1)?;
            let channel = match ch.as_str() {
                PAY_CASH => PayChannel::CASH,
                PAY_STORED_VALUE => PayChannel::STORED_VALUE,
                PAY_WECHAT => PayChannel::WECHAT,
                PAY_ALIPAY => PayChannel::ALIPAY,
                _ => PayChannel::CASH,
            };
            Ok((channel, amt))
        })?;
        let mut v = Vec::new();
        for row in rs {
            v.push(row?);
        }
        v
    };
    let (orig_bonus, orig_principal): (i64, i64) = conn.query_row(
        "SELECT COALESCE(SUM(sv_bonus_cents),0),COALESCE(SUM(sv_principal_cents),0)
         FROM payment_record WHERE order_no=?1 AND pay_channel=?2 AND status='SUCCESS'",
        rusqlite::params![order_no, PAY_STORED_VALUE],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;

    let return_no = gen_no("RT");
    let total: i64;
    // 扫码渠道退款任务（渠道受理在事务提交后执行，受理 ≠ 成功）
    let mut scan_refunds: Vec<(String, PayChannel, i64, Option<String>)> = Vec::new();
    {
        let tx = conn.unchecked_transaction()?;
        total = checkout_core::order::create_return(
            &tx,
            &return_no,
            &order_no,
            member_id,
            operator_id,
            Some(approver_id),
            &rows,
            &refunds,
            operator_id,
        )?;
        // 渠道占比分摊 → refund_record（§6.6）
        if !shares.is_empty() {
            let allocated = checkout_core::payment::route_refund(&shares, total)?;
            let mut leg_no = 0usize;
            for (channel, amt) in allocated.into_iter() {
                // 同一渠道可能有多笔收款（组合支付/补差价）：按各笔的未退余额逐笔挂账，
                // 不能一律挂第一笔，否则对账与按笔退款都会错位。
                let pays: Vec<(i64, i64, Option<String>)> = {
                    let mut stmt = tx.prepare(
                        "SELECT pay_id, amount_cents, out_trade_no FROM payment_record
                         WHERE order_no=?1 AND pay_channel=?2 AND status='SUCCESS'
                         ORDER BY pay_id",
                    )?;
                    let rows = stmt
                        .query_map(rusqlite::params![order_no, channel.as_str()], |r| {
                            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
                        })?;
                    let v = rows.collect::<rusqlite::Result<Vec<_>>>()?;
                    drop(stmt);
                    v
                };
                let mut left = amt;
                for (pay_id, paid, otn) in &pays {
                    if left <= 0 {
                        break;
                    }
                    let refunded: i64 = tx.query_row(
                        "SELECT COALESCE(SUM(amount_cents),0) FROM refund_record
                         WHERE pay_id=?1 AND status IN ('PROCESSING','COMPLETED')",
                        [pay_id],
                        |r| r.get(0),
                    )?;
                    let room = (paid - refunded).max(0);
                    // 已无余额（此前退款失败重试）时仍挂到该笔，保留追溯关系。
                    let take = if room == 0 { left } else { left.min(room) };
                    leg_no += 1;
                    let refund_no = format!("{return_no}:R{leg_no}");
                    let is_scan = matches!(channel, PayChannel::WECHAT | PayChannel::ALIPAY);
                    let status = if is_scan { "PROCESSING" } else { "COMPLETED" };
                    tx.execute(
                        "INSERT INTO refund_record(pay_id,return_no,order_no,refund_no,amount_cents,status,created_at,updated_at)
                         VALUES (?1,?2,?3,?4,?5,?6,?7,?7)",
                        rusqlite::params![
                            pay_id, return_no, order_no, refund_no, take, status, now_str()
                        ],
                    )?;
                    if is_scan {
                        scan_refunds.push((refund_no, channel, take, otn.clone()));
                    }
                    left -= take;
                }
                if left > 0 {
                    return Err(CoreError::Validation(format!(
                        "{} 渠道可退余额不足，剩余 {} 分无法归集",
                        channel.as_str(),
                        left
                    )));
                }
            }
        }
        // 会员资产回补与退款单据同事务：退款落库成功=储值/积分已回补，
        // 不会出现"退货已记、钱没回会员"的悬空状态。
        if let Some(mid) = member_id {
            if orig_bonus > 0 || orig_principal > 0 {
                let original = checkout_core::member::Deduction {
                    bonus_cents: orig_bonus,
                    principal_cents: orig_principal,
                };
                let sv_share: i64 = checkout_core::payment::route_refund(
                    &[(PayChannel::STORED_VALUE, orig_bonus + orig_principal)],
                    total,
                )?
                .first()
                .map(|(_, a)| *a)
                .unwrap_or(0);
                if sv_share > 0 {
                    checkout_core::member::refund_mirror(
                        &tx,
                        mid,
                        &format!("{return_no}:SV"),
                        &order_no,
                        sv_share,
                        original,
                    )?;
                }
            }
            // 积分扣回（§13.4）：按退款额整元回收
            let per_yuan = cfg_i64(&tx, CFG_POINTS_PER_YUAN, DEFAULT_POINTS_PER_YUAN);
            if per_yuan > 0 && total >= 100 {
                let points_back = -(total / 100 * per_yuan);
                checkout_core::member::append_asset_log(
                    &tx,
                    mid,
                    checkout_core::member::POINTS,
                    points_back,
                    "REFUND_POINTS",
                    Some(&order_no),
                    &format!("REFUND:{return_no}:POINTS"),
                    &now_str(),
                )?;
            }
        }
        checkout_core::rbac::audit_operation(
            &tx,
            "refund_order",
            Some(operator_id),
            Some(approver_id),
            serde_json::json!({"order_no": order_no, "return_no": return_no, "amount_cents": total}),
            &now_str(),
        )?;
        tx.commit()?;
    }

    // 扫码渠道退款执行（PAY-04）：受理成功 ≠ 成功，终态由退款查询收敛
    let mut channel_failed = false;
    let mut channel_pending = false;
    for (refund_no, channel, amt, out_trade_no) in scan_refunds {
        // 0 元退款仅做账务登记，不调用渠道：无资金可退，直接终态，避免渠道全额退款语义。
        if amt <= 0 {
            conn.execute(
                "UPDATE refund_record SET status='COMPLETED', updated_at=?1 WHERE refund_no=?2",
                rusqlite::params![now_str(), refund_no],
            )?;
            continue;
        }
        if let Some(otn) = out_trade_no {
            let gw = state.gateway.of_channel(channel);
            match gw.refund(&otn, &refund_no, amt) {
                Ok(PaymentState::SUCCESS) => {
                    conn.execute(
                        "UPDATE refund_record SET status='COMPLETED', updated_at=?1 WHERE refund_no=?2",
                        rusqlite::params![now_str(), refund_no],
                    )?;
                }
                Ok(_) => {
                    // 受理成功但未终态：保持 PROCESSING，等待退款查询收敛
                    channel_pending = true;
                }
                Err(e) => {
                    conn.execute(
                        "UPDATE refund_record SET status='FAILED', updated_at=?1 WHERE refund_no=?2",
                        rusqlite::params![now_str(), refund_no],
                    )?;
                    checkout_core::rbac::audit_operation(
                        &conn,
                        "refund_channel_failed",
                        Some(operator_id),
                        Some(approver_id),
                        serde_json::json!({"refund_no": refund_no, "error": e.to_string()}),
                        &now_str(),
                    )?;
                    channel_failed = true;
                }
            }
        }
    }
    if channel_failed {
        return Err(CoreError::Validation(
            "渠道退款受理失败：退款单已标记 FAILED/待人工处理，请勿重复退货".into(),
        ));
    }
    // 退货单状态跟渠道终态一致：无微信/支付宝在途才 COMPLETED，否则保持 PROCESSING。
    if !channel_pending {
        conn.execute(
            "UPDATE sales_return SET status='COMPLETED' WHERE return_no=?1 AND status='PROCESSING'",
            [&return_no],
        )?;
    }

    Ok(total)
}

// ===== 二次授权（§12.4）=====

/// 二次授权：校验批准人（店长/管理员）密码与其权限，签发一次性 nonce。
/// 敏感操作命令消费该 nonce 后立即失效，重放被拒。
#[tauri::command]
pub fn request_sensitive_auth(
    permission: String,
    approver_username: String,
    approver_password: String,
    state: State<'_, AppState>,
) -> Result<String> {
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    let now = parse_rfc3339(&now_str())?;
    let nonce = new_nonce();
    let auth = rbac::authorize_sensitive(
        &conn,
        &approver_username,
        &approver_password,
        &permission,
        &nonce,
        now,
    )?;
    let mut pending = state
        .pending_sensitive
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    // 并发授权（如同时开退款与盘点审批）各占一槽，先清过期项防堆积。
    pending.retain(|p| p.auth.expires_at > now);
    pending.push(PendingSensitive { auth, permission });
    Ok(nonce)
}

// ===== 系统配置（system.config）=====

#[derive(Debug, Serialize)]
pub struct ConfigRow {
    pub config_key: String,
    pub config_value: String,
    /// 中文名称（界面标签）。
    pub label: String,
    /// 中文说明（界面副标题，告诉用户这个值会改变什么）。
    pub description: String,
    /// 控件类型：text / number / decimal / switch。
    pub kind: String,
    /// 是否可在系统配置页直接编辑：false 表示由专属命令管理，界面只读展示。
    pub editable: bool,
}

/// 读取全部系统配置（§15.1 扫码阈值 / §14.0 业务日切点 / §13.4 积分等）。
/// 按 CONFIG_META 的顺序返回，并带上中文名称、说明与控件类型——
/// 界面直接渲染，不再在前端维护第二份键名映射。
#[tauri::command]
pub fn get_configs(state: State<'_, AppState>) -> Result<Vec<ConfigRow>> {
    let (_op, conn) = admin_guard(&state, PERM_SYSTEM_CONFIG)?;
    let mut stmt = conn.prepare("SELECT config_key, config_value FROM system_config")?;
    let stored = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let mut values: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for row in stored {
        let (k, v) = row?;
        values.insert(k, v);
    }
    let mut out = Vec::with_capacity(checkout_core::constants::CONFIG_META.len());
    for (key, label, description, kind) in checkout_core::constants::CONFIG_META {
        let value = values.get(key).cloned().unwrap_or_default();
        out.push(ConfigRow {
            config_key: key.to_string(),
            config_value: value,
            label: label.to_string(),
            description: description.to_string(),
            kind: kind.to_string(),
            editable: checkout_core::constants::CONFIG_EDITABLE.contains(&key),
        });
    }
    Ok(out)
}

/// 更新单个系统配置（白名单：仅允许业务可配键，防止改写授权内部状态）。
/// 白名单与 get_configs 返回的「可编辑」标记同源（constants::CONFIG_EDITABLE）。
#[tauri::command]
pub fn set_config(key: String, value: String, state: State<'_, AppState>) -> Result<()> {
    let (_op, conn) = admin_guard(&state, PERM_SYSTEM_CONFIG)?;
    if !checkout_core::constants::CONFIG_EDITABLE.contains(&key.as_str()) {
        return Err(CoreError::Validation(format!("配置项不可修改: {key}")));
    }
    // 值域校验
    match key.as_str() {
        CFG_SCAN_GAP_MS => {
            let n: u64 = value
                .parse()
                .map_err(|_| CoreError::Validation("扫码阈值必须为整数".into()))?;
            if !checkout_core::scanner::validate_gap_ms(n) {
                return Err(CoreError::Validation("扫码阈值须在 20–200ms".into()));
            }
        }
        CFG_MAX_DISCOUNT_RATIO => {
            let f: f64 = value
                .parse()
                .map_err(|_| CoreError::Validation("折扣上限必须为数字".into()))?;
            if !(0.0..=1.0).contains(&f) {
                return Err(CoreError::Validation("折扣上限须在 0–1".into()));
            }
        }
        CFG_PAY_CHANNEL_CASH
        | CFG_PAY_CHANNEL_WECHAT
        | CFG_PAY_CHANNEL_ALIPAY
        | CFG_PAY_CHANNEL_SV
        | CFG_PRINT_AUTO
        | CFG_POINTS_ENABLED
        | CFG_TABLE_ALLOW_REBIND
        | CFG_ALLOW_NEGATIVE_STOCK => {
            if value != "0" && value != "1" {
                return Err(CoreError::Validation("开关只允许 0 或 1".into()));
            }
        }
        checkout_core::constants::CFG_CHESS_FEE_UNIT => {
            if value != "MINUTE" && value != "HOUR" {
                return Err(CoreError::Validation(
                    "棋牌计费单位只能是 MINUTE 或 HOUR".into(),
                ));
            }
        }
        checkout_core::constants::CFG_CHESS_MIN_MINUTES => {
            let n: i64 = value
                .parse()
                .map_err(|_| CoreError::Validation("最低计费分钟必须为整数".into()))?;
            if !(0..=1440).contains(&n) {
                return Err(CoreError::Validation("最低计费分钟须在 0–1440".into()));
            }
        }
        checkout_core::constants::CFG_CHESS_ROUND_MINUTES => {
            let n: i64 = value
                .parse()
                .map_err(|_| CoreError::Validation("取整分钟必须为整数".into()))?;
            if !(0..=120).contains(&n) {
                return Err(CoreError::Validation("取整分钟须在 0–120".into()));
            }
        }
        checkout_core::constants::CFG_CHESS_FEE_CAP_CENTS => {
            let n: i64 = value
                .parse()
                .map_err(|_| CoreError::Validation("房费封顶金额必须为整数分".into()))?;
            if !(0..=100_000_000).contains(&n) {
                return Err(CoreError::Validation("房费封顶须在 0–1000000 元".into()));
            }
        }
        checkout_core::constants::CFG_CHESS_FEE_SLOTS_JSON => {
            let slots: Vec<checkout_core::service::TimeSlot> = serde_json::from_str(&value)
                .map_err(|_| CoreError::Validation("分时段费率必须是 JSON 数组".into()))?;
            for s in slots {
                if s.from_minute < 0
                    || s.rate_cents < 0
                    || (s.unit != "MINUTE" && s.unit != "HOUR")
                    || s.to_minute.is_some_and(|to| to <= s.from_minute)
                {
                    return Err(CoreError::Validation(
                        "分时段费率格式错误：分钟与费率不得为负、单位须为 MINUTE/HOUR、结束分钟须大于起始分钟".into(),
                    ));
                }
            }
        }
        checkout_core::constants::CFG_ROUND_OFF_MODE => {
            if value != "OFF" && value != "YUAN" && value != "JIAO" {
                return Err(CoreError::Validation(
                    "抹零模式只能是 OFF / YUAN / JIAO".into(),
                ));
            }
        }
        checkout_core::constants::CFG_TABLE_MEMBER_BIND => {
            if value != "OPTIONAL" && value != "REQUIRED" {
                return Err(CoreError::Validation(
                    "开台会员策略只能是 OPTIONAL 或 REQUIRED".into(),
                ));
            }
        }
        CFG_POINTS_PER_YUAN => {
            let n: i64 = value
                .parse()
                .map_err(|_| CoreError::Validation("积分配置必须为非负整数".into()))?;
            if n < 0 {
                return Err(CoreError::Validation("积分配置必须为非负整数".into()));
            }
        }
        CFG_EXPENSE_RENT_YUAN | CFG_EXPENSE_UTILITY_YUAN | CFG_EXPENSE_LABOR_YUAN => {
            let n: i64 = value
                .parse()
                .map_err(|_| CoreError::Validation("月度费用必须为非负整数（元）".into()))?;
            if n < 0 {
                return Err(CoreError::Validation("月度费用必须为非负整数（元）".into()));
            }
        }
        CFG_ACTIVATION_ENDPOINT => {
            let v = value.trim();
            if !v.is_empty() && !(v.starts_with("http://") || v.starts_with("https://")) {
                return Err(CoreError::Validation(
                    "激活服务地址必须以 http:// 或 https:// 开头".into(),
                ));
            }
        }
        CFG_PRINT_HOST => {
            let v = value.trim();
            // 允许留空关闭；非空时只接受主机名/IP 字符集，杜绝把端口或 URL 拼进地址。
            if !v.is_empty()
                && (v.len() > 64
                    || !v
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-'))
            {
                return Err(CoreError::Validation(
                    "打印机地址只能是 IP/主机名（字母数字 . -），不含端口与协议".into(),
                ));
            }
        }
        CFG_PRINT_PORT => {
            let n: u16 = value
                .parse()
                .map_err(|_| CoreError::Validation("打印机端口必须是 1–65535 的整数".into()))?;
            if n == 0 {
                return Err(CoreError::Validation(
                    "打印机端口必须是 1–65535 的整数".into(),
                ));
            }
        }
        CFG_STORE_NO | CFG_DEVICE_NO => {
            let v = value.trim();
            if v.is_empty() || v.len() > 8 || !v.chars().all(|c| c.is_ascii_alphanumeric()) {
                return Err(CoreError::Validation(
                    "门店/终端编号必须是 1–8 位字母或数字".into(),
                ));
            }
        }
        _ => {}
    }
    conn.execute(
        "INSERT INTO system_config(config_key,config_value,updated_at) VALUES (?1,?2,?3)
         ON CONFLICT(config_key) DO UPDATE SET config_value=excluded.config_value, updated_at=excluded.updated_at",
        rusqlite::params![key, value, now_str()],
    )?;
    Ok(())
}

// ===== 称重码识别（SCN-02）=====

#[derive(Debug, Serialize)]
pub struct WeighHit {
    pub sku_id: i64,
    pub product_name: String,
    pub qty: i64,
    pub unit_price_cents: i64,
    pub line_total_cents: i64,
    pub mode: String,
}

/// 称重店内码识别：解析 EAN-13 → 匹配 sku.instore_code → 计算行数量/金额。
#[tauri::command]
pub fn lookup_weigh(code: String, state: State<'_, AppState>) -> Result<WeighHit> {
    let wc = checkout_core::scanner::parse_weighed_code(&code)
        .ok_or_else(|| CoreError::Validation("称重码非法（校验位或前缀错误）".into()))?;
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    let (sku_id, name, price): (i64, String, i64) = conn.query_row(
        "SELECT s.sku_id, p.spu_name, s.sale_price_cents
         FROM sku s JOIN spu p ON p.spu_id=s.spu_id
         WHERE s.instore_code=?1 AND p.status=1",
        [&wc.instore_code],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    let (qty, total) = checkout_core::scanner::weighed_line_total(price, &wc)
        .ok_or_else(|| CoreError::Validation("称重数值非法".into()))?;
    let mode = format!("{:?}", wc.mode);
    Ok(WeighHit {
        sku_id,
        product_name: name,
        qty,
        unit_price_cents: if qty > 0 { total / qty } else { total },
        line_total_cents: total,
        mode,
    })
}

// ===== 报表维度（§14.2~14.5）=====

#[derive(Debug, Serialize)]
pub struct ChannelReport {
    pub channels: Vec<checkout_core::report::ChannelTotal>,
    pub balanced: bool,
}

#[tauri::command]
pub fn channel_report(date: String, state: State<'_, AppState>) -> Result<ChannelReport> {
    let (_op, conn) = admin_guard(&state, PERM_REPORT_VIEW)?;
    let channels = checkout_core::report::channel_distribution(&conn, &date)?;
    let balanced = checkout_core::report::verify_channel_sum(&conn, &date)?;
    Ok(ChannelReport { channels, balanced })
}

#[tauri::command]
pub fn product_report(
    from: String,
    to: String,
    state: State<'_, AppState>,
) -> Result<Vec<checkout_core::report::ProductSales>> {
    let (_op, conn) = admin_guard(&state, PERM_REPORT_VIEW)?;
    checkout_core::report::sales_by_product(&conn, &from, &to)
}

#[tauri::command]
pub fn reconcile_csv(date: String, state: State<'_, AppState>) -> Result<String> {
    let (_op, conn) = admin_guard(&state, PERM_REPORT_EXPORT)?;
    checkout_core::report::export_reconcile_csv(&conn, &date)
}

/// 按类目维度销售（§14.3）：从商品销售聚合到类目。
#[derive(Debug, Serialize)]
pub struct CategorySales {
    pub category_id: i64,
    pub category_name: String,
    pub qty: i64,
    pub sales_cents: i64,
}

#[tauri::command]
pub fn category_report(
    from: String,
    to: String,
    state: State<'_, AppState>,
) -> Result<Vec<CategorySales>> {
    let (_op, conn) = admin_guard(&state, PERM_REPORT_VIEW)?;
    let mut stmt = conn.prepare(
        "SELECT COALESCE(c.category_id,0), COALESCE(c.name,'未分类'),
                SUM(i.qty - i.returned_qty), SUM(i.line_total_cents - i.discount_cents)
         FROM sales_order_item i
         JOIN sales_order o ON o.order_no=i.order_no
         JOIN sku s ON s.sku_id=i.sku_id
         JOIN spu p ON p.spu_id=s.spu_id
         LEFT JOIN category c ON c.category_id=p.category_id
         WHERE o.status IN ('COMPLETED','PARTIAL_REFUND') AND o.business_date BETWEEN ?1 AND ?2
         GROUP BY COALESCE(c.category_id,0) ORDER BY COALESCE(c.category_id,0)",
    )?;
    let rows = stmt.query_map(rusqlite::params![from, to], |r| {
        Ok(CategorySales {
            category_id: r.get(0)?,
            category_name: r.get(1)?,
            qty: r.get(2)?,
            sales_cents: r.get(3)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// 按收银员维度销售（§14.3）。
#[derive(Debug, Serialize)]
pub struct CashierSales {
    pub cashier_id: i64,
    pub order_count: i64,
    pub pay_cents: i64,
}

#[tauri::command]
pub fn cashier_report(
    from: String,
    to: String,
    state: State<'_, AppState>,
) -> Result<Vec<CashierSales>> {
    let (_op, conn) = admin_guard(&state, PERM_REPORT_VIEW)?;
    let mut stmt = conn.prepare(
        "SELECT o.cashier_id, COUNT(DISTINCT o.order_no), COALESCE(SUM(o.pay_cents),0)
         FROM sales_order o
         WHERE o.status IN ('COMPLETED','PARTIAL_REFUND') AND o.business_date BETWEEN ?1 AND ?2
         GROUP BY o.cashier_id ORDER BY o.cashier_id",
    )?;
    let rows = stmt.query_map(rusqlite::params![from, to], |r| {
        Ok(CashierSales {
            cashier_id: r.get(0)?,
            order_count: r.get(1)?,
            pay_cents: r.get(2)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

// ===== 统一报表中心（§14 扩展）：34 张报表共用一个命令 =====

/// 报表清单：按分组返回全部报表元数据，前端据此渲染左侧导航。
#[tauri::command]
pub fn report_catalog(
    state: State<'_, AppState>,
) -> Result<Vec<checkout_core::report_catalog::ReportMeta>> {
    let (_op, _conn) = admin_guard(&state, PERM_REPORT_VIEW)?;
    Ok(checkout_core::report_catalog::report_catalog())
}

/// 统一报表查询：按 report_id 分发到对应 SQL，日期区间为营业日/创建日。
#[tauri::command]
pub fn generic_report(
    report_id: String,
    from: String,
    to: String,
    state: State<'_, AppState>,
) -> Result<checkout_core::report_catalog::ReportResult> {
    let (_op, conn) = admin_guard(&state, PERM_REPORT_VIEW)?;
    checkout_core::report_catalog::run_report(&conn, &report_id, &from, &to)
}

/// 报表导出（Android）：CSV 写入应用专属外部数据目录 reports/（免存储权限，
/// Android 10+ scoped storage 下外部 Download 直写不可靠）。返回完整路径，
/// 前端 toast 展示，用户可用系统文件管理器打开。
#[tauri::command]
pub fn export_report_csv(
    report_id: String,
    from: String,
    to: String,
    state: State<'_, AppState>,
) -> Result<String> {
    let (_op, conn) = admin_guard(&state, PERM_REPORT_EXPORT)?;
    let result = checkout_core::report_catalog::run_report(&conn, &report_id, &from, &to)?;
    let csv = checkout_core::report_catalog::report_to_csv(&result);

    let dir = state.data_dir.join("reports");
    std::fs::create_dir_all(&dir)
        .map_err(|e| CoreError::Other(format!("创建导出目录失败: {e}")))?;
    let path = dir.join(format!("{report_id}_{from}_{to}.csv"));
    std::fs::write(&path, &csv)
        .map_err(|e| CoreError::Other(format!("写入导出文件失败: {e}")))?;
    Ok(path.to_string_lossy().to_string())
}

// ===== 交班闭环（§5.4 / F-09）=====

/// 开班：创建班次并设为当前班次，收银结算引用其 shift_id。
#[tauri::command]
pub fn open_shift(opening_float_cents: i64, state: State<'_, AppState>) -> Result<i64> {
    let session = state
        .session
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    let cashier_id = session
        .as_ref()
        .map(|s| s.user_id)
        .ok_or(CoreError::NotAuthenticated)?;
    let _session = session;
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    if opening_float_cents < 0 {
        return Err(CoreError::Validation("备用金不能为负".into()));
    }
    // 单机只允许一个 OPEN 班次：并行班次会让现金/单据串账。
    let open: Option<i64> = conn
        .query_row(
            "SELECT shift_id FROM shift WHERE status='OPEN' ORDER BY shift_id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(id) = open {
        return Err(CoreError::Validation(format!(
            "已存在未关闭班次 #{id}，请先交班"
        )));
    }
    let now = now_str();
    let cashiers = serde_json::json!([cashier_id]).to_string();
    conn.execute(
        "INSERT INTO shift(device_id,cashier_ids_json,opened_at,opening_float_cents,status)
         VALUES ('POS1',?1,?2,?3,'OPEN')",
        rusqlite::params![cashiers, now, opening_float_cents],
    )?;
    let shift_id = conn.last_insert_rowid();
    *state
        .current_shift
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))? = shift_id;
    Ok(shift_id)
}

#[derive(Debug, Serialize)]
pub struct CurrentShift {
    pub shift_id: i64,
    pub opened_at: String,
    pub opening_float_cents: i64,
}

/// 当前未关闭班次：应用重启后前端据此恢复"当前班次"，否则关班会打到 #0 上。
#[tauri::command]
pub fn current_shift(state: State<'_, AppState>) -> Result<Option<CurrentShift>> {
    let (_operator_id, conn) = admin_guard(&state, PERM_POS_CHECKOUT)?;
    let row = conn
        .query_row(
            "SELECT shift_id, opened_at, opening_float_cents FROM shift
             WHERE status='OPEN' ORDER BY shift_id DESC LIMIT 1",
            [],
            |r| {
                Ok(CurrentShift {
                    shift_id: r.get(0)?,
                    opened_at: r.get(1)?,
                    opening_float_cents: r.get(2)?,
                })
            },
        )
        .optional()?;
    Ok(row)
}

#[derive(Debug, Serialize)]
pub struct ShiftCloseResult {
    pub shift_id: i64,
    pub expected_cash_cents: i64,
    pub actual_cash_cents: i64,
    pub difference_cents: i64,
}

/// 关班：录入实收现金，计算与应收现金差额（含备用金），写审计并关闭班次。
#[tauri::command]
pub fn close_shift(
    shift_id: i64,
    actual_cash_cents: i64,
    difference_reason: Option<String>,
    sensitive_nonce: Option<String>,
    state: State<'_, AppState>,
) -> Result<ShiftCloseResult> {
    let now_dt = parse_rfc3339(&now_str())?;
    let auth = consume_sensitive(&state, "pos.shift.close", sensitive_nonce, now_dt)?;
    let license = state
        .license
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    let session = state
        .session
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    let session = session.as_ref().ok_or(CoreError::NotAuthenticated)?;
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    checkout_core::rbac::guard(
        &conn,
        &checkout_core::rbac::GuardRequest {
            entitlement: ENT_POS_CORE,
            permission: "pos.shift.close",
            session,
            license: &license,
            sensitive_auth: Some(&auth),
            current_nonce: &auth.nonce,
            now: now_dt,
            amount_cents: 0,
            limit_key: None,
        },
    )?;
    let operator_id = session.user_id;
    let role_name = session.role_name.clone();
    let approver_id = auth.approver_id;
    let _ = session;
    drop(license);
    let (opening, status, cashiers_json, opened_at): (i64, String, String, String) = conn
        .query_row(
            "SELECT opening_float_cents,status,COALESCE(cashier_ids_json,'[]'),opened_at
             FROM shift WHERE shift_id=?1",
            [shift_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;
    if status != "OPEN" {
        return Err(CoreError::Validation(format!(
            "班次 {shift_id} 状态为 {status}，不可重复关班"
        )));
    }
    if actual_cash_cents < 0 {
        return Err(CoreError::Validation("实收现金不能为负数".into()));
    }
    // 归属守卫：本人班次可直接交班；店长/管理员可代交，其他收银员不行。
    let owns = serde_json::from_str::<Vec<i64>>(&cashiers_json)
        .map(|ids| ids.contains(&operator_id))
        .unwrap_or(false);
    if !owns && !matches!(role_name.as_str(), ROLE_MANAGER | ROLE_ADMIN) {
        return Err(CoreError::PermissionDenied(
            "只能交自己当班的班次，或由店长/管理员代交".into(),
        ));
    }
    // 应收现金 = 备用金 + 现金收款 - 现金退款 + 班次内现金充值
    let cash_net: i64 = conn.query_row(
        "SELECT COALESCE(SUM(CASE WHEN p.pay_channel='CASH' THEN p.amount_cents ELSE 0 END),0)
         FROM sales_order o JOIN payment_record p ON p.order_no=o.order_no AND p.status='SUCCESS'
         WHERE o.shift_id=?1 AND o.status IN ('COMPLETED','PARTIAL_REFUND')",
        [shift_id],
        |r| r.get(0),
    )?;
    let cash_refund: i64 = conn.query_row(
        "SELECT COALESCE(SUM(r.amount_cents),0)
         FROM refund_record r
         JOIN payment_record p ON p.pay_id=r.pay_id
         JOIN sales_order o ON o.order_no=r.order_no
         WHERE o.shift_id=?1 AND p.pay_channel='CASH'
           AND r.status='COMPLETED'",
        [shift_id],
        |r| r.get(0),
    )?;
    let recharge_cash: i64 = conn.query_row(
        "SELECT COALESCE(SUM(p.amount_cents),0)
         FROM payment_record p
         WHERE p.pay_channel='CASH' AND p.status='SUCCESS'
           AND p.created_at >= ?1 AND p.created_at <= ?2
           AND p.order_no NOT IN (SELECT order_no FROM sales_order)",
        rusqlite::params![opened_at, now_str()],
        |r| r.get(0),
    )?;
    let expected_cash = opening + cash_net - cash_refund + recharge_cash;
    let diff = actual_cash_cents - expected_cash;
    let reason = difference_reason
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    // 长短款必须写原因：没有说明的差额在审计里无法追责，也无法与后续对账闭环。
    if diff != 0 && reason.is_none() {
        return Err(CoreError::Validation(format!(
            "现金差额 {} 元，必须填写差异原因",
            checkout_core::money::cents_to_yuan(diff)
        )));
    }
    // 条件更新 + affected 行校验：并发/重复关班不会写出两份账。
    let updated = conn.execute(
        "UPDATE shift SET closed_at=?1, actual_cash_cents=?2, expected_cash_cents=?3,
                difference_cents=?4, difference_reason=?5, status='CLOSED'
         WHERE shift_id=?6 AND status='OPEN'",
        rusqlite::params![
            now_str(),
            actual_cash_cents,
            expected_cash,
            diff,
            reason,
            shift_id
        ],
    )?;
    if updated != 1 {
        return Err(CoreError::Validation("班次已被关闭，拒绝重复关班".into()));
    }
    checkout_core::rbac::audit_operation(
        &conn,
        "close_shift",
        Some(operator_id),
        Some(approver_id),
        serde_json::json!({"shift_id": shift_id, "difference_cents": diff}),
        &now_str(),
    )?;
    let current: i64 = *state
        .current_shift
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    if current == shift_id {
        *state
            .current_shift
            .lock()
            .map_err(|e| CoreError::Other(e.to_string()))? = 0;
    }
    Ok(ShiftCloseResult {
        shift_id,
        expected_cash_cents: expected_cash,
        actual_cash_cents,
        difference_cents: diff,
    })
}

// ===== 打印配置与重打队列（§7.6 / PRT-06）=====

#[tauri::command]
pub fn print_retries(state: State<'_, AppState>) -> Result<Vec<String>> {
    let (_op, conn) = admin_guard(&state, PERM_REPORT_VIEW)?;
    checkout_core::escpos::pending_retries(&conn)
}

/// 手动重打：复用自动打印同一渲染/发送路径（真实订单变量 + 打印机），
/// 只有确认送达才出队；失败保留 PENDING 供再次重打（PRT-06）。
#[tauri::command]
pub async fn retry_print(order_no: String, state: State<'_, AppState>) -> Result<String> {
    let (_op, conn) = admin_guard(&state, PERM_REPORT_VIEW)?;
    let msg = print_receipt_inner(&conn, &order_no)?;
    if msg == "小票已打印" {
        conn.execute(
            "UPDATE print_retry_queue SET status='DONE', retry_count=retry_count+1
             WHERE order_no=?1 AND status='PENDING'",
            [&order_no],
        )?;
    }
    Ok(msg)
}

// ===== 券管理（§13.7 / MEM-04）=====

#[derive(Debug, Serialize)]
pub struct CouponRow {
    pub coupon_code: String,
    pub member_id: Option<i64>,
    /// 归属会员名（通用券为 None），仅用于列表展示归属。
    pub member_name: Option<String>,
    pub discount_cents: i64,
    pub min_order_cents: i64,
    pub status: String,
    pub valid_until: Option<String>,
}

#[tauri::command]
pub fn list_coupons(state: State<'_, AppState>) -> Result<Vec<CouponRow>> {
    let (_op, conn) = admin_guard(&state, PERM_REPORT_VIEW)?;
    let mut stmt = conn.prepare(
        "SELECT c.coupon_code,c.member_id,m.name,c.discount_cents,c.min_order_cents,c.status,c.valid_until
         FROM coupon c LEFT JOIN member m ON c.member_id=m.member_id
         ORDER BY c.coupon_id DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(CouponRow {
            coupon_code: r.get(0)?,
            member_id: r.get(1)?,
            member_name: r.get(2)?,
            discount_cents: r.get(3)?,
            min_order_cents: r.get(4)?,
            status: r.get(5)?,
            valid_until: r.get(6)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// 发券（§13.7）：状态机从 UNUSED 起。
/// 单个操作员每日手工发券上限（防止无审批批量刷券）。
const DAILY_COUPON_ISSUE_LIMIT: i64 = 50;

#[tauri::command]
pub fn issue_coupon(
    member_id: Option<i64>,
    discount_cents: i64,
    min_order_cents: i64,
    valid_until: Option<String>,
    state: State<'_, AppState>,
) -> Result<String> {
    let (operator_id, conn) = admin_guard(&state, PERM_MEMBER_MANAGE)?;
    if discount_cents <= 0 || min_order_cents < 0 {
        return Err(CoreError::Validation("券额度/门槛非法".into()));
    }
    if discount_cents > min_order_cents && min_order_cents > 0 {
        return Err(CoreError::Validation(
            "券面额不能高于使用门槛（否则等于无条件大额折扣）".into(),
        ));
    }
    if let Some(mid) = member_id {
        let exists: i64 = conn.query_row(
            "SELECT COUNT(*) FROM member WHERE member_id=?1",
            [mid],
            |r| r.get(0),
        )?;
        if exists == 0 {
            return Err(CoreError::Validation(format!("会员 {mid} 不存在")));
        }
    }
    // 前端 datetime-local 不带时区：按门店时区（UTC+8）解释后统一存 UTC RFC3339，
    // 避免字符串比较时把本地时间当 UTC 用，券提前/延后 8 小时失效。
    let valid_until = match valid_until
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        Some(v) => {
            let utc = if let Ok(dt) = parse_rfc3339(v) {
                dt
            } else if let Ok(ndt) = NaiveDateTime::parse_from_str(v, "%Y-%m-%dT%H:%M:%S") {
                ndt.and_local_timezone(FixedOffset::east_opt(8 * 3600).unwrap())
                    .single()
                    .ok_or_else(|| CoreError::Validation("券有效期时区非法".into()))?
                    .with_timezone(&Utc)
            } else if let Ok(ndt) = NaiveDateTime::parse_from_str(v, "%Y-%m-%dT%H:%M") {
                ndt.and_local_timezone(FixedOffset::east_opt(8 * 3600).unwrap())
                    .single()
                    .ok_or_else(|| CoreError::Validation("券有效期时区非法".into()))?
                    .with_timezone(&Utc)
            } else if let Ok(d) = NaiveDate::parse_from_str(v, "%Y-%m-%d") {
                d.and_hms_opt(23, 59, 59)
                    .unwrap()
                    .and_local_timezone(FixedOffset::east_opt(8 * 3600).unwrap())
                    .single()
                    .ok_or_else(|| CoreError::Validation("券有效期时区非法".into()))?
                    .with_timezone(&Utc)
            } else {
                return Err(CoreError::Validation(
                    "券有效期格式非法，应为 YYYY-MM-DDTHH:mm".into(),
                ));
            };
            if utc <= Utc::now() {
                return Err(CoreError::Validation("券有效期必须晚于当前时间".into()));
            }
            Some(format_utc(&utc))
        }
        None => None,
    };
    let today = now_str();
    let prefix = &today[..10];
    let issued_today: i64 = conn.query_row(
        "SELECT COUNT(*) FROM operation_audit
         WHERE command='issue_coupon' AND operator_id=?1 AND created_at LIKE ?2",
        rusqlite::params![operator_id, format!("{prefix}%")],
        |r| r.get(0),
    )?;
    if issued_today >= DAILY_COUPON_ISSUE_LIMIT {
        return Err(CoreError::AmountLimitExceeded(format!(
            "今日发券已达上限 {DAILY_COUPON_ISSUE_LIMIT} 张，请走审批或明日再发"
        )));
    }
    let code = format!("CP{}-{}", Utc::now().timestamp_millis(), issued_today + 1);
    checkout_core::rbac::audit_operation(
        &conn,
        "issue_coupon",
        Some(operator_id),
        None,
        serde_json::json!({
            "coupon_code": code,
            "member_id": member_id,
            "discount_cents": discount_cents,
            "min_order_cents": min_order_cents,
            "valid_until": valid_until,
        }),
        &today,
    )?;
    conn.execute(
        "INSERT INTO coupon(coupon_code,member_id,discount_cents,min_order_cents,status,valid_until)
         VALUES (?1,?2,?3,?4,'UNUSED',?5)",
        rusqlite::params![code, member_id, discount_cents, min_order_cents, valid_until],
    )?;
    Ok(code)
}

// ===== Native 支付收口 / 支付凭据 / 账单对账 / 二维码 =====

/// 轮询 Native 支付结果（§6.3）。SUCCESS → 资产收口 + 订单提交。
#[tauri::command]
pub async fn poll_native_pay(
    order_no: String,
    state: State<'_, AppState>,
) -> Result<CheckoutResult> {
    {
        let license = state
            .license
            .lock()
            .map_err(|e| CoreError::Other(e.to_string()))?;
        license.gate(ENT_POS_CORE)?;
    }
    {
        let session = state
            .session
            .lock()
            .map_err(|e| CoreError::Other(e.to_string()))?;
        let s = session.as_ref().ok_or(CoreError::NotAuthenticated)?;
        if s.expires_at < parse_rfc3339(&now_str())? {
            return Err(CoreError::SessionExpired);
        }
    }
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    let status: String = conn
        .query_row(
            "SELECT status FROM sales_order WHERE order_no=?1",
            [&order_no],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| CoreError::Validation("订单不存在".into()))?;
    if status != "PENDING_PAYMENT" {
        // 幂等：已完成/已作废直接回当前状态；已完成的单回显本单已累计积分
        let points_earned: i64 = conn
            .query_row(
                "SELECT COALESCE(SUM(change_amount),0) FROM member_asset_log
                 WHERE ref_order_no=?1 AND asset_type='POINTS' AND reason='SALE_POINTS'",
                [&order_no],
                |r| r.get(0),
            )
            .unwrap_or(0);
        return Ok(CheckoutResult {
            order_no,
            status,
            code_url: None,
            points_earned,
        });
    }
    let (out_trade_no, channel_str): (String, String) = conn
        .query_row(
            "SELECT out_trade_no, pay_channel FROM payment_record
             WHERE order_no=?1 AND pay_mode='NATIVE' AND status='PENDING' ORDER BY pay_id LIMIT 1",
            [&order_no],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| CoreError::Validation("订单无待支付 Native 流水".into()))?;
    let channel = match channel_str.as_str() {
        PAY_WECHAT => PayChannel::WECHAT,
        PAY_ALIPAY => PayChannel::ALIPAY,
        other => return Err(CoreError::Validation(format!("未知渠道 {other}"))),
    };
    let st = state.gateway.of_channel(channel).query(&out_trade_no)?;
    match st {
        PaymentState::SUCCESS => {
            conn.execute(
                "UPDATE payment_record SET status='SUCCESS', paid_at=?1
                 WHERE order_no=?2 AND out_trade_no=?3",
                rusqlite::params![now_str(), order_no, out_trade_no],
            )?;
            let points_earned = finalize_order_assets(&conn, &order_no)?;
            auto_print_receipt(&conn, &order_no);
            Ok(CheckoutResult {
                order_no,
                status: "COMPLETED".into(),
                code_url: None,
                points_earned,
            })
        }
        PaymentState::CLOSED => {
            conn.execute(
                "UPDATE payment_record SET status='CLOSED' WHERE order_no=?1 AND out_trade_no=?2",
                rusqlite::params![order_no, out_trade_no],
            )?;
            release_order_coupon(&conn, &order_no)?;
            Ok(CheckoutResult {
                order_no,
                status: "CLOSED".into(),
                code_url: None,
                points_earned: 0,
            })
        }
        PaymentState::FAILED(msg) => Ok(CheckoutResult {
            order_no,
            status: format!("FAILED:{msg}"),
            code_url: None,
            points_earned: 0,
        }),
        PaymentState::USERPAYING => Ok(CheckoutResult {
            order_no,
            status: "PENDING_PAYMENT".into(),
            code_url: None,
            points_earned: 0,
        }),
    }
}

/// 取消 Native 支付：渠道关单；遇"已支付"自动全额退款 + 审计（§6.2）。
#[tauri::command]
pub fn cancel_native_pay(order_no: String, state: State<'_, AppState>) -> Result<()> {
    // 撤销在途收款等同退款级敏感操作：必须登录 + 退款权限 + 审计，不能裸调渠道关单。
    let (operator_id, conn) = admin_guard(&state, PERM_POS_REFUND)?;
    let (out_trade_no, channel_str, amount): (String, String, i64) = conn
        .query_row(
            "SELECT out_trade_no, pay_channel, amount_cents FROM payment_record
             WHERE order_no=?1 AND pay_mode='NATIVE' AND status='PENDING' ORDER BY pay_id LIMIT 1",
            [&order_no],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| CoreError::Validation("订单无待支付 Native 流水".into()))?;
    let channel = match channel_str.as_str() {
        PAY_WECHAT => PayChannel::WECHAT,
        PAY_ALIPAY => PayChannel::ALIPAY,
        other => return Err(CoreError::Validation(format!("未知渠道 {other}"))),
    };
    let gw = state.gateway.of_channel(channel);
    let now = now_str();
    let outcome = match gw.cancel(&out_trade_no)? {
        checkout_core::payment::CancelResult::Canceled => "CANCELED".to_string(),
        checkout_core::payment::CancelResult::AlreadyPaid => {
            // 渠道已扣款 → 自动全额退款（幂等 refund_no）+ 审计。
            // 退款结果必须按渠道终态判定：处理中/失败都不能宣告"已退款"并关单，
            // 否则钱在商户账上、本地已当退款完成。
            let refund_no = format!("{out_trade_no}:AUTO_REFUND");
            match gw.refund(&out_trade_no, &refund_no, amount) {
                Ok(PaymentState::SUCCESS) => {
                    checkout_core::rbac::audit_operation(
                        &conn,
                        "native_cancel_already_paid",
                        Some(operator_id),
                        None,
                        serde_json::json!({"order_no": order_no, "auto_refunded": amount}),
                        &now,
                    )?;
                    "AUTO_REFUNDED".to_string()
                }
                Ok(other) => {
                    checkout_core::rbac::audit_operation(
                        &conn,
                        "native_cancel_refund_pending",
                        Some(operator_id),
                        None,
                        serde_json::json!({
                            "order_no": order_no, "amount": amount, "state": format!("{other:?}")
                        }),
                        &now,
                    )?;
                    return Err(CoreError::Validation(format!(
                        "渠道已扣款，自动退款状态为 {other:?}；订单保持待支付，请稍后在对账中确认退款结果"
                    )));
                }
                Err(e) => {
                    checkout_core::rbac::audit_operation(
                        &conn,
                        "native_cancel_refund_failed",
                        Some(operator_id),
                        None,
                        serde_json::json!({"order_no": order_no, "amount": amount, "error": e.to_string()}),
                        &now,
                    )?;
                    return Err(CoreError::Validation(format!(
                        "渠道已扣款但自动退款失败（{e}），订单保持待支付，请人工在渠道退款"
                    )));
                }
            }
        }
    };
    // 只有渠道确认关单/退款成功，才把流水关闭并取消订单、释放券。
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE payment_record SET status='CLOSED' WHERE order_no=?1 AND status='PENDING'",
        [order_no.clone()],
    )?;
    tx.execute(
        "UPDATE sales_order SET status='CANCELED' WHERE order_no=?1 AND status='PENDING_PAYMENT'",
        [order_no.clone()],
    )?;
    release_order_coupon(&tx, &order_no)?;
    tx.commit()?;
    checkout_core::rbac::audit_operation(
        &conn,
        "cancel_native_pay",
        Some(operator_id),
        None,
        serde_json::json!({"order_no": order_no, "outcome": outcome}),
        &now,
    )?;
    Ok(())
}

/// 本地确认 Native 支付完成（无真实渠道凭据时的本地确认模式）。
#[tauri::command]
pub fn confirm_native_pay(order_no: String, state: State<'_, AppState>) -> Result<i64> {
    let license = state
        .license
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    license.gate(ENT_POS_CORE)?;
    let session = state
        .session
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    let session = session.as_ref().ok_or(CoreError::NotAuthenticated)?;
    let now_dt = parse_rfc3339(&now_str())?;
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    // 人工确认到账等同收款确认：必须登录 + 具备退款级权限，并留审计（不静默改状态）。
    checkout_core::rbac::guard(
        &conn,
        &checkout_core::rbac::GuardRequest {
            entitlement: ENT_POS_CORE,
            permission: PERM_POS_REFUND,
            session,
            license: &license,
            sensitive_auth: None,
            current_nonce: "",
            now: now_dt,
            amount_cents: 0,
            limit_key: None,
        },
    )?;
    let operator_id = session.user_id;
    let _ = session;
    drop(license);
    // 人工确认只服务于 LOCAL 网关（未配置真实渠道）。一旦接入微信/支付宝，
    // 到账必须由渠道查单确认，人工按钮不能绕过渠道终态，否则可凭空把订单置为已付。
    if state.gateway.wechat.is_some() || state.gateway.alipay.is_some() {
        return Err(CoreError::Validation(
            "已配置真实支付渠道：Native 支付请使用渠道查单确认，禁止人工确认".into(),
        ));
    }
    let out_trade_no: String = conn
        .query_row(
            "SELECT out_trade_no FROM payment_record
             WHERE order_no=?1 AND pay_mode='NATIVE' AND status='PENDING' ORDER BY pay_id LIMIT 1",
            [&order_no],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| CoreError::Validation("订单无待支付 Native 流水".into()))?;
    checkout_core::payment::confirm_native(&state.gateway.local, &out_trade_no)?;
    conn.execute(
        "UPDATE payment_record SET status='SUCCESS', paid_at=?1
         WHERE order_no=?2 AND out_trade_no=?3",
        rusqlite::params![now_str(), order_no, out_trade_no],
    )?;
    let points_earned = finalize_order_assets(&conn, &order_no)?;
    checkout_core::rbac::audit_operation(
        &conn,
        "confirm_native_pay",
        Some(operator_id),
        None,
        serde_json::json!({"order_no": order_no, "out_trade_no": out_trade_no}),
        &now_str(),
    )?;
    auto_print_receipt(&conn, &order_no);
    Ok(points_earned)
}

/// 订单上的待用券释放（REDEEM/LOCKED → UNUSED，§13.7 取消路径）。
fn release_order_coupon(conn: &Connection, order_no: &str) -> Result<()> {
    let code: Option<String> = conn
        .query_row(
            "SELECT coupon_code FROM sales_order WHERE order_no=?1",
            [order_no],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    if let Some(code) = code {
        let status: String = conn.query_row(
            "SELECT status FROM coupon WHERE coupon_code=?1",
            [&code],
            |r| r.get(0),
        )?;
        if status == "LOCKED" {
            checkout_core::member::transition_coupon(conn, &code, COUPON_UNUSED)?;
        }
    }
    Ok(())
}

/// 写入支付商户凭据（§6.8）：存操作系统凭据库，不落库不入日志。
#[tauri::command]
pub fn set_payment_credential(
    channel: String,
    field: String,
    value: String,
    state: State<'_, AppState>,
) -> Result<()> {
    let (operator_id, _conn) = admin_guard(&state, PERM_SYSTEM_CONFIG)?;
    validate_credential_field(&channel, &field)?;
    checkout_core::paycreds::KeyringStore.set_secret(&field, &value)?;
    checkout_core::rbac::audit_operation(
        &_conn,
        "set_payment_credential",
        Some(operator_id),
        None,
        serde_json::json!({"channel": channel, "field": field}),
        &now_str(),
    )?;
    Ok(())
}

/// 删除支付商户凭据。
#[tauri::command]
pub fn delete_payment_credential(
    channel: String,
    field: String,
    state: State<'_, AppState>,
) -> Result<()> {
    let (operator_id, _conn) = admin_guard(&state, PERM_SYSTEM_CONFIG)?;
    validate_credential_field(&channel, &field)?;
    checkout_core::paycreds::KeyringStore.delete_secret(&field)?;
    checkout_core::rbac::audit_operation(
        &_conn,
        "delete_payment_credential",
        Some(operator_id),
        None,
        serde_json::json!({"channel": channel, "field": field}),
        &now_str(),
    )?;
    Ok(())
}

/// 凭据字段合法性：必须进白名单且渠道前缀匹配。
fn validate_credential_field(channel: &str, field: &str) -> Result<()> {
    if !checkout_core::paycreds::ALLOWED_FIELDS.contains(&field) {
        return Err(CoreError::Validation("非法凭据字段".into()));
    }
    let prefix_ok = match channel {
        "WECHAT" => field.starts_with("wechat."),
        "ALIPAY" => field.starts_with("alipay."),
        _ => false,
    };
    if !prefix_ok {
        return Err(CoreError::Validation("凭据字段与渠道不匹配".into()));
    }
    Ok(())
}

/// 网关配置状态（设置页展示，只报是否已配置，绝不回显明文）。
#[derive(Debug, Serialize)]
pub struct GatewayStatus {
    pub wechat_mode: String,
    pub alipay_mode: String,
    pub wechat_error: Option<String>,
    pub alipay_error: Option<String>,
}

/// 渠道开关（前台可读，POS 收银台过滤可选渠道；§1.2 生存能力不阻断）。
#[derive(Debug, Serialize)]
pub struct PayChannels {
    pub cash: bool,
    pub wechat: bool,
    pub alipay: bool,
    pub stored_value: bool,
}

#[tauri::command]
pub fn get_pay_channels(state: State<'_, AppState>) -> Result<PayChannels> {
    let license = state
        .license
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    // 前台开单生存能力：任何非 UNACTIVATED 状态不阻断
    license.gate(ENT_POS_CORE)?;
    drop(license);
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    let on =
        |key: &str| -> Result<bool> { Ok(cfg_get(&conn, key)?.map(|v| v != "0").unwrap_or(true)) };
    Ok(PayChannels {
        cash: on(CFG_PAY_CHANNEL_CASH)?,
        wechat: on(CFG_PAY_CHANNEL_WECHAT)?,
        alipay: on(CFG_PAY_CHANNEL_ALIPAY)?,
        stored_value: on(CFG_PAY_CHANNEL_SV)?,
    })
}

#[tauri::command]
pub fn payment_gateway_status() -> Result<GatewayStatus> {
    let store = checkout_core::paycreds::KeyringStore;
    let (wechat_mode, wechat_error) = match checkout_core::paycreds::load_wechat(&store) {
        Ok(Some(_)) => ("REAL".into(), None),
        Ok(None) => ("LOCAL".into(), None),
        Err(e) => ("LOCAL".into(), Some(e.to_string())),
    };
    let (alipay_mode, alipay_error) = match checkout_core::paycreds::load_alipay(&store) {
        Ok(Some(_)) => ("REAL".into(), None),
        Ok(None) => ("LOCAL".into(), None),
        Err(e) => ("LOCAL".into(), Some(e.to_string())),
    };
    Ok(GatewayStatus {
        wechat_mode,
        alipay_mode,
        wechat_error,
        alipay_error,
    })
}

/// 导入微信交易账单并对账（PAY-05/06）。
#[tauri::command]
pub fn import_wechat_bill(
    recon_date: String,
    csv_text: String,
    state: State<'_, AppState>,
) -> Result<checkout_core::bill::ReconcileSummary> {
    let (_op, conn) = admin_guard(&state, PERM_REPORT_EXPORT)?;
    let rows = checkout_core::bill::parse_wechat_bill(&csv_text)?;
    checkout_core::bill::reconcile_channel(&conn, PAY_WECHAT, &recon_date, &rows)
}

/// 导入支付宝对账单并对账。
#[tauri::command]
pub fn import_alipay_bill(
    recon_date: String,
    csv_text: String,
    state: State<'_, AppState>,
) -> Result<checkout_core::bill::ReconcileSummary> {
    let (_op, conn) = admin_guard(&state, PERM_REPORT_EXPORT)?;
    let rows = checkout_core::bill::parse_alipay_bill(&csv_text)?;
    checkout_core::bill::reconcile_channel(&conn, PAY_ALIPAY, &recon_date, &rows)
}

/// 当日对账差异清单（只告警不抹平，§6.7）。
#[tauri::command]
pub fn list_reconcile_diffs(
    recon_date: String,
    state: State<'_, AppState>,
) -> Result<Vec<serde_json::Value>> {
    let (_op, conn) = admin_guard(&state, PERM_REPORT_VIEW)?;
    let mut stmt = conn.prepare(
        "SELECT recon_id, channel, diff_type, order_no, local_amount_cents, channel_amount_cents, detail_json, created_at
         FROM reconcile_record WHERE recon_date=?1 ORDER BY recon_id",
    )?;
    let rows = stmt.query_map([&recon_date], |r| {
        Ok(serde_json::json!({
            "recon_id": r.get::<_, i64>(0)?,
            "channel": r.get::<_, String>(1)?,
            "diff_type": r.get::<_, String>(2)?,
            "order_no": r.get::<_, Option<String>>(3)?,
            "local_amount_cents": r.get::<_, i64>(4)?,
            "channel_amount_cents": r.get::<_, i64>(5)?,
            "detail": r.get::<_, Option<String>>(6)?,
            "created_at": r.get::<_, String>(7)?,
        }))
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// 文本 → 二维码 SVG（Native 支付/重打小票用，离线生成无外部依赖）。
#[tauri::command]
pub fn qr_svg(text: String) -> Result<String> {
    let code = qrcode::QrCode::new(text.as_bytes())
        .map_err(|e| CoreError::Other(format!("二维码生成失败: {e}")))?;
    let colors = code.to_colors();
    let w = code.width();
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{w}\" viewBox=\"0 0 {w} {w}\" shape-rendering=\"crispEdges\"><rect width=\"{w}\" height=\"{w}\" fill=\"#ffffff\"/>"
    );
    for y in 0..w {
        for x in 0..w {
            if colors[y * w + x] == qrcode::types::Color::Dark {
                svg.push_str(&format!(
                    "<rect x=\"{x}\" y=\"{y}\" width=\"1\" height=\"1\" fill=\"#000000\"/>"
                ));
            }
        }
    }
    svg.push_str("</svg>");
    Ok(svg)
}

/// 崩溃恢复扫描（§23）：PENDING_PAYMENT 订单向渠道查单收敛。
/// 收口只看「已确认金额 + 待补扣储值」是否覆盖应付；未覆盖且渠道未明确失败的一律保持原状，
/// 绝不作废（顾客可能已扣款）。全部处理留审计。返回处理单数。
pub fn recover_pending_orders(
    conn: &Connection,
    gateway: &checkout_core::payment::RoutingGateway,
) -> Result<usize> {
    let pendings: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT order_no FROM sales_order WHERE status='PENDING_PAYMENT' ORDER BY created_at",
        )?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()?
    };
    let mut recovered = 0;
    for order_no in pendings {
        let records: Vec<(String, String, String, String)> = {
            let mut stmt = conn.prepare(
                "SELECT IFNULL(pay_channel,''), IFNULL(pay_mode,''), IFNULL(out_trade_no,''), status
                 FROM payment_record WHERE order_no=?1",
            )?;
            let rows = stmt.query_map([&order_no], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };
        let payable: i64 = conn.query_row(
            "SELECT total_cents - discount_cents FROM sales_order WHERE order_no=?1",
            [&order_no],
            |r| r.get(0),
        )?;
        let confirmed: i64 = conn.query_row(
            "SELECT COALESCE(SUM(amount_cents),0) FROM payment_record
             WHERE order_no=?1 AND status='SUCCESS'",
            [&order_no],
            |r| r.get(0),
        )?;
        let sv_pending: i64 = conn.query_row(
            "SELECT COALESCE(SUM(amount_cents),0) FROM payment_record
             WHERE order_no=?1 AND pay_channel=?2 AND status='PENDING'",
            rusqlite::params![order_no, PAY_STORED_VALUE],
            |r| r.get(0),
        )?;
        // 收口口径 = 已确认支付 + 待补扣储值，必须覆盖应付才允许提交。
        if payable > 0 && confirmed + sv_pending >= payable {
            finalize_order_assets(conn, &order_no)?;
            recovered += 1;
            continue;
        }

        // 未决扫码流水逐条查单：只有渠道明确 CLOSED/FAILED 才算未支付。
        // 查单失败、USERPAYING、部分已付一律保持原状，绝不贸然作废（顾客可能已扣款）。
        let mut unresolved = confirmed > 0;
        for (ch, _mode, otn, st) in &records {
            if st != "PENDING" || otn.is_empty() {
                continue;
            }
            let channel = match ch.as_str() {
                PAY_WECHAT => PayChannel::WECHAT,
                PAY_ALIPAY => PayChannel::ALIPAY,
                _ => continue,
            };
            let gw = gateway.of_channel(channel);
            match gw.query(otn) {
                Ok(PaymentState::SUCCESS) => {
                    conn.execute(
                        "UPDATE payment_record SET status='SUCCESS', paid_at=?1
                         WHERE order_no=?2 AND out_trade_no=?3",
                        rusqlite::params![now_str(), order_no, otn],
                    )?;
                }
                Ok(PaymentState::FAILED(_)) | Ok(PaymentState::CLOSED) => {
                    conn.execute(
                        "UPDATE payment_record SET status='CLOSED'
                         WHERE order_no=?1 AND out_trade_no=?2",
                        rusqlite::params![order_no, otn],
                    )?;
                }
                Ok(PaymentState::USERPAYING) | Err(_) => unresolved = true,
            }
        }

        // 查单后金额已覆盖 → 收口（不再回到作废分支）
        if payable > 0 {
            let now_confirmed: i64 = conn.query_row(
                "SELECT COALESCE(SUM(amount_cents),0) FROM payment_record
                 WHERE order_no=?1 AND status='SUCCESS'",
                [&order_no],
                |r| r.get(0),
            )?;
            if now_confirmed + sv_pending >= payable {
                finalize_order_assets(conn, &order_no)?;
                recovered += 1;
                continue;
            }
        }

        if unresolved {
            checkout_core::rbac::audit_operation(
                conn,
                "recovery_unresolved",
                None,
                None,
                serde_json::json!({"order_no": order_no}),
                &now_str(),
            )?;
            continue;
        }

        // 全部流水均已明确未支付 → 释放券并作废
        release_order_coupon(conn, &order_no)?;
        void_pending_order_system(conn, &order_no)?;
        recovered += 1;
    }
    Ok(recovered)
}

/// 系统级作废待支付订单（恢复扫描专用）：只允许未支付订单，
/// 由领域 void_order 统一守卫，不再按"是否当日"降级为直接改状态（绕过审计与库存约束）。
fn void_pending_order_system(conn: &Connection, order_no: &str) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    checkout_core::order::void_order(&tx, order_no)?;
    // 未完成流水一并关闭，避免 VOIDED 订单挂着 PENDING 流水被后续扫描反复处理。
    tx.execute(
        "UPDATE payment_record SET status='CLOSED'
         WHERE order_no=?1 AND status IN ('PENDING','USERPAYING')",
        [order_no],
    )?;
    checkout_core::rbac::audit_operation(
        &tx,
        "recovery_void_order",
        None,
        None,
        serde_json::json!({"order_no": order_no, "method": "void_order"}),
        &now_str(),
    )?;
    tx.commit()?;
    Ok(())
}

// ===== 员工管理（user.manage，仅 ADMIN）=====

#[derive(Debug, Serialize)]
pub struct UserRow {
    pub user_id: i64,
    pub username: String,
    pub role_name: String,
    pub status: i64,
}

#[tauri::command]
pub fn create_user(
    username: String,
    password: String,
    role_name: String,
    state: State<'_, AppState>,
) -> Result<i64> {
    let (_operator_id, conn) = admin_guard(&state, PERM_USER_MANAGE)?;
    if username.trim().is_empty() {
        return Err(CoreError::Validation("用户名不能为空".into()));
    }
    if !matches!(role_name.as_str(), ROLE_CASHIER | ROLE_MANAGER | ROLE_ADMIN) {
        return Err(CoreError::Validation(format!(
            "角色仅允许 {} / {} / {}",
            ROLE_CASHIER, ROLE_MANAGER, ROLE_ADMIN
        )));
    }
    checkout_core::rbac::validate_password_policy(&password)?;
    let hash = checkout_core::rbac::hash_password(&password)?;
    let role_id: i64 = conn.query_row(
        "SELECT role_id FROM role WHERE role_name=?1",
        [&role_name],
        |r| r.get(0),
    )?;
    conn.execute(
        "INSERT INTO user_account(username,password_hash,role_id) VALUES (?1,?2,?3)",
        rusqlite::params![username.trim(), hash, role_id],
    )?;
    Ok(conn.last_insert_rowid())
}

#[tauri::command]
pub fn list_users(state: State<'_, AppState>) -> Result<Vec<UserRow>> {
    let (_operator_id, conn) = admin_guard(&state, PERM_USER_MANAGE)?;
    let mut stmt = conn.prepare(
        "SELECT u.user_id, u.username, r.role_name, u.status
         FROM user_account u JOIN role r ON r.role_id=u.role_id ORDER BY u.user_id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(UserRow {
            user_id: r.get(0)?,
            username: r.get(1)?,
            role_name: r.get(2)?,
            status: r.get(3)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

// ===== 门店档案与行业（多行业适配）=====

/// 系统是否已建账号：决定门店配置类操作是否需要管理员权限。
fn system_has_users(conn: &Connection) -> Result<bool> {
    conn.query_row("SELECT EXISTS(SELECT 1 FROM user_account)", [], |r| {
        r.get(0)
    })
    .map_err(CoreError::from)
}

/// 门店配置读写连接：已建账号则走管理员门禁，首次部署（无账号）放行。
fn settings_conn(state: &AppState) -> Result<std::sync::MutexGuard<'_, Connection>> {
    let has_users = {
        let conn = state
            .conn
            .lock()
            .map_err(|e| CoreError::Other(e.to_string()))?;
        system_has_users(&conn)?
    };
    if has_users {
        let (_op, conn) = admin_guard(state, PERM_SYSTEM_CONFIG)?;
        Ok(conn)
    } else {
        Ok(state
            .conn
            .lock()
            .map_err(|e| CoreError::Other(e.to_string()))?)
    }
}

/// 门店档案：店名 + 行业。启动阶段（尚未登录）也要读取，故不做权限门禁。
#[tauri::command]
pub fn get_store_profile(state: State<'_, AppState>) -> Result<StoreProfile> {
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    checkout_core::industry::store_profile(&conn)
}

/// 行业选项列表（启动选择页）。
#[tauri::command]
pub fn list_industries() -> Result<Vec<IndustryOption>> {
    Ok(checkout_core::industry::list_industries())
}

/// 选定行业并落地该行业的分类模板。
#[tauri::command]
pub fn select_industry(industry: String, state: State<'_, AppState>) -> Result<StoreProfile> {
    let conn = settings_conn(&state)?;
    checkout_core::industry::apply_industry(&conn, &industry)?;
    checkout_core::industry::store_profile(&conn)
}

/// 设置店名（顶栏与小票抬头显示）。
#[tauri::command]
pub fn set_store_name(name: String, state: State<'_, AppState>) -> Result<StoreProfile> {
    let conn = settings_conn(&state)?;
    checkout_core::industry::set_store_name(&conn, &name)?;
    checkout_core::industry::store_profile(&conn)
}

// ===== 商品图片（本地上传 / 在线地址）=====

/// 商品图片允许的最大字节数：8MB，超出即拒绝，避免磁盘被撑爆。
const PRODUCT_IMAGE_MAX_BYTES: usize = 8 * 1024 * 1024;

/// 按文件头判定图片格式：只认 PNG / JPEG / WebP / GIF，
/// 不信任扩展名，杜绝把可执行文件当图片存盘。
fn sniff_image_ext(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some("png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpg")
    } else if bytes.len() > 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("gif")
    } else {
        None
    }
}

/// 上传商品图片：接收 base64 图片数据，落盘到应用数据目录并登记到 SKU。
/// 返回图片绝对路径（前端用 convertFileSrc 转成可展示地址）。
#[tauri::command]
pub fn upload_product_image(
    sku_id: i64,
    data_base64: String,
    state: State<'_, AppState>,
) -> Result<String> {
    let (_operator_id, conn) = admin_guard(&state, PERM_PRODUCT_EDIT)?;
    // 允许前端带 data URL 前缀，统一剥掉
    let raw = data_base64
        .split_once(',')
        .map(|(_, rest)| rest)
        .unwrap_or(data_base64.as_str());
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(raw.trim())
        .map_err(|e| CoreError::Validation(format!("图片数据无法解析: {e}")))?;
    if bytes.is_empty() {
        return Err(CoreError::Validation("图片内容为空".into()));
    }
    if bytes.len() > PRODUCT_IMAGE_MAX_BYTES {
        return Err(CoreError::Validation("图片超过 8MB，请压缩后再上传".into()));
    }
    let ext = sniff_image_ext(&bytes).ok_or_else(|| {
        CoreError::Validation("仅支持 PNG / JPEG / WebP / GIF 格式的商品图片".into())
    })?;

    let dir = state.data_dir.join("images");
    std::fs::create_dir_all(&dir)
        .map_err(|e| CoreError::Other(format!("创建图片目录失败: {e}")))?;
    let file_name = format!("sku{sku_id}_{}.{ext}", Utc::now().timestamp_millis());
    let path = dir.join(&file_name);
    std::fs::write(&path, &bytes).map_err(|e| CoreError::Other(format!("保存图片失败: {e}")))?;
    // 库内存文件名，读取时统一拼 data_dir/images 解析成绝对路径；
    // 存绝对路径会在安装目录变化/迁移后失效，也会让读取端重复拼接。
    checkout_core::product::set_product_image_path(&conn, sku_id, Some(file_name.as_str()))?;
    Ok(path.to_string_lossy().to_string())
}

/// 登记商品在线图片地址（http/https）。空字符串表示清除在线地址。
#[tauri::command]
pub fn set_product_image_url(sku_id: i64, url: String, state: State<'_, AppState>) -> Result<()> {
    let (_operator_id, conn) = admin_guard(&state, PERM_PRODUCT_EDIT)?;
    let u = url.trim();
    let u = if u.is_empty() { None } else { Some(u) };
    checkout_core::product::set_product_image_url(&conn, sku_id, u)
}

// ===== 订单查询（订单详情 / 退款检索）=====

/// 订单列表：按业务日过滤 + 单号模糊匹配。退款页与订单详情页共用。
#[tauri::command]
pub fn list_orders(
    date: Option<String>,
    keyword: String,
    limit: i64,
    state: State<'_, AppState>,
) -> Result<Vec<OrderSummary>> {
    let session = state
        .session
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    session.as_ref().ok_or(CoreError::NotAuthenticated)?;
    let _session = session;
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    let d = date.filter(|s| !s.trim().is_empty());
    checkout_core::orderview::list_orders(&conn, d.as_deref(), keyword.trim(), limit)
}

/// 订单详情：头 + 明细 + 支付流水 + 优惠 + 退货记录。
#[tauri::command]
pub fn get_order_detail(order_no: String, state: State<'_, AppState>) -> Result<OrderDetail> {
    let session = state
        .session
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    session.as_ref().ok_or(CoreError::NotAuthenticated)?;
    let _session = session;
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    checkout_core::orderview::order_detail(&conn, order_no.trim())?
        .ok_or_else(|| CoreError::Validation("订单不存在".into()))
}

// ===== 在线激活（§4.7 激活方式之二）=====

/// 解析生效的激活服务地址：先取用户配置，未配置则回退到构建时内置的地址。
/// 内置地址来自 build.rs 的 ACTIVATION_ENDPOINT 环境变量（发行包自带，无需手填）。
fn resolve_activation_endpoint(conn: &Connection) -> Result<String> {
    let configured = cfg_get(conn, CFG_ACTIVATION_ENDPOINT)
        .ok()
        .flatten()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let endpoint =
        configured.unwrap_or_else(|| checkout_core::default_activation_endpoint().to_string());
    if endpoint.trim().is_empty() {
        return Err(CoreError::Validation(
            "未配置在线激活服务地址：请在「设置 → 系统配置 → 在线激活服务」填写服务地址".into(),
        ));
    }
    checkout_core::license::online::normalize_endpoint(&endpoint)
}

/// 防线：任何构建都不得用公开的测试向量公钥激活（用它签发的许可任何人都能伪造）。
/// 正式公钥由 build.rs 固化内嵌，轮换时可用 CHECKOUT_PUBLIC_KEY 覆盖并强制校验指纹。
fn ensure_production_key() -> Result<()> {
    if checkout_core::EMBEDDED_KEY_IS_DEV {
        return Err(CoreError::Validation(
            "此安装包未内嵌正式授权公钥，不能激活；请使用正式发布版本重新构建".into(),
        ));
    }
    Ok(())
}

/// 在线激活：向激活服务换取签名激活码 → 本地验签 → 落库。
/// 签发权始终在服务端；客户端只做验签与存储，与离线导入共用同一条链路。
#[tauri::command]
pub fn activate_online(
    activation_key: String,
    customer_name: Option<String>,
    state: State<'_, AppState>,
) -> Result<LicenseSnapshot> {
    ensure_production_key()?;
    let key = activation_key.trim();
    if key.is_empty() {
        return Err(CoreError::Validation("请输入激活码".into()));
    }
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    let endpoint = resolve_activation_endpoint(&conn)?;
    let store_name = cfg_get(&conn, CFG_STORE_NAME)
        .ok()
        .flatten()
        .unwrap_or_default();
    let fingerprint = checkout_core::license::device_fingerprint().ok().flatten();
    let name = customer_name.unwrap_or_default().trim().to_string();
    let req = checkout_core::license::OnlineActivateRequest {
        store_id: if store_name.trim().is_empty() {
            "STORE".to_string()
        } else {
            store_name.trim().to_string()
        },
        customer_name: if name.is_empty() {
            store_name.trim().to_string()
        } else {
            name
        },
        device_fingerprint: fingerprint,
        activation_key: key.to_string(),
    };
    let resp = checkout_core::license::request_activation(&endpoint, &req)?;
    let svc = build_license_service(&state);
    // 保存激活码：后续可凭它联网刷新授权（撤销/过期以服务端为准）
    let snap = svc.activate_with_key(&conn, &resp.token, Some(key))?;
    *state
        .license
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))? = snap.clone();
    Ok(snap)
}

/// 在线刷新授权：用本地保存的激活码向激活服务幂等换票。
/// 服务端明确拒绝（撤销/过期）时同步降级本地状态并回传降级快照；
/// 网络不可达时保留本地缓存，前台收银继续可用。
#[tauri::command]
pub fn refresh_license_online(state: State<'_, AppState>) -> Result<LicenseSnapshot> {
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    let endpoint = resolve_activation_endpoint(&conn)?;
    let store_name = cfg_get(&conn, CFG_STORE_NAME)
        .ok()
        .flatten()
        .unwrap_or_default();
    let svc = build_license_service(&state);
    let snap = match svc.refresh_online(&conn, &endpoint, &store_name, &store_name) {
        Ok(s) => s,
        Err(e)
            if matches!(
                e,
                CoreError::License(checkout_core::error::LicenseError::OnlineRejected(_))
            ) =>
        {
            svc.mark_revoked(&conn, &e.to_string())?
        }
        Err(e) => return Err(e),
    };
    *state
        .license
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))? = snap.clone();
    Ok(snap)
}

/// 探测激活服务可达性：前端在激活页用于提示「服务可达 / 不可达」。
#[tauri::command]
pub fn probe_activation_service(state: State<'_, AppState>) -> Result<bool> {
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    let endpoint = resolve_activation_endpoint(&conn)?;
    checkout_core::license::online::probe_service(&endpoint)
}

/// 获取激活码：用系统默认浏览器打开官网（与激活服务同源）。
/// 前端不接触、不展示具体地址，避免在界面暴露服务部署位置。
#[tauri::command]
pub fn open_activation_site(state: State<'_, AppState>) -> Result<()> {
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    let endpoint = resolve_activation_endpoint(&conn)?;
    open_in_browser(&endpoint)
}

/// 跨平台用系统默认浏览器打开 URL。仅接受经 normalize_endpoint 校验过的 http/https 地址。
fn open_in_browser(url: &str) -> Result<()> {
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .spawn()
        .map(|_| ());
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open")
        .arg(url)
        .spawn()
        .map(|_| ());
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let result = std::process::Command::new("xdg-open")
        .arg(url)
        .spawn()
        .map(|_| ());
    result.map_err(|e| CoreError::Other(format!("打开官网失败：{e}")))
}

#[tauri::command]
pub fn set_user_status(user_id: i64, status: i64, state: State<'_, AppState>) -> Result<()> {
    let (_operator_id, conn) = admin_guard(&state, PERM_USER_MANAGE)?;
    if status != 0 && status != 1 {
        return Err(CoreError::Validation("status 仅允许 0/1".into()));
    }
    let n = conn.execute(
        "UPDATE user_account SET status=?1 WHERE user_id=?2",
        rusqlite::params![status, user_id],
    )?;
    if n == 0 {
        return Err(CoreError::Validation(format!("用户 {user_id} 不存在")));
    }
    Ok(())
}

/// 启动应用（Tauri 2）。
// ─────────────────── 行业业务模型扩展命令（M0020） ───────────────────

#[tauri::command]
pub fn create_spec_group(
    spu_id: i64,
    name: String,
    state: State<'_, AppState>,
) -> Result<i64> {
    let (_op, conn) = admin_guard(&state, PERM_PRODUCT_EDIT)?;
    checkout_core::catalog_ext::create_spec_group(&conn, spu_id, name)
}

#[tauri::command]
pub fn add_spec_value(
    spec_group_id: i64,
    value: String,
    state: State<'_, AppState>,
) -> Result<i64> {
    let (_op, conn) = admin_guard(&state, PERM_PRODUCT_EDIT)?;
    checkout_core::catalog_ext::add_spec_value(&conn, spec_group_id, value)
}

#[tauri::command]
pub fn list_spec_groups(
    spu_id: i64,
    state: State<'_, AppState>,
) -> Result<Vec<checkout_core::catalog_ext::SpecGroup>> {
    let (_op, conn) = admin_guard(&state, PERM_POS_CHECKOUT)?;
    checkout_core::catalog_ext::list_spec_groups(&conn, spu_id)
}

#[tauri::command]
pub fn create_sku_variants(
    spu_id: i64,
    variants: Vec<checkout_core::catalog_ext::VariantInput>,
    state: State<'_, AppState>,
) -> Result<Vec<i64>> {
    let (_op, conn) = admin_guard(&state, PERM_PRODUCT_EDIT)?;
    checkout_core::catalog_ext::create_sku_variants(&conn, spu_id, &variants)
}

#[tauri::command]
pub fn list_sku_variants(
    spu_id: i64,
    state: State<'_, AppState>,
) -> Result<Vec<checkout_core::catalog_ext::SkuVariant>> {
    let (_op, conn) = admin_guard(&state, PERM_POS_CHECKOUT)?;
    checkout_core::catalog_ext::list_sku_variants(&conn, spu_id)
}

#[tauri::command]
pub fn set_product_age_range(
    sku_id: i64,
    month_min: Option<i64>,
    month_max: Option<i64>,
    shelf_life_days: i64,
    state: State<'_, AppState>,
) -> Result<()> {
    let (_op, conn) = admin_guard(&state, PERM_PRODUCT_EDIT)?;
    checkout_core::catalog_ext::set_product_age_range(
        &conn,
        sku_id,
        month_min,
        month_max,
        shelf_life_days,
    )
}

#[tauri::command]
pub fn create_sku_batch(
    sku_id: i64,
    batch_no: String,
    production_date: Option<String>,
    expiry_date: Option<String>,
    qty: i64,
    state: State<'_, AppState>,
) -> Result<i64> {
    let (_op, conn) = admin_guard(&state, PERM_PRODUCT_EDIT)?;
    checkout_core::catalog_ext::create_sku_batch(
        &conn,
        sku_id,
        &batch_no,
        production_date.as_deref(),
        expiry_date.as_deref(),
        qty,
    )
}

#[tauri::command]
pub fn list_sku_batches(
    sku_id: i64,
    state: State<'_, AppState>,
) -> Result<Vec<checkout_core::catalog_ext::SkuBatch>> {
    let (_op, conn) = admin_guard(&state, PERM_POS_CHECKOUT)?;
    checkout_core::catalog_ext::list_sku_batches(&conn, sku_id)
}

#[tauri::command]
pub fn list_products_by_age(
    month: i64,
    limit: i64,
    state: State<'_, AppState>,
) -> Result<Vec<checkout_core::catalog_ext::AgeProduct>> {
    let (_op, conn) = admin_guard(&state, PERM_POS_CHECKOUT)?;
    checkout_core::catalog_ext::list_products_by_age(&conn, month, limit)
}

#[tauri::command]
pub fn create_combo(
    sku_id: i64,
    note: Option<String>,
    state: State<'_, AppState>,
) -> Result<i64> {
    let (_op, conn) = admin_guard(&state, PERM_PRODUCT_EDIT)?;
    checkout_core::catalog_ext::create_combo(&conn, sku_id, note.as_deref())
}

#[tauri::command]
pub fn set_combo_items(
    combo_id: i64,
    items: Vec<checkout_core::catalog_ext::ComboItemInput>,
    state: State<'_, AppState>,
) -> Result<()> {
    let (_op, conn) = admin_guard(&state, PERM_PRODUCT_EDIT)?;
    checkout_core::catalog_ext::set_combo_items(&conn, combo_id, &items)
}

#[tauri::command]
pub fn list_combos(state: State<'_, AppState>) -> Result<Vec<checkout_core::catalog_ext::Combo>> {
    let (_op, conn) = admin_guard(&state, PERM_POS_CHECKOUT)?;
    checkout_core::catalog_ext::list_combos(&conn)
}

#[tauri::command]
pub fn combo_items_by_sku(
    sku_id: i64,
    state: State<'_, AppState>,
) -> Result<Vec<checkout_core::catalog_ext::ComboItem>> {
    let (_op, conn) = admin_guard(&state, PERM_POS_CHECKOUT)?;
    checkout_core::catalog_ext::combo_items_by_sku(&conn, sku_id)
}

#[tauri::command]
pub fn create_option_group(
    name: String,
    scene: String,
    state: State<'_, AppState>,
) -> Result<i64> {
    let (_op, conn) = admin_guard(&state, PERM_PRODUCT_EDIT)?;
    checkout_core::catalog_ext::create_option_group(&conn, &name, &scene)
}

#[tauri::command]
pub fn set_option_items(
    group_id: i64,
    sku_ids: Vec<i64>,
    state: State<'_, AppState>,
) -> Result<()> {
    let (_op, conn) = admin_guard(&state, PERM_PRODUCT_EDIT)?;
    checkout_core::catalog_ext::set_option_items(&conn, group_id, &sku_ids)
}

#[tauri::command]
pub fn list_option_groups(
    scene: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<checkout_core::catalog_ext::OptionGroup>> {
    let (_op, conn) = admin_guard(&state, PERM_POS_CHECKOUT)?;
    checkout_core::catalog_ext::list_option_groups(&conn, scene.as_deref())
}

#[tauri::command]
pub fn assign_bell(
    technician_id: i64,
    table_id: i64,
    business_date: String,
    minutes: i64,
    is_pointed: bool,
    state: State<'_, AppState>,
) -> Result<i64> {
    let (_op, conn) = admin_guard(&state, PERM_POS_CHECKOUT)?;
    checkout_core::technician::assign_bell(
        &conn,
        technician_id,
        table_id,
        &business_date,
        minutes,
        is_pointed,
    )
}

#[tauri::command]
pub fn auto_assign_bell(
    table_id: i64,
    business_date: String,
    minutes: i64,
    state: State<'_, AppState>,
) -> Result<i64> {
    let (_op, conn) = admin_guard(&state, PERM_POS_CHECKOUT)?;
    checkout_core::technician::auto_assign_bell(&conn, table_id, &business_date, minutes)
}

#[tauri::command]
pub fn add_bell_minutes(
    bell_id: i64,
    extra_minutes: i64,
    state: State<'_, AppState>,
) -> Result<()> {
    let (_op, conn) = admin_guard(&state, PERM_POS_CHECKOUT)?;
    checkout_core::technician::add_bell_minutes(&conn, bell_id, extra_minutes)
}

#[tauri::command]
pub fn finish_bell(bell_id: i64, state: State<'_, AppState>) -> Result<()> {
    let (_op, conn) = admin_guard(&state, PERM_POS_CHECKOUT)?;
    checkout_core::technician::finish_bell(&conn, bell_id)
}

#[tauri::command]
pub fn list_bells(
    business_date: String,
    status: Option<String>,
    state: State<'_, AppState>,
) -> Result<Vec<checkout_core::technician::TechnicianBell>> {
    let (_op, conn) = admin_guard(&state, PERM_POS_CHECKOUT)?;
    checkout_core::technician::list_bells(&conn, &business_date, status.as_deref())
}

#[tauri::command]
pub fn active_bell_for_table(
    table_id: i64,
    state: State<'_, AppState>,
) -> Result<Option<checkout_core::technician::TechnicianBell>> {
    let (_op, conn) = admin_guard(&state, PERM_POS_CHECKOUT)?;
    checkout_core::technician::active_bell_for_table(&conn, table_id)
}

#[tauri::command]
pub fn set_service_table_fee_cap(
    table_id: i64,
    cap_cents: i64,
    state: State<'_, AppState>,
) -> Result<()> {
    let (_op, conn) = admin_guard(&state, PERM_PRODUCT_EDIT)?;
    checkout_core::service::set_table_fee_cap(&conn, table_id, cap_cents)
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            #[cfg(target_os = "android")]
            checkout_core::paycreds::init_store_dir(data_dir.clone())?;
            let db_path = data_dir.join("checkoutr.db");
            let conn = init_db(&db_path)?;

            // 支付网关装配：凭据已配置 → 真实渠道；未配置/加载失败 → 本地确认兜底。
            // 凭据读取失败不阻断启动（生存能力 §1.2），但落启动日志。
            let store = checkout_core::paycreds::KeyringStore;
            let (wechat_gw, alipay_gw) = {
                let w = checkout_core::paycreds::load_wechat(&store)
                    .ok()
                    .flatten()
                    .and_then(|c| {
                        match checkout_core::gateway_wechat::WechatGateway::from_cred(&c) {
                            Ok(g) => Some(g),
                            Err(e) => {
                                tracing::warn!("微信网关构造失败，回落本地确认: {e}");
                                None
                            }
                        }
                    });
                let a = checkout_core::paycreds::load_alipay(&store)
                    .ok()
                    .flatten()
                    .and_then(|c| {
                        match checkout_core::gateway_alipay::AlipayGateway::from_cred(&c) {
                            Ok(g) => Some(g),
                            Err(e) => {
                                tracing::warn!("支付宝网关构造失败，回落本地确认: {e}");
                                None
                            }
                        }
                    });
                (w, a)
            };
            // 本地模拟收款仅限 debug 联调构建；release 无条件禁用。
            // 即使发行环境配置文件被写入 pay.allow_local_simulation=1，也不能重开
            // "任意付款码即记 SUCCESS"的后门——真实营业缺凭据时应直接拒绝扫码收款。
            // release 下该配置仅用于可观测性告警，不再改变网关行为。
            let requested_local_sim = cfg_get(&conn, CFG_ALLOW_LOCAL_SIMULATION)
                .ok()
                .flatten()
                .as_deref()
                == Some("1");
            let allow_local_simulation = cfg!(debug_assertions);
            if allow_local_simulation {
                tracing::warn!("本地模拟支付网关已启用（仅 debug 联调构建允许）");
            } else if requested_local_sim {
                tracing::warn!(
                    "检测到 {CFG_ALLOW_LOCAL_SIMULATION}=1，但 release 构建禁止本地模拟收款，已忽略"
                );
            }
            let gateway = checkout_core::payment::RoutingGateway::new(
                wechat_gw,
                alipay_gw,
                allow_local_simulation,
            );

            // 崩溃恢复扫描（§23）：PENDING_PAYMENT 订单向渠道查单收敛
            match recover_pending_orders(&conn, &gateway) {
                Ok(n) if n > 0 => tracing::warn!("崩溃恢复：处理待支付订单 {n} 单"),
                Ok(_) => {}
                Err(e) => tracing::error!("崩溃恢复扫描失败: {e}"),
            }

            app.manage(AppState {
                conn: std::sync::Mutex::new(conn),
                license: std::sync::Mutex::new(LicenseSnapshot::unactivated()),
                session: std::sync::Mutex::new(None::<Session>),
                license_path: data_dir.join("license.enc"),
                data_dir: data_dir.clone(),
                gateway,
                current_shift: std::sync::Mutex::new(0i64),
                pending_sensitive: std::sync::Mutex::new(Vec::new()),
            });

            // Android 版无系统托盘与窗口关闭拦截：移动端由系统返回键管理生命周期。
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            init_admin,
            has_users,
            get_license,
            activate,
            renew_license,
            login,
            logout,
            change_password,
            current_session,
            lookup_product,
            list_pos_products,
            list_pos_categories,
            list_service_tables,
            create_service_table,
            delete_service_table,
            open_service_table,
            rebind_service_table_member,
            save_service_table_cart,
            submit_service_table_order,
            return_service_table_line,
            merge_service_tables,
            transfer_service_table,
            chess_table_fee,
            print_kitchen_ticket,
            get_kitchen_routes,
            set_kitchen_routes,
            create_technician,
            set_technician_rule,
            delete_technician_rule,
            list_technician_rules,
            set_technician_shift,
            list_technicians,
            technician_commission,
            close_service_table,
            member_prices,
            parse_weigh_barcode,
            detect_payment_code,
            lookup_weigh,
            quote_order,
            checkout,
            park_cart,
            list_parked,
            resume_cart,
            daily_report,
            channel_report,
            product_report,
            category_report,
            cashier_report,
            report_catalog,
            generic_report,
            export_report_csv,
            reconcile_csv,
            print_receipt,
            get_print_auto,
            get_receipt_template,
            save_receipt_template,
            preview_receipt,
            test_print,
            print_retries,
            retry_print,
            shift_report,
            open_shift,
            current_shift,
            close_shift,
            request_sensitive_auth,
            get_configs,
            set_config,
            list_coupons,
            issue_coupon,
            create_product,
            list_products,
            update_product,
            update_product_profile,
            set_product_service,
            set_product_status,
            list_categories,
            create_category,
            update_category,
            delete_category,
            adjust_stock,
            create_supplier,
            list_suppliers,
            create_purchase_order,
            list_purchase_orders,
            get_purchase_items,
            receive_purchase,
            list_stocktakes,
            get_stocktake_items,
            create_stocktake,
            record_count,
            approve_stocktake,
            create_member,
            list_members,
            recharge_member,
            list_promotions,
            create_promotion,
            set_promotion_status,
            void_order_cmd,
            get_order_items,
            refund_order,
            poll_native_pay,
            cancel_native_pay,
            confirm_native_pay,
            set_payment_credential,
            delete_payment_credential,
            payment_gateway_status,
            get_pay_channels,
            import_wechat_bill,
            import_alipay_bill,
            list_reconcile_diffs,
            qr_svg,
            create_user,
            list_users,
            set_user_status,
            get_store_profile,
            list_industries,
            select_industry,
            set_store_name,
            upload_product_image,
            set_product_image_url,
            list_orders,
            get_order_detail,
            preview_refund,
            activate_online,
            refresh_license_online,
            probe_activation_service,
            open_activation_site,
            create_spec_group,
            add_spec_value,
            list_spec_groups,
            create_sku_variants,
            list_sku_variants,
            set_product_age_range,
            create_sku_batch,
            list_sku_batches,
            list_products_by_age,
            create_combo,
            set_combo_items,
            list_combos,
            combo_items_by_sku,
            create_option_group,
            set_option_items,
            list_option_groups,
            assign_bell,
            auto_assign_bell,
            add_bell_minutes,
            finish_bell,
            list_bells,
            active_bell_for_table,
            set_service_table_fee_cap,
        ])
        .run(tauri::generate_context!())
        .expect("应用启动失败");
}
