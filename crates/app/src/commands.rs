//! Tauri 命令层。所有命令在此注册；capabilities 仅暴露此清单。
//! 门禁统一入口按 §12.2 顺序执行。

use crate::state::{build_license_service, init_db, AppState};
use checkout_core::error::{CoreError, Result};
use checkout_core::license::LicenseSnapshot;
use checkout_core::rbac::{self, Session};
use checkout_core::time::parse_rfc3339;
use serde::{Deserialize, Serialize};
use tauri::{Manager, State};

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
fn admin_guard(
    state: &AppState,
    permission: &str,
) -> Result<(i64, std::sync::MutexGuard<'_, rusqlite::Connection>)> {
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
                entitlement: "report.standard",
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
        "SELECT s.sku_id, p.spu_name, COALESCE(s.barcode,''), s.sale_price_cents, s.is_weighed
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

/// 商品档案创建（F-11/F-12）：SPU/SKU 分离，需 product.edit 权限。
#[tauri::command]
pub fn create_product(
    name: String,
    barcode: Option<String>,
    category_id: i64,
    unit: String,
    sale_price_cents: i64,
    cost_price_cents: i64,
    is_weighed: bool,
    initial_qty: i64,
    alert_qty: i64,
    state: State<'_, AppState>,
) -> Result<i64> {
    let (operator_id, conn) = admin_guard(&state, "product.edit")?;
    checkout_core::product::create_product(
        &conn,
        category_id,
        &name,
        barcode.as_deref(),
        &unit,
        sale_price_cents,
        cost_price_cents,
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
    license.gate("report.standard")?;
    let conn = state
        .conn
        .lock()
        .map_err(|e| CoreError::Other(e.to_string()))?;
    checkout_core::product::list_products(&conn, &keyword, limit)
}

#[tauri::command]
pub fn update_product(
    sku_id: i64,
    sale_price_cents: i64,
    alert_qty: i64,
    state: State<'_, AppState>,
) -> Result<()> {
    let (_operator_id, conn) = admin_guard(&state, "product.edit")?;
    checkout_core::product::update_product(&conn, sku_id, sale_price_cents, alert_qty)
}

#[tauri::command]
pub fn set_product_status(spu_id: i64, status: i64, state: State<'_, AppState>) -> Result<()> {
    let (_operator_id, conn) = admin_guard(&state, "product.edit")?;
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
    license.gate("report.standard")?;
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
    let (_operator_id, conn) = admin_guard(&state, "product.edit")?;
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
    let (operator_id, conn) = admin_guard(&state, "inventory.adjust")?;
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
    let (approver_id, conn) = admin_guard(&state, "stocktake.approve")?;
    checkout_core::purchase::approve_stocktake(&conn, &task_no, approver_id)
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
                license_path: data_dir.join("license.enc"),
            });
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
        ])
        .run(tauri::generate_context!())
        .expect("应用启动失败");
}
