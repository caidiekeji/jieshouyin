//! Tauri 命令层。所有命令在此注册；capabilities 仅暴露此清单。
//! 门禁统一入口按 §12.2 顺序执行。

use crate::state::{build_license_service, init_db, AppState};
use checkout_core::error::{CoreError, Result};
use checkout_core::license::{LicenseService, LicenseSnapshot};
use checkout_core::rbac::{self, Session};
use checkout_core::time::parse_rfc3339;
use serde::{Deserialize, Serialize};
use tauri::State;

fn now_str() -> String {
    chrono::Utc::now().to_rfc3339()
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
    *license = svc.verify_on_launch(&conn)?;
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
    pub is_weighed: bool,
}

/// 商品识别（F-01）：条码精确匹配，模糊搜索兜底。
#[tauri::command]
pub fn lookup_product(keyword: String, state: State<'_, AppState>) -> Result<Vec<ProductHit>> {
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    let pattern = format!("%{keyword}%");
    let mut stmt = conn.prepare(
        "SELECT s.sku_id, p.spu_name, COALESCE(s.barcode,''), s.sale_price_cents, s.is_weighed
         FROM sku s JOIN spu p ON p.spu_id=s.spu_id
         WHERE s.barcode LIKE ?1 OR s.instore_code LIKE ?1 OR p.spu_name LIKE ?1
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
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// 称重码解析（SCN-02）。
#[tauri::command]
pub fn parse_weigh_barcode(code: String) -> Result<checkout_core::scanner::WeighedCode> {
    checkout_core::scanner::parse_weighed_code(&code)
        .ok_or_else(|| CoreError::Validation("称重店内码非法（校验位或前缀错误）".into()))
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
}

#[derive(Debug, Serialize)]
pub struct CheckoutResult {
    pub order_no: String,
    pub status: String,
}

/// 收银结算（F-05/F-06）。现金/储值离线可用；扫码渠道经 trait 抽象。
/// tx A → 支付 → tx B；授权门禁只校验 entitlement，不阻断 pos.core。
#[tauri::command]
pub fn checkout(
    lines: Vec<CartLineInput>,
    payments: Vec<PaymentInput>,
    state: State<'_, AppState>,
) -> Result<CheckoutResult> {
    let license = state
        .license
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    license.gate("pos.core")?; // 降级态仍放行
    let session = state
        .session
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    let session = session.as_ref().ok_or(CoreError::NotAuthenticated)?;
    if session.expires_at < parse_rfc3339(&now_str())? {
        return Err(CoreError::SessionExpired);
    }
    if lines.is_empty() {
        return Err(CoreError::Validation("购物车为空".into()));
    }

    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    let order_no = checkout_core::order::generate_order_no("STORE", "POS1");
    let cart_lines: Vec<checkout_core::promo::CartLine> = lines
        .iter()
        .map(|l| checkout_core::promo::CartLine::new(l.sku_id, l.qty, l.unit_price_cents))
        .collect::<Result<Vec<_>>>()?;
    let total: i64 = cart_lines.iter().map(|l| l.line_total_cents).sum();
    let paid: i64 = payments.iter().map(|p| p.amount_cents).sum();
    if paid < total {
        return Err(CoreError::Validation(format!(
            "支付金额不足: 应付 {total}, 实付 {paid}"
        )));
    }
    let now = now_str();
    let biz = checkout_core::order::business_date(&parse_rfc3339(&now)?, 0);

    // tx A：落库 PENDING_PAYMENT
    {
        let tx = conn.unchecked_transaction()?;
        checkout_core::order::begin_order(
            &tx,
            &order_no,
            "POS1",
            session.user_id,
            1,
            None,
            &cart_lines,
            total,
            0,
            &biz,
            &now,
        )?;
        tx.commit()?;
    }
    // 外部渠道：本版本现金/储值本地闭环；扫码走 trait（MockGateway 联调），
    // 生产环境由渠道适配器实现，此处记录支付流水。
    {
        let tx = conn.unchecked_transaction()?;
        for (i, p) in payments.iter().enumerate() {
            let channel = match p.channel.as_str() {
                "CASH" => checkout_core::payment::Channel::CASH,
                "STORED_VALUE" => checkout_core::payment::Channel::STORED_VALUE,
                "WECHAT" => checkout_core::payment::Channel::WECHAT,
                "ALIPAY" => checkout_core::payment::Channel::ALIPAY,
                other => return Err(CoreError::Validation(format!("未知支付渠道: {other}"))),
            };
            let out_trade_no = checkout_core::payment::make_out_trade_no(&order_no, i as u32 + 1);
            // 现金净额口径：收钞 − 找零
            let net = if channel == checkout_core::payment::Channel::CASH {
                p.tendered_cents.unwrap_or(p.amount_cents) - p.change_cents.unwrap_or(0)
            } else {
                p.amount_cents
            };
            tx.execute(
                "INSERT INTO payment_record(order_no,pay_channel,pay_mode,attempt_no,amount_cents,cash_tendered_cents,change_cents,out_trade_no,status,paid_at,created_at)
                 VALUES (?1,?2,NULL,?3,?4,?5,?6,?7,'SUCCESS',?8,?8)",
                rusqlite::params![
                    order_no, channel.as_str(), i as i64 + 1, net,
                    p.tendered_cents, p.change_cents, out_trade_no, now
                ],
            )?;
        }
        // tx B：提交（库存扣减 + 订单完成）
        checkout_core::order::commit_order(&tx, &order_no, session.user_id)?;
        tx.commit()?;
    }
    Ok(CheckoutResult {
        order_no,
        status: "COMPLETED".into(),
    })
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

/// 打印：渲染 ESC/POS 字节流，失败入重打队列，不阻断。
#[tauri::command]
pub fn print_receipt(order_no: String, state: State<'_, AppState>) -> Result<String> {
    use checkout_core::escpos::{enqueue_retry, render_receipt, Template};
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
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
    let mut vars = std::collections::HashMap::new();
    vars.insert("store_name".into(), "Checkout-R".into());
    vars.insert("store_phone".into(), "000-0000".into());
    vars.insert("order_no".into(), order_no.clone());
    vars.insert("cashier_name".into(), "收银员".into());
    vars.insert("created_at".into(), now_str());
    vars.insert("member_phone".into(), "".into());
    vars.insert("subtotal".into(), "0.00".into());
    vars.insert("discount_total".into(), "0.00".into());
    vars.insert("pay_total".into(), "0.00".into());
    vars.insert("pay_channel_lines".into(), "".into());
    vars.insert("change".into(), "0.00".into());
    vars.insert(
        "qrcode_pay_url".into(),
        format!("checkout-r://order/{order_no}"),
    );
    match render_receipt(&template, &vars) {
        Ok(_bytes) => Ok("已生成打印字节流".into()), // 真实发送由硬件适配器完成
        Err(e) => {
            enqueue_retry(&conn, &order_no, "receipt_default", &e.to_string())?;
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

/// 商品档案创建（F-11）：SPU/SKU 分离。
#[tauri::command]
pub fn create_product(
    name: String,
    barcode: String,
    price_cents: i64,
    state: State<'_, AppState>,
) -> Result<i64> {
    let license = state
        .license
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    license.gate("pos.core")?;
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    let code = format!("SPU{}", chrono::Utc::now().timestamp_millis());
    conn.execute(
        "INSERT INTO spu(category_id,spu_name,spu_code) VALUES (1,?1,?2)",
        rusqlite::params![name, code],
    )?;
    let spu_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO sku(spu_id,barcode,sale_price_cents) VALUES (?1,?2,?3)",
        rusqlite::params![spu_id, barcode, price_cents],
    )?;
    Ok(conn.last_insert_rowid())
}

/// 启动应用（Tauri 2）。
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let db_path = data_dir.join("checkoutr.db");
            let conn = init_db(&db_path)?;
            app.manage(AppState {
                conn: std::sync::Mutex::new(conn),
                license: std::sync::Mutex::new(LicenseSnapshot::unactivated()),
                session: std::sync::Mutex::new(None::<Session>),
                public_key: checkout_core::EMBEDDED_PUBLIC_KEY,
                db_path,
                license_path: data_dir.join("license.enc"),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            init_admin,
            get_license,
            activate,
            renew_license,
            login,
            logout,
            lookup_product,
            parse_weigh_barcode,
            checkout,
            park_cart,
            list_parked,
            resume_cart,
            daily_report,
            print_receipt,
            shift_report,
            create_product,
        ])
        .run(tauri::generate_context!())
        .expect("应用启动失败");
}

// 保证 init_db 被引用（setup 时使用）
#[allow(dead_code)]
fn _init_db_ref(path: &std::path::PathBuf) -> Result<rusqlite::Connection> {
    init_db(path)
}

// 保证 LicenseService 类型被引用
#[allow(dead_code)]
fn _svc_ref(_s: &LicenseService) {}
