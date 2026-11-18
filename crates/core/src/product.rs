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
    pub status: i64,
    pub qty: i64,
    pub alert_qty: i64,
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
    if name.trim().is_empty() {
        return Err(CoreError::Validation("商品名不能为空".into()));
    }
    if sale_price_cents <= 0 {
        return Err(CoreError::Validation("售价必须大于 0".into()));
    }
    if cost_price_cents < 0 || initial_qty < 0 || alert_qty < 0 {
        return Err(CoreError::Validation("成本/库存/预警值不能为负".into()));
    }
    let cat_exists: Option<i64> = conn
        .query_row(
            "SELECT category_id FROM category WHERE category_id=?1",
            [category_id],
            |r| r.get(0),
        )
        .optional()?;
    if cat_exists.is_none() {
        return Err(CoreError::Validation(format!("分类 {category_id} 不存在")));
    }
    let code = format!("SPU{}", Utc::now().timestamp_millis());
    conn.execute(
        "INSERT INTO spu(category_id,spu_name,spu_code,unit) VALUES (?1,?2,?3,?4)",
        params![category_id, name.trim(), code, unit],
    )?;
    let spu_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO sku(spu_id,barcode,sale_price_cents,cost_price_cents,is_weighed,alert_qty)
         VALUES (?1,?2,?3,?4,?5,?6)",
        params![
            spu_id,
            barcode,
            sale_price_cents,
            cost_price_cents,
            is_weighed as i64,
            alert_qty
        ],
    )?;
    let sku_id = conn.last_insert_rowid();
    if initial_qty > 0 {
        crate::inventory::apply_event(
            conn,
            sku_id,
            initial_qty,
            crate::inventory::TransType::PURCHASE_IN,
            Some(TRANS_INITIAL),
            operator_id,
            false,
        )?;
    } else {
        // 零库存也建立台账行，便于预警与盘点
        conn.execute(
            "INSERT INTO inv_stock(sku_id,warehouse_type,qty,version) VALUES (?1,'STORE',0,1)",
            [sku_id],
        )?;
    }
    Ok(sku_id)
}

/// 商品列表：关键字模糊匹配 + 全量库存（F-15 预警由前端按 qty<=alert_qty 标红）。
pub fn list_products(conn: &Connection, keyword: &str, limit: i64) -> Result<Vec<ProductRow>> {
    let pattern = format!("%{keyword}%");
    let mut stmt = conn.prepare(
        "SELECT s.sku_id, p.spu_id, p.spu_name, s.barcode, p.category_id, COALESCE(c.name,'未分类'),
                p.unit, s.sale_price_cents, s.cost_price_cents, s.is_weighed, p.status,
                COALESCE(i.qty,0), s.alert_qty, s.member_price_cents, s.instore_code
         FROM sku s
         JOIN spu p ON p.spu_id = s.spu_id
         LEFT JOIN category c ON c.category_id = p.category_id
         LEFT JOIN inv_stock i ON i.sku_id = s.sku_id AND i.warehouse_type='STORE'
         WHERE (?1 = '' OR s.barcode LIKE ?2 OR p.spu_name LIKE ?2)
         ORDER BY p.spu_id DESC LIMIT ?3",
    )?;
    let rows = stmt.query_map(params![keyword, pattern, limit], |r| {
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
        })
    })?;
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
    initial_qty: i64,
    alert_qty: i64,
    operator_id: Option<i64>,
) -> Result<i64> {
    if is_weighed && instore_code.is_none() {
        return Err(CoreError::Validation("称重商品必须填写店内码".into()));
    }
    let sku_id = create_product(
        conn,
        category_id,
        name,
        barcode,
        unit,
        sale_price_cents,
        cost_price_cents,
        is_weighed,
        initial_qty,
        alert_qty,
        operator_id,
    )?;
    conn.execute(
        "UPDATE sku SET instore_code=?1, member_price_cents=?2 WHERE sku_id=?3",
        params![instore_code, member_price_cents, sku_id],
    )?;
    Ok(sku_id)
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
    if sale_price_cents <= 0 || alert_qty < 0 {
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
        Some(reason.trim()),
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
        let rows = list_products(&c, "", 50).unwrap();
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
    fn weighed_product_no_barcode_ok() {
        let c = setup();
        let sku = create_product(&c, 1, "散装糖", None, "克", 100, 50, true, 0, 0, None).unwrap();
        let rows = list_products(&c, "散装", 10).unwrap();
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
        let row = &list_products(&c, "", 10).unwrap()[0];
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
        assert!(create_product(&c, 1, "X", None, "件", 0, 0, false, 0, 0, None).is_err());
        assert!(create_product(&c, 999, "X", None, "件", 100, 0, false, 0, 0, None).is_err());
        assert!(create_category(&c, None, " ").is_err());
        assert!(create_category(&c, Some(999), "Y").is_err());
    }
}
