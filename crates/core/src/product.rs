//! 商品主数据（F-11/F-12/F-15）：SPU/SKU 档案、多级类目、库存台账查询。
//! 库存增减必经 inventory::apply_event（F-13），本模块只提供档案与查询。

use crate::constants::TRANS_INITIAL;
use crate::error::{CoreError, Result};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};

#[derive(Debug, Clone, serde::Serialize)]
pub struct Category {
    pub category_id: i64,
    pub parent_id: Option<i64>,
    pub name: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProductRow {
    pub sku_id: i64,
    pub spu_id: i64,
    pub name: String,
    pub barcode: Option<String>,
    pub category_id: i64,
    pub category_name: String,
    pub unit: String,
    pub sale_price_cents: i64,
    pub cost_price_cents: i64,
    pub member_price_cents: Option<i64>,
    pub instore_code: Option<String>,
    pub is_weighed: bool,
    /// 服务类商品：按次售卖，不建库存台账、销售与退货都不动库存。
    pub is_service: bool,
    pub status: i64,
    pub qty: i64,
    pub alert_qty: i64,
    /// 本地上传的图片路径：库内存绝对路径（前端需绝对路径转可展示地址）。
    pub image_path: Option<String>,
    /// 在线图片地址（http/https）。
    pub image_url: Option<String>,
}

/// 新建分类（F-12 多级类目），返回 category_id。
pub fn create_category(conn: &Connection, parent_id: Option<i64>, name: &str) -> Result<i64> {
    if name.trim().is_empty() {
        return Err(CoreError::Validation("分类名不能为空".into()));
    }
    if let Some(pid) = parent_id {
        let exists: Option<i64> = conn
            .query_row(
                "SELECT category_id FROM category WHERE category_id=?1",
                [pid],
                |r| r.get(0),
            )
            .optional()?;
        if exists.is_none() {
            return Err(CoreError::Validation(format!("父分类 {pid} 不存在")));
        }
    }
    conn.execute(
        "INSERT INTO category(parent_id,name) VALUES (?1,?2)",
        params![parent_id, name.trim()],
    )?;
    Ok(conn.last_insert_rowid())
}

/// 改名/改父级：父级链不得成环（不能挂到自己或自己的后代下）。
pub fn update_category(
    conn: &Connection,
    category_id: i64,
    parent_id: Option<i64>,
    name: &str,
) -> Result<()> {
    if name.trim().is_empty() {
        return Err(CoreError::Validation("分类名不能为空".into()));
    }
    let exists: Option<i64> = conn
        .query_row(
            "SELECT category_id FROM category WHERE category_id=?1 AND status=1",
            [category_id],
            |r| r.get(0),
        )
        .optional()?;
    if exists.is_none() {
        return Err(CoreError::Validation(format!("分类 {category_id} 不存在")));
    }
    if let Some(mut pid) = parent_id {
        if pid == category_id {
            return Err(CoreError::Validation("上级分类不能是自己".into()));
        }
        // 沿父链上溯，若遇到自己则说明目标父级在后代中，会形成环。
        let mut depth = 0;
        loop {
            if pid == category_id {
                return Err(CoreError::Validation("上级分类不能是自己的下级".into()));
            }
            depth += 1;
            if depth > 32 {
                return Err(CoreError::Validation("分类层级异常（疑似已有环）".into()));
            }
            let next: Option<Option<i64>> = conn
                .query_row(
                    "SELECT parent_id FROM category WHERE category_id=?1 AND status=1",
                    [pid],
                    |r| r.get(0),
                )
                .optional()?;
            match next {
                Some(Some(p)) => pid = p,
                Some(None) => break,
                None => {
                    return Err(CoreError::Validation(format!("上级分类 {pid} 不存在")));
                }
            }
        }
    }
    conn.execute(
        "UPDATE category SET name=?1, parent_id=?2 WHERE category_id=?3",
        params![name.trim(), parent_id, category_id],
    )?;
    Ok(())
}

/// 删除分类（软删 status=0）：有在售商品或在用子分类时拒绝，避免商品失去归属。
pub fn delete_category(conn: &Connection, category_id: i64) -> Result<()> {
    let exists: Option<i64> = conn
        .query_row(
            "SELECT category_id FROM category WHERE category_id=?1 AND status=1",
            [category_id],
            |r| r.get(0),
        )
        .optional()?;
    if exists.is_none() {
        return Err(CoreError::Validation(format!("分类 {category_id} 不存在")));
    }
    let sku_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM spu WHERE category_id=?1 AND status=1",
        [category_id],
        |r| r.get(0),
    )?;
    if sku_count > 0 {
        return Err(CoreError::Validation(format!(
            "分类下还有 {sku_count} 个商品，请先移出或停用"
        )));
    }
    let child_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM category WHERE parent_id=?1 AND status=1",
        [category_id],
        |r| r.get(0),
    )?;
    if child_count > 0 {
        return Err(CoreError::Validation(format!(
            "分类下还有 {child_count} 个子分类，请先删除子分类"
        )));
    }
    conn.execute(
        "UPDATE category SET status=0 WHERE category_id=?1",
        [category_id],
    )?;
    Ok(())
}

pub fn list_categories(conn: &Connection) -> Result<Vec<Category>> {
    let mut stmt = conn.prepare(
        "SELECT category_id, parent_id, name FROM category WHERE status=1 ORDER BY category_id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Category {
            category_id: r.get(0)?,
            parent_id: r.get(1)?,
            name: r.get(2)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// 新建商品（F-11）：SPU/SKU 分离，库存永远挂 SKU。
/// 初始库存经 inv_trans INITIAL 事件投影写入，保证库存变化有据可查（F-13）。
/// 返回 sku_id。
#[allow(clippy::too_many_arguments)]
pub fn create_product(
    conn: &Connection,
    category_id: i64,
    name: &str,
    barcode: Option<&str>,
    unit: &str,
    sale_price_cents: i64,
    cost_price_cents: i64,
    is_weighed: bool,
    initial_qty: i64,
    alert_qty: i64,
    operator_id: Option<i64>,
) -> Result<i64> {
    let tx = conn.unchecked_transaction()?;
    let sku_id = create_product_in(
        &tx,
        category_id,
        name,
        barcode,
        unit,
        sale_price_cents,
        cost_price_cents,
        is_weighed,
        false,
        initial_qty,
        alert_qty,
        operator_id,
    )?;
    tx.commit()?;
    Ok(sku_id)
}

/// 事务内的建品实现：调用方负责事务边界，便于与店内码/会员价一次性提交。
#[allow(clippy::too_many_arguments)]
fn create_product_in(
    tx: &Connection,
    category_id: i64,
    name: &str,
    barcode: Option<&str>,
    unit: &str,
    sale_price_cents: i64,
    cost_price_cents: i64,
    is_weighed: bool,
    is_service: bool,
    initial_qty: i64,
    alert_qty: i64,
    operator_id: Option<i64>,
) -> Result<i64> {
    if name.trim().is_empty() {
        return Err(CoreError::Validation("商品名不能为空".into()));
    }
    if name.trim().chars().count() > crate::constants::PRODUCT_NAME_MAX_CHARS {
        return Err(CoreError::Validation(format!(
            "商品名不能超过 {} 个字符",
            crate::constants::PRODUCT_NAME_MAX_CHARS
        )));
    }
    if sale_price_cents < 0 {
        return Err(CoreError::Validation("售价不能为负".into()));
    }
    if cost_price_cents < 0 || initial_qty < 0 || alert_qty < 0 {
        return Err(CoreError::Validation("成本/库存/预警值不能为负".into()));
    }
    if is_service && is_weighed {
        return Err(CoreError::Validation("服务类商品不能同时按重量计价".into()));
    }
    if is_service && initial_qty > 0 {
        return Err(CoreError::Validation(
            "服务类商品不计库存，期初库存必须为 0".into(),
        ));
    }
    let cat_exists: Option<i64> = tx
        .query_row(
            "SELECT category_id FROM category WHERE category_id=?1",
            [category_id],
            |r| r.get(0),
        )
        .optional()?;
    if cat_exists.is_none() {
        return Err(CoreError::Validation(format!("分类 {category_id} 不存在")));
    }
    // 条码先查重，给收银员可读错误，而不是把 UNIQUE constraint failed 抛到前端。
    if let Some(b) = barcode.map(str::trim).filter(|b| !b.is_empty()) {
        let exists: Option<i64> = tx
            .query_row("SELECT sku_id FROM sku WHERE barcode=?1", [b], |r| r.get(0))
            .optional()?;
        if exists.is_some() {
            return Err(CoreError::Validation(format!("条码 {b} 已存在")));
        }
    }
    // SPU 编码加随机段，避免毫秒时间戳并发撞码。
    let code = format!(
        "SPU{}{:04}",
        Utc::now().timestamp_millis(),
        rand::random::<u16>() % 10000
    );
    tx.execute(
        "INSERT INTO spu(category_id,spu_name,spu_code,unit) VALUES (?1,?2,?3,?4)",
        params![category_id, name.trim(), code, unit],
    )?;
    let spu_id = tx.last_insert_rowid();
    tx.execute(
        "INSERT INTO sku(spu_id,barcode,sale_price_cents,cost_price_cents,is_weighed,is_service,alert_qty)
         VALUES (?1,?2,?3,?4,?5,?6,?7)",
        params![
            spu_id,
            barcode.map(str::trim).filter(|b| !b.is_empty()),
            sale_price_cents,
            cost_price_cents,
            is_weighed as i64,
            is_service as i64,
            alert_qty
        ],
    )?;
    let sku_id = tx.last_insert_rowid();
    // 服务类商品只按次售卖，不建库存台账。
    if !is_service {
        if initial_qty > 0 {
            crate::inventory::apply_event(
                tx,
                sku_id,
                initial_qty,
                crate::inventory::TransType::PURCHASE_IN,
                Some(TRANS_INITIAL),
                operator_id,
                false,
            )?;
        } else {
            // 零库存也建立台账行，便于预警与盘点
            tx.execute(
                "INSERT INTO inv_stock(sku_id,warehouse_type,qty,version) VALUES (?1,'STORE',0,1)",
                [sku_id],
            )?;
        }
    }
    Ok(sku_id)
}

/// 商品列表：默认只返回在售（status=1），与 POS 扫码口径一致；
/// 后台管理需包含停用商品时显式传 include_disabled=true。
pub fn list_products(
    conn: &Connection,
    keyword: &str,
    limit: i64,
    include_disabled: bool,
) -> Result<Vec<ProductRow>> {
    let pattern = format!("%{keyword}%");
    let mut stmt = conn.prepare(
        "SELECT s.sku_id, p.spu_id, p.spu_name, s.barcode, p.category_id, COALESCE(c.name,'未分类'),
                p.unit, s.sale_price_cents, s.cost_price_cents, s.is_weighed, p.status,
                COALESCE(i.qty,0), s.alert_qty, s.member_price_cents, s.instore_code,
                s.image_path, s.image_url, s.is_service
         FROM sku s
         JOIN spu p ON p.spu_id = s.spu_id
         LEFT JOIN category c ON c.category_id = p.category_id
         LEFT JOIN inv_stock i ON i.sku_id = s.sku_id AND i.warehouse_type='STORE'
         WHERE (?1 = '' OR s.barcode LIKE ?2 OR p.spu_name LIKE ?2)
           AND (?4 <> 0 OR p.status = 1)
         ORDER BY p.spu_id DESC LIMIT ?3",
    )?;
    let rows = stmt.query_map(
        params![keyword, pattern, limit, include_disabled as i64],
        |r| {
            Ok(ProductRow {
                sku_id: r.get(0)?,
                spu_id: r.get(1)?,
                name: r.get(2)?,
                barcode: r.get(3)?,
                category_id: r.get(4)?,
                category_name: r.get(5)?,
                unit: r.get(6)?,
                sale_price_cents: r.get(7)?,
                cost_price_cents: r.get(8)?,
                is_weighed: r.get::<_, i64>(9)? != 0,
                status: r.get(10)?,
                qty: r.get(11)?,
                alert_qty: r.get(12)?,
                member_price_cents: r.get(13)?,
                instore_code: r.get(14)?,
                image_path: r.get(15)?,
                image_url: r.get(16)?,
                is_service: r.get::<_, i64>(17)? != 0,
            })
        },
    )?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// 新建商品（完整版）：在基础字段外补称重店内码（§15.2）与会员价（§5.3）。
#[allow(clippy::too_many_arguments)]
pub fn create_product_full(
    conn: &Connection,
    category_id: i64,
    name: &str,
    barcode: Option<&str>,
    instore_code: Option<&str>,
    unit: &str,
    sale_price_cents: i64,
    cost_price_cents: i64,
    member_price_cents: Option<i64>,
    is_weighed: bool,
    is_service: bool,
    initial_qty: i64,
    alert_qty: i64,
    operator_id: Option<i64>,
) -> Result<i64> {
    if is_weighed && instore_code.is_none() {
        return Err(CoreError::Validation("称重商品必须填写店内码".into()));
    }
    // 店内码是唯一检索键，先给可读错误。
    if let Some(code) = instore_code.map(str::trim).filter(|c| !c.is_empty()) {
        let exists: Option<i64> = conn
            .query_row(
                "SELECT sku_id FROM sku WHERE instore_code=?1",
                [code],
                |r| r.get(0),
            )
            .optional()?;
        if exists.is_some() {
            return Err(CoreError::Validation(format!("店内码 {code} 已存在")));
        }
    }
    // 建品、店内码、会员价必须同一事务：否则主记录已提交而店内码写失败会留下半成品。
    let tx = conn.unchecked_transaction()?;
    let sku_id = create_product_in(
        &tx,
        category_id,
        name,
        barcode,
        unit,
        sale_price_cents,
        cost_price_cents,
        is_weighed,
        is_service,
        initial_qty,
        alert_qty,
        operator_id,
    )?;
    if instore_code.is_some() || member_price_cents.is_some() {
        tx.execute(
            "UPDATE sku SET instore_code=?1, member_price_cents=?2 WHERE sku_id=?3",
            params![instore_code, member_price_cents, sku_id],
        )?;
    }
    tx.commit()?;
    Ok(sku_id)
}

/// 商品图片：本地上传文件与在线地址两个来源。
/// 两个来源各自独立登记：设置其中一个不会抹掉另一个（前端按「本地优先」展示）。
/// 本地图片文件名登记（None 表示清除），不校验协议。
pub fn set_product_image_path(
    conn: &Connection,
    sku_id: i64,
    image_path: Option<&str>,
) -> Result<()> {
    let n = conn.execute(
        "UPDATE sku SET image_path=?1 WHERE sku_id=?2",
        params![image_path, sku_id],
    )?;
    if n == 0 {
        return Err(CoreError::Validation(format!("SKU {sku_id} 不存在")));
    }
    Ok(())
}

/// 登记商品在线图片地址（None/空串表示清除）。
/// 只接受 https：页面直接渲染用户填写的地址，http 会被 WebView 降级或拦截，
/// 也避免明文链路被中间人替换成任意资源。
pub fn set_product_image_url(
    conn: &Connection,
    sku_id: i64,
    image_url: Option<&str>,
) -> Result<()> {
    let normalized = image_url.map(str::trim).filter(|t| !t.is_empty());
    if let Some(t) = normalized {
        if !t.starts_with("https://") {
            return Err(CoreError::Validation("图片地址必须以 https:// 开头".into()));
        }
        if t.len() > 2048 {
            return Err(CoreError::Validation(
                "图片地址过长（上限 2048 字符）".into(),
            ));
        }
        if t.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(CoreError::Validation(
                "图片地址不能包含空白或控制字符".into(),
            ));
        }
    }
    let n = conn.execute(
        "UPDATE sku SET image_url=?1 WHERE sku_id=?2",
        params![normalized, sku_id],
    )?;
    if n == 0 {
        return Err(CoreError::Validation(format!("SKU {sku_id} 不存在")));
    }
    Ok(())
}

/// 改价/改预警阈值/会员价（product.edit）。
pub fn update_product_full(
    conn: &Connection,
    sku_id: i64,
    sale_price_cents: i64,
    alert_qty: i64,
    member_price_cents: Option<i64>,
    instore_code: Option<&str>,
) -> Result<()> {
    update_product(conn, sku_id, sale_price_cents, alert_qty)?;
    conn.execute(
        "UPDATE sku SET member_price_cents=?1, instore_code=?2 WHERE sku_id=?3",
        params![member_price_cents, instore_code, sku_id],
    )?;
    Ok(())
}

/// 改价/改预警阈值（product.edit）。
pub fn update_product(
    conn: &Connection,
    sku_id: i64,
    sale_price_cents: i64,
    alert_qty: i64,
) -> Result<()> {
    if sale_price_cents < 0 || alert_qty < 0 {
        return Err(CoreError::Validation("售价/预警值非法".into()));
    }
    let n = conn.execute(
        "UPDATE sku SET sale_price_cents=?1, alert_qty=?2 WHERE sku_id=?3",
        params![sale_price_cents, alert_qty, sku_id],
    )?;
    if n == 0 {
        return Err(CoreError::Validation(format!("SKU {sku_id} 不存在")));
    }
    Ok(())
}

/// 停用/启用（SPU 级状态；停用后 lookup_product 不再命中——由查询过滤）。
pub fn set_product_status(conn: &Connection, spu_id: i64, status: i64) -> Result<()> {
    if status != 0 && status != 1 {
        return Err(CoreError::Validation("status 仅允许 0/1".into()));
    }
    let n = conn.execute(
        "UPDATE spu SET status=?1 WHERE spu_id=?2",
        params![status, spu_id],
    )?;
    if n == 0 {
        return Err(CoreError::Validation(format!("SPU {spu_id} 不存在")));
    }
    Ok(())
}

/// 库存调整（inventory.adjust）：ADJUST 事件带符号差值，盈 +
pub fn adjust_stock(
    conn: &Connection,
    sku_id: i64,
    change_qty: i64,
    reason: &str,
    operator_id: Option<i64>,
) -> Result<i64> {
    if change_qty == 0 {
        return Err(CoreError::Validation("调整数量不能为 0".into()));
    }
    if reason.trim().is_empty() {
        return Err(CoreError::Validation("调整原因不能为空（审计要求）".into()));
    }
    let allow_negative = crate::inventory::allows_negative(conn, sku_id)?;
    let new_qty = crate::inventory::apply_event(
        conn,
        sku_id,
        change_qty,
        crate::inventory::TransType::ADJUST,
        // ref_order_no 只放业务单据号，不能塞调整原因（会污染按单追溯）；
        // 原因由命令层写 audit_operation。
        None,
        operator_id,
        allow_negative,
    )?;
    Ok(new_qty)
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
    fn default_category_exists() {
        let c = setup();
        let name: String = c
            .query_row("SELECT name FROM category WHERE category_id=1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(name, "未分类");
    }

    #[test]
    fn create_and_list_product() {
        let c = setup();
        let cat = create_category(&c, None, "饮料").unwrap();
        let sku = create_product(
            &c,
            cat,
            "可乐",
            Some("6900000000001"),
            "瓶",
            300,
            150,
            false,
            100,
            10,
            None,
        )
        .unwrap();
        let rows = list_products(&c, "", 50, true).unwrap();
        assert_eq!(rows.len(), 1);
        let p = &rows[0];
        assert_eq!(p.sku_id, sku);
        assert_eq!(p.qty, 100);
        assert_eq!(p.category_name, "饮料");
        assert!(!p.is_weighed);
        // 台账可解释初始库存
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
    fn service_product_has_no_stock_ledger() {
        let c = setup();
        let sku = create_product_full(
            &c,
            1,
            "中式推拿",
            None,
            None,
            "次",
            16800,
            0,
            None,
            false,
            true,
            0,
            0,
            None,
        )
        .unwrap();
        let stock_rows: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM inv_stock WHERE sku_id=?1",
                [sku],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(stock_rows, 0);
        let rows = list_products(&c, "中式推拿", 10, true).unwrap();
        assert!(rows[0].is_service);
    }

    #[test]
    fn weighed_product_no_barcode_ok() {
        let c = setup();
        let sku = create_product(&c, 1, "散装糖", None, "克", 100, 50, true, 0, 0, None).unwrap();
        let rows = list_products(&c, "散装", 10, true).unwrap();
        assert_eq!(rows[0].sku_id, sku);
        assert!(rows[0].is_weighed);
        assert_eq!(rows[0].qty, 0);
    }

    #[test]
    fn update_price_and_status() {
        let c = setup();
        let sku = create_product(&c, 1, "A", Some("B1"), "件", 100, 50, false, 5, 1, None).unwrap();
        let spu_id: i64 = c
            .query_row("SELECT spu_id FROM sku WHERE sku_id=?1", [sku], |r| {
                r.get(0)
            })
            .unwrap();
        update_product(&c, sku, 200, 2).unwrap();
        set_product_status(&c, spu_id, 0).unwrap();
        let row = &list_products(&c, "", 10, true).unwrap()[0];
        assert_eq!(row.sale_price_cents, 200);
        assert_eq!(row.alert_qty, 2);
        assert_eq!(row.status, 0);
        assert!(set_product_status(&c, spu_id, 3).is_err());
        assert!(update_product(&c, 999, 100, 0).is_err());
    }

    #[test]
    fn adjust_stock_audited() {
        let c = setup();
        let sku = create_product(&c, 1, "A", None, "件", 100, 50, false, 50, 0, None).unwrap();
        let q = adjust_stock(&c, sku, -3, "破损报废", None).unwrap();
        assert_eq!(q, 47);
        assert!(adjust_stock(&c, sku, 0, "x", None).is_err());
        assert!(adjust_stock(&c, sku, 5, "", None).is_err());
        let t: String = c
            .query_row(
                "SELECT trans_type FROM inv_trans WHERE sku_id=?1 AND change_qty=-3",
                [sku],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(t, "ADJUST");
    }

    #[test]
    fn invalid_inputs_rejected() {
        let c = setup();
        assert!(create_product(&c, 1, "", None, "件", 100, 0, false, 0, 0, None).is_err());
        assert!(create_product(&c, 1, "X", None, "件", -1, 0, false, 0, 0, None).is_err());
        assert!(create_product(&c, 999, "X", None, "件", 100, 0, false, 0, 0, None).is_err());
        assert!(create_category(&c, None, " ").is_err());
        assert!(create_category(&c, Some(999), "Y").is_err());
    }
}
