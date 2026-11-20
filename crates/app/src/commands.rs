//! Tauri 命令层。所有命令在此注册；capabilities 仅暴露此清单。
//! 门禁统一入口按 §12.2 顺序执行。

use crate::state::{build_license_service, init_db, AppState, PendingSensitive};
use checkout_core::constants::{
    CFG_ACTIVATION_ENDPOINT, CFG_ALLOW_NEGATIVE_STOCK, CFG_BUSINESS_DAY_CUTOFF,
    CFG_MAX_DISCOUNT_RATIO, CFG_MICROPAY_POLL_MS, CFG_MICROPAY_TIMEOUT_S, CFG_PAY_CHANNEL_ALIPAY,
    CFG_PAY_CHANNEL_CASH, CFG_PAY_CHANNEL_SV, CFG_PAY_CHANNEL_WECHAT, CFG_POINTS_PER_YUAN,
    CFG_PRINT_AUTO, CFG_PRINT_HOST, CFG_PRINT_PORT, CFG_SCAN_GAP_MS, CFG_STORE_NAME, COUPON_UNUSED,
    DEFAULT_MAX_DISCOUNT_RATIO, DEFAULT_MICROPAY_POLL_MS, DEFAULT_MICROPAY_TIMEOUT_S,
    DEFAULT_POINTS_PER_YUAN, DEFAULT_PRINT_PORT, ENT_MEMBER_BASIC, ENT_POS_CORE,
    ENT_REPORT_STANDARD, LIMIT_REFUND_MAX_CENTS, PAY_ALIPAY, PAY_CASH, PAY_MODE_MICROPAY,
    PAY_MODE_NATIVE, PAY_STORED_VALUE, PAY_WECHAT, PERM_INVENTORY_ADJUST, PERM_POS_REFUND,
    PERM_POS_VOID, PERM_PRODUCT_EDIT, PERM_REPORT_EXPORT, PERM_REPORT_VIEW, PERM_STOCKTAKE_APPROVE,
    PERM_SYSTEM_CONFIG, PERM_USER_MANAGE, ROLE_ADMIN, ROLE_CASHIER, ROLE_MANAGER,
};
use checkout_core::error::{CoreError, Result};
use checkout_core::industry::{IndustryOption, StoreProfile};
use checkout_core::license::{LicenseSnapshot, LicenseStatus};
use checkout_core::orderview::{OrderDetail, OrderSummary};
use checkout_core::paycreds::SecretStore;
use checkout_core::payment::{
    run_micropay, Channel as PayChannel, PayMode, PayRequest, PaymentGateway, PaymentState,
};
use checkout_core::promo::{Promotion, Scope as PromoScope};
use checkout_core::rbac::{self, Session};
use checkout_core::time::parse_rfc3339;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension};
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

/// 读取 system_config f64 值，缺失/非法回退到默认值。
fn cfg_f64(conn: &Connection, key: &str, default: f64) -> f64 {
    cfg_get(conn, key)
        .ok()
        .flatten()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(default)
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

/// 后台管理命令统一门禁（§12.2 顺序）：授权状态（entitlement=report.standard，
/// 降级态后台受限）→ 会话 → RBAC 权限。返回 (operator_id, conn)；
/// license/session 锁在返回前释放，调用方继续持有 conn 做业务写入。
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
                entitlement: ENT_REPORT_STANDARD,
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
    drop(session);
    drop(license);
    Ok((operator_id, conn))
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

#[derive(Debug, Serialize, Deserialize)]
pub struct ProductHit {
    pub sku_id: i64,
    pub spu_name: String,
    pub barcode: String,
    pub price_cents: i64,
    /// 会员价：识别会员后按此价展示（结算时由后端与促销取优）。
    pub member_price_cents: Option<i64>,
    pub is_weighed: bool,
}

/// 商品识别（F-01）：条码精确匹配，模糊搜索兜底。停用商品不参与识别。
#[tauri::command]
pub fn lookup_product(keyword: String, state: State<'_, AppState>) -> Result<Vec<ProductHit>> {
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    let pattern = format!("%{keyword}%");
    let mut stmt = conn.prepare(
        "SELECT s.sku_id, p.spu_name, COALESCE(s.barcode,''), s.sale_price_cents, s.is_weighed,
                s.member_price_cents
         FROM sku s JOIN spu p ON p.spu_id=s.spu_id
         WHERE p.status=1 AND (s.barcode LIKE ?1 OR s.instore_code LIKE ?1 OR p.spu_name LIKE ?1)
         ORDER BY (s.barcode = ?2) DESC LIMIT 20",
    )?;
    use rusqlite::params;
    let rows = stmt.query_map(params![pattern, keyword], |r| {
        Ok(ProductHit {
            sku_id: r.get(0)?,
            spu_name: r.get(1)?,
            barcode: r.get(2)?,
            price_cents: r.get(3)?,
            is_weighed: r.get::<_, i64>(4)? != 0,
            member_price_cents: r.get(5)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// 批量取 SKU 会员价：识别会员后刷新购物车行的会员价（前台按会员价展示）。
#[derive(Debug, Serialize)]
pub struct SkuMemberPrice {
    pub sku_id: i64,
    pub member_price_cents: Option<i64>,
}

#[tauri::command]
pub fn member_prices(sku_ids: Vec<i64>, state: State<'_, AppState>) -> Result<Vec<SkuMemberPrice>> {
    let session = state
        .session
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    session.as_ref().ok_or(CoreError::NotAuthenticated)?;
    drop(session);
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
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
        let cond: serde_json::Value =
            serde_json::from_str(&r.get::<_, String>(5)?).unwrap_or(serde_json::json!({}));
        let act: serde_json::Value =
            serde_json::from_str(&r.get::<_, String>(6)?).unwrap_or(serde_json::json!({}));
        Ok(Promotion {
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
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
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
    let slot = pending.as_ref().ok_or(CoreError::SensitiveAuthRequired)?;
    if slot.permission != permission || slot.auth.nonce != nonce {
        return Err(CoreError::SensitiveAuthRequired);
    }
    let auth = slot.auth.clone();
    if auth.expires_at <= now {
        *pending = None;
        return Err(CoreError::SensitiveAuthRequired);
    }
    *pending = None; // 一次性消费
    Ok(auth)
}

#[derive(Debug, Deserialize)]
pub struct CartLineInput {
    pub sku_id: i64,
    pub qty: i64,
    pub unit_price_cents: i64,
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
}

/// 订单金额试算结果（只读）。原价 / 优惠 / 应付 + 券可用性提示。
struct AmountPlan {
    cart_lines: Vec<checkout_core::promo::CartLine>,
    applied: Vec<checkout_core::promo::AppliedPromo>,
    total: i64,
    discount: i64,
    payable: i64,
    /// 券不可用等提示（不参与金额试算，界面据此提示）。
    coupon_note: Option<String>,
}

/// 计算订单金额（只读，不改任何状态）。
/// 报价接口与 checkout 共用此函数，保证「界面显示的应收」与「实际收款口径」完全一致；
/// 否则前端只按会员价相加会漏掉促销与券，收款金额对不上账。
fn plan_amounts(
    conn: &Connection,
    lines: &[CartLineInput],
    member_id: Option<i64>,
    coupon_code: Option<&str>,
    now: &str,
) -> Result<AmountPlan> {
    // 1. 行 + 会员价装载（§8.2 会员价参与单品取优）
    let mut cart_lines: Vec<checkout_core::promo::CartLine> = Vec::with_capacity(lines.len());
    for l in lines {
        let mut cl = checkout_core::promo::CartLine::new(l.sku_id, l.qty, l.unit_price_cents)?;
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
    if let Some(code) = coupon_code.map(str::trim).filter(|c| !c.is_empty()) {
        let coupon: Option<(i64, i64, String, Option<String>, Option<String>)> = conn
            .query_row(
                "SELECT discount_cents,min_order_cents,status,valid_from,valid_until
                 FROM coupon WHERE coupon_code=?1",
                [code],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .optional()?;
        match coupon {
            None => coupon_note = Some("优惠券不存在".into()),
            Some((amount, min_order, status, valid_from, valid_until)) => {
                let expired = valid_until.as_deref().map(|v| v < now).unwrap_or(false);
                let not_started = valid_from.as_deref().map(|v| v > now).unwrap_or(false);
                if status != COUPON_UNUSED {
                    coupon_note = Some("优惠券不可用".into());
                } else if expired {
                    coupon_note = Some("优惠券已过期".into());
                } else if not_started {
                    coupon_note = Some("优惠券尚未生效".into());
                } else {
                    let after_promo: i64 = cart_lines.iter().map(|l| l.payable()).sum();
                    if after_promo < min_order {
                        coupon_note = Some("订单金额未达用券门槛".into());
                    } else {
                        coupon_discount = amount.min(after_promo);
                    }
                }
            }
        }
    }
    let total: i64 = cart_lines.iter().map(|l| l.line_total_cents).sum();
    let discount: i64 = promo_result.total_discount_cents + coupon_discount;
    Ok(AmountPlan {
        applied: promo_result.applied,
        cart_lines,
        total,
        discount,
        payable: total - discount,
        coupon_note,
    })
}

/// 收银台报价：返回后端口径的应收金额，界面「合计/待收」以此为准。
#[derive(Debug, Serialize)]
pub struct OrderQuote {
    pub total_cents: i64,
    pub discount_cents: i64,
    pub payable_cents: i64,
    /// 券不可用等提示（不阻断报价）。
    pub coupon_note: Option<String>,
}

#[tauri::command]
pub fn quote_order(
    lines: Vec<CartLineInput>,
    member_id: Option<i64>,
    coupon_code: Option<String>,
    state: State<'_, AppState>,
) -> Result<OrderQuote> {
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    let plan = plan_amounts(&conn, &lines, member_id, coupon_code.as_deref(), &now_str())?;
    Ok(OrderQuote {
        total_cents: plan.total,
        discount_cents: plan.discount,
        payable_cents: plan.payable,
        coupon_note: plan.coupon_note,
    })
}

/// 收银结算（F-05/F-06）。现金本地闭环；储值走领域扣款；扫码渠道走网关状态机。
/// 促销引擎计算行/整单优惠并落 order_promotion；tx A → 支付 → tx B。
/// 授权门禁只校验 entitlement，任何非 UNACTIVATED 状态不阻断 pos.core。
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn checkout(
    lines: Vec<CartLineInput>,
    payments: Vec<PaymentInput>,
    member_id: Option<i64>,
    coupon_code: Option<String>,
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
    if session.expires_at < now_dt {
        return Err(CoreError::SessionExpired);
    }
    let cashier_id = session.user_id;
    drop(session);
    drop(license);

    if lines.is_empty() {
        return Err(CoreError::Validation("购物车为空".into()));
    }
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;

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
    let plan = plan_amounts(&conn, &lines, member_id, coupon_code.as_deref(), &now)?;
    if let Some(note) = &plan.coupon_note {
        return Err(CoreError::Validation(note.clone()));
    }
    // 券核销（§8.3 第 3 层，§13.7 状态机）：试算已校验可用性，此处只做状态迁移
    if let Some(code) = coupon_key {
        checkout_core::member::transition_coupon(&conn, code, "LOCKED")?;
    }
    let cart_lines = plan.cart_lines;
    let total = plan.total;
    let discount = plan.discount;
    let payable = plan.payable;
    if payable < 0 {
        return Err(CoreError::Validation("优惠后应付金额为负".into()));
    }
    let paid: i64 = payments.iter().map(|p| p.amount_cents).sum();
    if paid < payable {
        return Err(CoreError::Validation(format!(
            "支付金额不足: 应付 {payable}, 实付 {paid}"
        )));
    }
    if paid > payable {
        // 多收的钱没有对应科目，直接吞掉会与应收对不上账。
        return Err(CoreError::Validation(format!(
            "支付金额超出应收: 应付 {payable}, 实付 {paid}"
        )));
    }

    let order_no = checkout_core::order::generate_order_no("STORE", "POS1");
    let cutoff = business_cutoff_hour(&conn);
    let biz = checkout_core::order::business_date(&now_dt, cutoff);
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

    // 支付执行。领域扣款函数自带事务，须在普通 conn 上调用；
    // 流水状态更新走单一事务。
    let mut native_code_url: Option<String> = None;
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
                let d = checkout_core::member::deduct(&conn, mid, &order_no, net)?;
                conn.execute(
                    "UPDATE payment_record SET status='SUCCESS', sv_bonus_cents=?1, sv_principal_cents=?2, paid_at=?3
                     WHERE order_no=?4 AND out_trade_no=?5",
                    rusqlite::params![d.bonus_cents, d.principal_cents, now, order_no, out_trade_no],
                )?;
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
                let poll =
                    cfg_i64(&conn, CFG_MICROPAY_POLL_MS, DEFAULT_MICROPAY_POLL_MS as i64) as u64;
                let timeout = cfg_i64(
                    &conn,
                    CFG_MICROPAY_TIMEOUT_S,
                    DEFAULT_MICROPAY_TIMEOUT_S as i64,
                ) as u64;
                let out = run_micropay(&state.gateway, &req, poll, timeout, &mut |_ms| {})?;
                match out.state {
                    PaymentState::SUCCESS => {
                        conn.execute(
                            "UPDATE payment_record SET status='SUCCESS', trade_no=?1, paid_at=?2
                             WHERE order_no=?3 AND out_trade_no=?4",
                            rusqlite::params![out.trade_no, now, order_no, out_trade_no],
                        )?;
                    }
                    other => {
                        // 流水保持 PENDING → 崩溃恢复可向渠道查单收敛；
                        // 明确失败/关闭由恢复扫描或操作员重试处理
                        let _ = conn.execute(
                            "UPDATE payment_record SET status='CLOSED' WHERE order_no=?1 AND out_trade_no=?2",
                            rusqlite::params![order_no, out_trade_no],
                        );
                        return Err(CoreError::Validation(format!("扫码支付未完成: {other:?}")));
                    }
                }
            }
            PayChannel::CASH => {}
        }
    }

    if has_native {
        // 券保持 LOCKED，等 finalize 收敛为 REDEEMED（取消则回 UNUSED）
        return Ok(CheckoutResult {
            order_no,
            status: "PENDING_PAYMENT".into(),
            code_url: native_code_url,
        });
    }

    finalize_order_assets(&conn, &order_no)?;
    auto_print_receipt(&conn, &order_no);

    Ok(CheckoutResult {
        order_no,
        status: "COMPLETED".into(),
        code_url: None,
    })
}

/// 订单资产收口（§13.3/§13.4/§13.7 + 订单提交）：在全部渠道支付确认后执行。
/// - PENDING 储值流水补扣款（外部先行、储值后扣）
/// - 积分累积（赠送金抵扣部分不计分）
/// - 券 UNUSED→REDEEMED
/// - commit_order（库存扣减 + 订单完成）
fn finalize_order_assets(conn: &Connection, order_no: &str) -> Result<()> {
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
        // 幂等：已完成的订单直接返回
        return Ok(());
    }
    let now = now_str();

    // PENDING 储值流水补扣（领域函数自带事务）
    let pending_sv: Vec<(i64, i64, String)> = {
        let mut stmt = conn.prepare(
            "SELECT pay_id, amount_cents, out_trade_no FROM payment_record
             WHERE order_no=?1 AND pay_channel='STORED_VALUE' AND status='PENDING'",
        )?;
        let rows = stmt.query_map([order_no], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()?
    };
    let mut bonus_used: i64 = 0;
    for (_pay_id, amount, out_trade_no) in pending_sv {
        let mid = member_id.ok_or_else(|| CoreError::Validation("储值支付需先识别会员".into()))?;
        let d = checkout_core::member::deduct(conn, mid, order_no, amount)?;
        bonus_used += d.bonus_cents;
        conn.execute(
            "UPDATE payment_record SET status='SUCCESS', sv_bonus_cents=?1, sv_principal_cents=?2, paid_at=?3
             WHERE order_no=?4 AND out_trade_no=?5",
            rusqlite::params![d.bonus_cents, d.principal_cents, now, order_no, out_trade_no],
        )?;
    }

    // 积分累积（§13.4）
    if let Some(mid) = member_id {
        let per_yuan = cfg_i64(conn, CFG_POINTS_PER_YUAN, DEFAULT_POINTS_PER_YUAN);
        let payable = total - discount;
        checkout_core::member::accrue_points(conn, mid, order_no, payable - bonus_used, per_yuan)?;
    }

    // 券 UNUSED→REDEEMED + 订单提交
    let tx = conn.unchecked_transaction()?;
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
    Ok(())
}

#[tauri::command]
pub fn park_cart(
    cart_json: String,
    remark: Option<String>,
    state: State<'_, AppState>,
) -> Result<i64> {
    let session = state
        .session
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    let cashier = session
        .as_ref()
        .map(|s| s.user_id)
        .ok_or(CoreError::NotAuthenticated)?;
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
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
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
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
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
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
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
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
pub fn print_receipt(order_no: String, state: State<'_, AppState>) -> Result<String> {
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    print_receipt_inner(&conn, order_no.trim())
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
    let (total, discount, pay, created, member_id): (i64, i64, i64, String, Option<i64>) = conn
        .query_row(
            "SELECT total_cents,discount_cents,pay_cents,created_at,member_id
             FROM sales_order WHERE order_no=?1",
            [order_no],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
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
    vars.insert("store_phone".into(), "000-0000".into());
    vars.insert("order_no".into(), order_no.to_string());
    vars.insert("created_at".into(), created);
    vars.insert("member_phone".into(), member_phone);
    vars.insert("subtotal".into(), cents_to_yuan(total));
    vars.insert("discount_total".into(), cents_to_yuan(discount));
    vars.insert("pay_total".into(), cents_to_yuan(pay));
    vars.insert("pay_channel_lines".into(), channel_lines.join("\n"));
    vars.insert("items".into(), items.join("\n"));
    vars.insert("change".into(), cents_to_yuan(change_total));
    vars.insert(
        "qrcode_pay_url".into(),
        format!("checkout-r://order/{order_no}"),
    );
    let bytes = match render_receipt(&template, &vars) {
        Ok(b) => b,
        Err(e) => {
            enqueue_retry(conn, order_no, "receipt_default", &e.to_string())?;
            return Ok(format!("小票生成失败，已入重打队列: {e}"));
        }
    };
    // 送到网络打印机（IP:9100）；未配置地址直接提示，发送失败才入重打队列
    let host = cfg_get(conn, CFG_PRINT_HOST)?.unwrap_or_default();
    if host.trim().is_empty() {
        return Ok("未配置打印机地址，无法打印（请在设置中填写打印机 IP）".into());
    }
    let port = cfg_get(conn, CFG_PRINT_PORT)?
        .and_then(|s| s.trim().parse::<u16>().ok())
        .unwrap_or(DEFAULT_PRINT_PORT);
    match checkout_core::printer::send_bytes(&host, port, &bytes) {
        Ok(()) => Ok("小票已打印".into()),
        Err(e) => {
            enqueue_retry(conn, order_no, "receipt_default", &e.to_string())?;
            Ok(format!("打印失败，已入重打队列: {e}"))
        }
    }
}

/// 交班汇总。
#[tauri::command]
pub fn shift_report(
    shift_id: i64,
    state: State<'_, AppState>,
) -> Result<Vec<checkout_core::order::ShiftSummary>> {
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
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
        initial_qty,
        alert_qty,
        Some(operator_id),
    )
}

#[tauri::command]
pub fn list_products(
    keyword: String,
    limit: i64,
    state: State<'_, AppState>,
) -> Result<Vec<checkout_core::product::ProductRow>> {
    let license = state
        .license
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    license.gate(ENT_REPORT_STANDARD)?;
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    let mut rows = checkout_core::product::list_products(&conn, &keyword, limit)?;
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

#[tauri::command]
pub fn set_product_status(spu_id: i64, status: i64, state: State<'_, AppState>) -> Result<()> {
    let (_operator_id, conn) = admin_guard(&state, PERM_PRODUCT_EDIT)?;
    checkout_core::product::set_product_status(&conn, spu_id, status)
}

#[tauri::command]
pub fn list_categories(
    state: State<'_, AppState>,
) -> Result<Vec<checkout_core::product::Category>> {
    let license = state
        .license
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    license.gate(ENT_REPORT_STANDARD)?;
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
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

/// 库存调整（inventory.adjust，F-13/F-14）：带符号差值，原因入 inv_trans 审计。
#[tauri::command]
pub fn adjust_stock(
    sku_id: i64,
    change_qty: i64,
    reason: String,
    state: State<'_, AppState>,
) -> Result<i64> {
    let (operator_id, conn) = admin_guard(&state, PERM_INVENTORY_ADJUST)?;
    checkout_core::product::adjust_stock(&conn, sku_id, change_qty, &reason, Some(operator_id))
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
    let (_operator_id, conn) = admin_guard(&state, PERM_REPORT_VIEW)?;
    if let Some(p) = &phone {
        if p.trim().is_empty() {
            return Err(CoreError::Validation("手机号不能为空".into()));
        }
    }
    let code = format!("M{}", chrono::Utc::now().timestamp_millis());
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
    let (_operator_id, conn) = admin_guard(&state, PERM_REPORT_VIEW)?;
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
/// 以 report.view 为门禁代理（收银员 ❌ / 店长 ✅ / 管理员 ✅，§12 权限矩阵）。
#[tauri::command]
pub fn recharge_member(
    member_id: i64,
    principal_cents: i64,
    bonus_cents: i64,
    state: State<'_, AppState>,
) -> Result<()> {
    let (operator_id, conn) = admin_guard(&state, PERM_REPORT_VIEW)?;
    if principal_cents <= 0 || bonus_cents < 0 {
        return Err(CoreError::Validation("充值金额非法".into()));
    }
    let recharge_no = gen_no("RC");
    checkout_core::member::recharge(&conn, member_id, &recharge_no, principal_cents, bonus_cents)?;
    checkout_core::rbac::audit_operation(
        &conn,
        "recharge_member",
        Some(operator_id),
        None,
        serde_json::json!({
            "member_id": member_id,
            "recharge_no": recharge_no,
            "principal_cents": principal_cents,
            "bonus_cents": bonus_cents
        }),
        &now_str(),
    )?;
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
    let rows = stmt.query_map([], |r| row_to_promotion_row(r))?;
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
    let session = state
        .session
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    session.as_ref().ok_or(CoreError::NotAuthenticated)?;
    drop(session);
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    if items.is_empty() {
        return Err(CoreError::Validation("请先选择退货明细".into()));
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
    drop(session);
    drop(license);
    let paid_via_scan: i64 = conn.query_row(
        "SELECT COUNT(*) FROM payment_record WHERE order_no=?1 AND pay_channel IN ('WECHAT','ALIPAY') AND status='SUCCESS'",
        [&order_no],
        |r| r.get(0),
    )?;
    let biz: String = conn.query_row(
        "SELECT business_date FROM sales_order WHERE order_no=?1",
        [&order_no],
        |r| r.get(0),
    )?;
    let cutoff = business_cutoff_hour(&conn);
    let today = checkout_core::order::business_date(&now_dt, cutoff);
    {
        let tx = conn.unchecked_transaction()?;
        checkout_core::order::void_order(
            &tx,
            &order_no,
            paid_via_scan > 0,
            biz == today,
            operator_id,
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
    let session = state
        .session
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    session.as_ref().ok_or(CoreError::NotAuthenticated)?;
    drop(session);
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
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
    drop(session);
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
    let mut scan_refunds: Vec<(String, PayChannel, i64)> = Vec::new();
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
            for (i, (channel, amt)) in allocated.into_iter().enumerate() {
                let refund_no = format!("{return_no}:R{}", i + 1);
                let is_scan = matches!(channel, PayChannel::WECHAT | PayChannel::ALIPAY);
                let status = if is_scan { "PROCESSING" } else { "COMPLETED" };
                tx.execute(
                    "INSERT INTO refund_record(pay_id,return_no,order_no,refund_no,amount_cents,status,created_at,updated_at)
                     VALUES ((SELECT pay_id FROM payment_record WHERE order_no=?1 AND pay_channel=?2 AND status='SUCCESS' ORDER BY pay_id LIMIT 1),
                             ?3,?4,?5,?6,?7,?8,?8)",
                    rusqlite::params![
                        order_no, channel.as_str(), return_no, order_no, refund_no, amt,
                        status, now_str()
                    ],
                )?;
                if is_scan {
                    scan_refunds.push((refund_no, channel, amt));
                }
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
    for (refund_no, channel, amt) in scan_refunds {
        let out_trade_no: Option<String> = conn
            .query_row(
                "SELECT out_trade_no FROM payment_record
                 WHERE order_no=?1 AND pay_channel=?2 AND status='SUCCESS' ORDER BY pay_id LIMIT 1",
                rusqlite::params![order_no, channel.as_str()],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        if let Some(otn) = out_trade_no {
            let gw = state.gateway.of_channel(channel);
            match gw.refund(&otn, &refund_no, amt) {
                Ok(PaymentState::SUCCESS) => {
                    conn.execute(
                        "UPDATE refund_record SET status='COMPLETED', updated_at=?1 WHERE refund_no=?2",
                        rusqlite::params![now_str(), refund_no],
                    )?;
                }
                Ok(_) => { /* PROCESSING 保持，等待退款查询收敛 */ }
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
                }
            }
        }
    }

    // 会员资产回补（领域函数自带事务，事务提交后在普通 conn 上执行）
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
                    &conn,
                    mid,
                    &format!("{return_no}:SV"),
                    &order_no,
                    sv_share,
                    original,
                )?;
            }
        }
        // 积分扣回（§13.4）：按退款额整元回收
        let per_yuan = cfg_i64(&conn, CFG_POINTS_PER_YUAN, DEFAULT_POINTS_PER_YUAN);
        if per_yuan > 0 && total >= 100 {
            let points_back = -(total / 100 * per_yuan);
            let _ = checkout_core::member::append_asset_log(
                &conn,
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
    *state
        .pending_sensitive
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))? =
        Some(PendingSensitive { auth, permission });
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
        | CFG_ALLOW_NEGATIVE_STOCK => {
            if value != "0" && value != "1" {
                return Err(CoreError::Validation("开关只允许 0 或 1".into()));
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

// ===== 统一报表中心（§14 扩展）：33 张报表共用一个命令 =====

/// 报表清单：按分组返回全部报表元数据，前端据此渲染左侧导航。
#[tauri::command]
pub fn report_catalog(state: State<'_, AppState>) -> Result<Vec<checkout_core::report_catalog::ReportMeta>> {
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
    drop(session);
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    if opening_float_cents < 0 {
        return Err(CoreError::Validation("备用金不能为负".into()));
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
    let approver_id = auth.approver_id;
    drop(session);
    drop(license);
    let opening: i64 = conn.query_row(
        "SELECT opening_float_cents FROM shift WHERE shift_id=?1",
        [shift_id],
        |r| r.get(0),
    )?;
    // 应收现金 = 备用金 + 班次内现金净收入
    let cash_net: i64 = conn.query_row(
        "SELECT COALESCE(SUM(CASE WHEN p.pay_channel='CASH' THEN p.amount_cents ELSE 0 END),0)
         FROM sales_order o JOIN payment_record p ON p.order_no=o.order_no AND p.status='SUCCESS'
         WHERE o.shift_id=?1 AND o.status IN ('COMPLETED','PARTIAL_REFUND')",
        [shift_id],
        |r| r.get(0),
    )?;
    let expected_cash = opening + cash_net;
    let diff = actual_cash_cents - expected_cash;
    conn.execute(
        "UPDATE shift SET closed_at=?1, actual_cash_cents=?2, expected_cash_cents=?3,
                difference_cents=?4, difference_reason=?5, status='CLOSED'
         WHERE shift_id=?6",
        rusqlite::params![
            now_str(),
            actual_cash_cents,
            expected_cash,
            diff,
            difference_reason,
            shift_id
        ],
    )?;
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
        actual_cash_cents: actual_cash_cents,
        difference_cents: diff,
    })
}

// ===== 打印配置与重打队列（§7.6 / PRT-06）=====

#[tauri::command]
pub fn print_retries(state: State<'_, AppState>) -> Result<Vec<String>> {
    let (_op, conn) = admin_guard(&state, PERM_REPORT_VIEW)?;
    checkout_core::escpos::pending_retries(&conn)
}

/// 手动重打：重新渲染并标记队列完成；不重复落库业务数据（PRT-06）。
#[tauri::command]
pub fn retry_print(order_no: String, state: State<'_, AppState>) -> Result<String> {
    let (_op, conn) = admin_guard(&state, PERM_REPORT_VIEW)?;
    let content: String = conn.query_row(
        "SELECT content_json FROM print_template WHERE template_id='receipt_default'",
        [],
        |r| r.get(0),
    )?;
    let template = checkout_core::escpos::Template {
        template_id: "receipt_default".into(),
        paper_width_mm: 58,
        sections: serde_json::from_str(&content)?,
    };
    let mut vars = std::collections::HashMap::new();
    vars.insert("order_no".into(), order_no.clone());
    // 渲染失败即保留在队列；成功标记完成。真实设备发送由硬件适配层完成。
    match checkout_core::escpos::render_receipt(&template, &vars) {
        Ok(_) => {
            conn.execute(
                "UPDATE print_retry_queue SET status='DONE', retry_count=retry_count+1
                 WHERE order_no=?1 AND status='PENDING'",
                [&order_no],
            )?;
            Ok("重打字节流已生成".into())
        }
        Err(e) => Ok(format!("重打仍失败: {e}")),
    }
}

// ===== 券管理（§13.7 / MEM-04）=====

#[derive(Debug, Serialize)]
pub struct CouponRow {
    pub coupon_code: String,
    pub member_id: Option<i64>,
    pub discount_cents: i64,
    pub min_order_cents: i64,
    pub status: String,
    pub valid_until: Option<String>,
}

#[tauri::command]
pub fn list_coupons(state: State<'_, AppState>) -> Result<Vec<CouponRow>> {
    let (_op, conn) = admin_guard(&state, PERM_REPORT_VIEW)?;
    let mut stmt = conn.prepare(
        "SELECT coupon_code,member_id,discount_cents,min_order_cents,status,valid_until
         FROM coupon ORDER BY coupon_id DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(CouponRow {
            coupon_code: r.get(0)?,
            member_id: r.get(1)?,
            discount_cents: r.get(2)?,
            min_order_cents: r.get(3)?,
            status: r.get(4)?,
            valid_until: r.get(5)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// 发券（§13.7）：状态机从 UNUSED 起。
#[tauri::command]
pub fn issue_coupon(
    member_id: Option<i64>,
    discount_cents: i64,
    min_order_cents: i64,
    valid_until: Option<String>,
    state: State<'_, AppState>,
) -> Result<String> {
    let (_op, conn) = admin_guard(&state, PERM_PRODUCT_EDIT)?;
    if discount_cents <= 0 || min_order_cents < 0 {
        return Err(CoreError::Validation("券额度/门槛非法".into()));
    }
    let code = format!("CP{}", Utc::now().timestamp_millis());
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
pub fn poll_native_pay(order_no: String, state: State<'_, AppState>) -> Result<CheckoutResult> {
    let license = state
        .license
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    license.gate(ENT_POS_CORE)?;
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
    drop(license);
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
        // 幂等：已完成/已作废直接回当前状态
        return Ok(CheckoutResult {
            order_no,
            status,
            code_url: None,
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
            finalize_order_assets(&conn, &order_no)?;
            auto_print_receipt(&conn, &order_no);
            Ok(CheckoutResult {
                order_no,
                status: "COMPLETED".into(),
                code_url: None,
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
            })
        }
        PaymentState::FAILED(msg) => Ok(CheckoutResult {
            order_no,
            status: format!("FAILED:{msg}"),
            code_url: None,
        }),
        PaymentState::USERPAYING => Ok(CheckoutResult {
            order_no,
            status: "PENDING_PAYMENT".into(),
            code_url: None,
        }),
    }
}

/// 取消 Native 支付：渠道关单；遇"已支付"自动全额退款 + 审计（§6.2）。
#[tauri::command]
pub fn cancel_native_pay(order_no: String, state: State<'_, AppState>) -> Result<()> {
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
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
    let outcome = match gw.cancel(&out_trade_no)? {
        checkout_core::payment::CancelResult::Canceled => "CANCELED".to_string(),
        checkout_core::payment::CancelResult::AlreadyPaid => {
            // 渠道已扣款 → 自动全额退款（幂等 refund_no）+ 审计
            let refund_no = format!("{out_trade_no}:AUTO_REFUND");
            let _ = gw.refund(&out_trade_no, &refund_no, amount);
            checkout_core::rbac::audit_operation(
                &conn,
                "native_cancel_already_paid",
                None,
                None,
                serde_json::json!({"order_no": order_no, "auto_refunded": amount}),
                &now_str(),
            )?;
            "AUTO_REFUNDED".to_string()
        }
    };
    conn.execute(
        "UPDATE payment_record SET status='CLOSED' WHERE order_no=?1 AND status='PENDING'",
        [order_no.clone()],
    )?;
    release_order_coupon(&conn, &order_no)?;
    checkout_core::rbac::audit_operation(
        &conn,
        "cancel_native_pay",
        None,
        None,
        serde_json::json!({"order_no": order_no, "outcome": outcome}),
        &now_str(),
    )?;
    Ok(())
}

/// 本地确认 Native 支付完成（无真实渠道凭据时的本地确认模式）。
#[tauri::command]
pub fn confirm_native_pay(order_no: String, state: State<'_, AppState>) -> Result<()> {
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
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
    finalize_order_assets(&conn, &order_no)?;
    auto_print_receipt(&conn, &order_no);
    Ok(())
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
/// 有 SUCCESS 流水 → 资产收口补提交；Native 待支付 → 查单成功收口否则关单作废；
/// 纯本地未支付 → 作废。全部留审计。返回处理单数。
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
        let has_success = records.iter().any(|(_, _, _, st)| st == "SUCCESS");
        if has_success {
            // 支付已确认但崩溃在提交前 → 补收口
            finalize_order_assets(conn, &order_no)?;
            recovered += 1;
            continue;
        }
        let native = records
            .iter()
            .find(|(_, mode, otn, st)| {
                mode == PAY_MODE_NATIVE && st == "PENDING" && !otn.is_empty()
            })
            .map(|(ch, _, otn, _)| (ch.clone(), otn.clone()));
        match native {
            Some((ch, otn)) => {
                let channel = match ch.as_str() {
                    PAY_WECHAT => PayChannel::WECHAT,
                    PAY_ALIPAY => PayChannel::ALIPAY,
                    _ => PayChannel::CASH,
                };
                let gw = gateway.of_channel(channel);
                let settled = match gw.query(&otn) {
                    Ok(PaymentState::SUCCESS) => {
                        conn.execute(
                            "UPDATE payment_record SET status='SUCCESS', paid_at=?1 WHERE order_no=?2 AND out_trade_no=?3",
                            rusqlite::params![now_str(), order_no, otn],
                        )?;
                        true
                    }
                    _ => false,
                };
                if settled {
                    finalize_order_assets(conn, &order_no)?;
                } else {
                    let _ = gw.cancel(&otn);
                    conn.execute(
                        "UPDATE payment_record SET status='CLOSED' WHERE order_no=?1 AND out_trade_no=?2",
                        rusqlite::params![order_no, otn],
                    )?;
                    release_order_coupon(conn, &order_no)?;
                    void_pending_order_system(conn, &order_no)?;
                }
                recovered += 1;
            }
            None => {
                // 纯本地渠道未支付 → 作废
                for (_, _, otn, st) in &records {
                    if st == "PENDING" && !otn.is_empty() {
                        conn.execute(
                            "UPDATE payment_record SET status='CLOSED' WHERE order_no=?1 AND out_trade_no=?2",
                            rusqlite::params![order_no, otn],
                        )?;
                    }
                }
                release_order_coupon(conn, &order_no)?;
                void_pending_order_system(conn, &order_no)?;
                recovered += 1;
            }
        }
    }
    Ok(recovered)
}

/// 系统级作废待支付订单（恢复扫描专用）：走领域 void_order，当日外订单直接关闭并审计。
fn void_pending_order_system(conn: &Connection, order_no: &str) -> Result<()> {
    let biz: String = conn.query_row(
        "SELECT business_date FROM sales_order WHERE order_no=?1",
        [order_no],
        |r| r.get(0),
    )?;
    let cutoff = business_cutoff_hour(conn);
    let today = checkout_core::order::business_date(&parse_rfc3339(&now_str())?, cutoff);
    let tx = conn.unchecked_transaction()?;
    let result = checkout_core::order::void_order(&tx, order_no, false, biz == today, 0);
    match result {
        Ok(()) => {
            tx.commit()?;
        }
        Err(_) => {
            // 非当日待支付单：恢复扫描按系统动作直接关闭并审计（§23）
            tx.rollback()?;
            conn.execute(
                "UPDATE sales_order SET status='CANCELED' WHERE order_no=?1 AND status='PENDING_PAYMENT'",
                [order_no],
            )?;
            checkout_core::rbac::audit_operation(
                conn,
                "recovery_void_order",
                None,
                None,
                serde_json::json!({"order_no": order_no, "method": "direct_close"}),
                &now_str(),
            )?;
        }
    }
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
    // 落库用绝对路径：前端 convertFileSrc 需要绝对路径才能转成可展示地址
    let abs = path.to_string_lossy().to_string();
    checkout_core::product::set_product_image_path(&conn, sku_id, Some(abs.as_str()))?;
    Ok(abs)
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
    drop(session);
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
    drop(session);
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

/// 在线激活：向激活服务换取签名激活码 → 本地验签 → 落库。
/// 签发权始终在服务端；客户端只做验签与存储，与离线导入共用同一条链路。
#[tauri::command]
pub fn activate_online(
    activation_key: String,
    customer_name: Option<String>,
    state: State<'_, AppState>,
) -> Result<LicenseSnapshot> {
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
    Ok(checkout_core::license::online::probe_service(&endpoint)?)
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
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
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
            let gateway = checkout_core::payment::RoutingGateway::new(wechat_gw, alipay_gw);

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
                public_key: checkout_core::EMBEDDED_PUBLIC_KEY,
                license_path: data_dir.join("license.enc"),
                data_dir: data_dir.clone(),
                gateway,
                current_shift: std::sync::Mutex::new(0i64),
                pending_sensitive: std::sync::Mutex::new(None),
            });

            // 系统托盘：关闭按钮只隐藏到托盘，真正退出走托盘菜单。
            // 图标优先用应用默认图标，构造失败则跳过托盘，不影响主程序启动。
            let tray_icon = app.default_window_icon().cloned().or_else(|| {
                tauri::image::Image::from_bytes(include_bytes!("../icons/icon.png")).ok()
            });
            if let Some(icon) = tray_icon {
                let show_item = tauri::menu::MenuItem::with_id(
                    app,
                    "tray_show",
                    "显示收银台",
                    true,
                    None::<&str>,
                );
                let quit_item = tauri::menu::MenuItem::with_id(
                    app,
                    "tray_quit",
                    "退出程序",
                    true,
                    None::<&str>,
                );
                if let (Ok(show_item), Ok(quit_item)) = (show_item, quit_item) {
                    if let Ok(tray_menu) =
                        tauri::menu::Menu::with_items(app, &[&show_item, &quit_item])
                    {
                        match tauri::tray::TrayIconBuilder::new()
                            .icon(icon)
                            .tooltip("Checkout-R 收银系统")
                            // 左键点击直接恢复窗口；右键才弹菜单（显示/退出）
                            .show_menu_on_left_click(false)
                            .menu(&tray_menu)
                            .on_menu_event(|app, event| match event.id.as_ref() {
                                "tray_show" => {
                                    if let Some(window) = app.get_webview_window("main") {
                                        let _ = window.show();
                                        let _ = window.set_focus();
                                    }
                                }
                                "tray_quit" => app.exit(0),
                                _ => {}
                            })
                            .on_tray_icon_event(|tray, event| {
                                if let tauri::tray::TrayIconEvent::Click {
                                    button: tauri::tray::MouseButton::Left,
                                    ..
                                } = event
                                {
                                    let app = tray.app_handle();
                                    if let Some(window) = app.get_webview_window("main") {
                                        let _ = window.show();
                                        let _ = window.set_focus();
                                    }
                                }
                            })
                            .build(app)
                        {
                            // TrayIcon 是 RAII 句柄，必须长期持有，否则图标立即消失
                            Ok(tray) => app.manage(tray),
                            Err(e) => tracing::warn!("托盘图标创建失败: {e}"),
                        }
                    }
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // 关闭按钮隐藏到托盘；真正退出走托盘菜单「退出程序」
                api.prevent_close();
                if let Err(e) = window.hide() {
                    tracing::warn!("隐藏窗口到托盘失败: {e}");
                }
                // 通知前端锁屏：从托盘恢复后需输入锁机码
                if let Err(e) = window.emit("lock-request", ()) {
                    tracing::warn!("发送锁屏事件失败: {e}");
                }
            }
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
            lookup_product,
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
            reconcile_csv,
            print_receipt,
            print_retries,
            retry_print,
            shift_report,
            open_shift,
            close_shift,
            request_sensitive_auth,
            get_configs,
            set_config,
            list_coupons,
            issue_coupon,
            create_product,
            list_products,
            update_product,
            set_product_status,
            list_categories,
            create_category,
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
        ])
        .run(tauri::generate_context!())
        .expect("应用启动失败");
}
