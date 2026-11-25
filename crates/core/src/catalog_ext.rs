//! 行业商品目录扩展（M0020）：规格矩阵、母婴月龄与批次效期、餐饮套餐与加料选项组。
//!
//! 设计边界（避免做成「假界面」）：
//! - 套餐本身是一个普通 SKU（通常建为服务项目），价格由该 SKU 维护；
//!   combo_item 只记录厨打/备餐展开明细，不参与计价。
//! - 加料/规格选项关联的是真实 SKU，选中后作为独立购物车行计价，价格由服务端读取 SKU。
//! - 批次只做效期登记与查询，不参与库存自动出库扣减（FEFO 需另立库存模型，不在本模块承诺）。

use crate::error::{CoreError, Result};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};

// ───────────────────────────── 服装：款×色×码 ─────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct SpecValue {
    pub spec_value_id: i64,
    pub spec_group_id: i64,
    pub value: String,
    pub sort_order: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SpecGroup {
    pub spec_group_id: i64,
    pub spu_id: i64,
    pub name: String,
    pub sort_order: i64,
    pub values: Vec<SpecValue>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SkuVariant {
    pub sku_id: i64,
    pub spu_id: i64,
    pub name: String,
    /// 由规格组/值生成的快照，如 {"颜色":"黑","尺码":"M"}
    pub spec_json: String,
    /// 面向收银员的展示串，如 颜色:黑 / 尺码:M
    pub spec_text: String,
    pub sale_price_cents: i64,
    pub cost_price_cents: i64,
    pub member_price_cents: Option<i64>,
    pub barcode: Option<String>,
    pub instore_code: Option<String>,
    pub qty: i64,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct VariantInput {
    /// 每个规格组必须且只能选一个规格值
    pub spec_value_ids: Vec<i64>,
    pub sale_price_cents: i64,
    #[serde(default)]
    pub cost_price_cents: i64,
    pub member_price_cents: Option<i64>,
    pub barcode: Option<String>,
    pub instore_code: Option<String>,
    #[serde(default)]
    pub initial_qty: i64,
}

pub fn spec_text_from_json(spec_json: &str) -> String {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(spec_json) else {
        return String::new();
    };
    let Some(map) = v.as_object() else {
        return String::new();
    };
    map.iter()
        .map(|(k, val)| {
            let value = val.as_str().unwrap_or("");
            format!("{k}:{value}")
        })
        .collect::<Vec<_>>()
        .join(" / ")
}

/// 新增规格组（如「颜色」「尺码」）；同 SPU 下同名不可重复。
pub fn create_spec_group(conn: &Connection, spu_id: i64, name: String) -> Result<i64> {
    let name = name.trim();
    if name.is_empty() {
        return Err(CoreError::Validation("规格组名不能为空".into()));
    }
    let exists: Option<i64> = conn
        .query_row("SELECT spu_id FROM spu WHERE spu_id=?1", [spu_id], |r| {
            r.get(0)
        })
        .optional()?;
    if exists.is_none() {
        return Err(CoreError::Validation(format!("商品 {spu_id} 不存在")));
    }
    let order: i64 = conn.query_row(
        "SELECT COALESCE(MAX(sort_order),0)+1 FROM spu_spec_group WHERE spu_id=?1",
        [spu_id],
        |r| r.get(0),
    )?;
    conn.execute(
        "INSERT INTO spu_spec_group(spu_id,name,sort_order) VALUES (?1,?2,?3)",
        params![spu_id, name, order],
    )?;
    Ok(conn.last_insert_rowid())
}

/// 给规格组新增一个可选值（如「黑」「M」）。
pub fn add_spec_value(conn: &Connection, spec_group_id: i64, value: String) -> Result<i64> {
    let value = value.trim();
    if value.is_empty() {
        return Err(CoreError::Validation("规格值不能为空".into()));
    }
    let order: i64 = conn.query_row(
        "SELECT COALESCE(MAX(sort_order),0)+1 FROM spu_spec_value WHERE spec_group_id=?1",
        [spec_group_id],
        |r| r.get(0),
    )?;
    conn.execute(
        "INSERT INTO spu_spec_value(spec_group_id,value,sort_order) VALUES (?1,?2,?3)",
        params![spec_group_id, value, order],
    )?;
    Ok(conn.last_insert_rowid())
}

/// 按 SPU 返回完整规格树，供商品档案编辑与 POS 规格筛选共用。
pub fn list_spec_groups(conn: &Connection, spu_id: i64) -> Result<Vec<SpecGroup>> {
    let mut stmt = conn.prepare(
        "SELECT spec_group_id,spu_id,name,sort_order FROM spu_spec_group
         WHERE spu_id=?1 ORDER BY sort_order,spec_group_id",
    )?;
    let groups = stmt
        .query_map([spu_id], |r| {
            Ok(SpecGroup {
                spec_group_id: r.get(0)?,
                spu_id: r.get(1)?,
                name: r.get(2)?,
                sort_order: r.get(3)?,
                values: Vec::new(),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);

    let mut out = Vec::new();
    let mut vs = conn.prepare(
        "SELECT spec_value_id,spec_group_id,value,sort_order FROM spu_spec_value
         WHERE spec_group_id=?1 ORDER BY sort_order,spec_value_id",
    )?;
    for mut g in groups {
        let values = vs
            .query_map([g.spec_group_id], |r| {
                Ok(SpecValue {
                    spec_value_id: r.get(0)?,
                    spec_group_id: r.get(1)?,
                    value: r.get(2)?,
                    sort_order: r.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        g.values = values;
        out.push(g);
    }
    Ok(out)
}

/// 批量生成 SKU 变体：每个变体必须有完整且不重复的规格值组合。
pub fn create_sku_variants(
    conn: &Connection,
    spu_id: i64,
    variants: &[VariantInput],
) -> Result<Vec<i64>> {
    if variants.is_empty() {
        return Err(CoreError::Validation("至少填写一个规格组合".into()));
    }
    let spu_exists: Option<i64> = conn
        .query_row("SELECT spu_id FROM spu WHERE spu_id=?1", [spu_id], |r| {
            r.get(0)
        })
        .optional()?;
    if spu_exists.is_none() {
        return Err(CoreError::Validation(format!("商品 {spu_id} 不存在")));
    }
    let group_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM spu_spec_group WHERE spu_id=?1",
        [spu_id],
        |r| r.get(0),
    )?;
    if group_count == 0 {
        return Err(CoreError::Validation("请先维护规格组与规格值".into()));
    }

    let tx = conn.unchecked_transaction()?;
    let mut created = Vec::new();
    let mut seen: Vec<Vec<i64>> = Vec::new();

    for v in variants {
        if v.sale_price_cents < 0 || v.cost_price_cents < 0 || v.initial_qty < 0 {
            return Err(CoreError::Validation("价格/成本/库存不能为负".into()));
        }
        if let Some(mp) = v.member_price_cents {
            if mp < 0 || mp > v.sale_price_cents {
                return Err(CoreError::Validation("会员价必须在 0 与售价之间".into()));
            }
        }
        if v.spec_value_ids.len() != group_count as usize {
            return Err(CoreError::Validation(format!(
                "每个变体必须选择全部 {group_count} 个规格组"
            )));
        }
        let mut sorted = v.spec_value_ids.clone();
        sorted.sort_unstable();
        let len_before_dedup = sorted.len();
        sorted.dedup();
        if sorted.len() != len_before_dedup {
            return Err(CoreError::Validation("同一规格组不能重复选择".into()));
        }
        if seen.contains(&sorted) {
            return Err(CoreError::Validation("规格组合重复，请检查变体列表".into()));
        }
        seen.push(sorted.clone());

        // 规格值必须属于该 SPU，否则规格快照会出现「张冠李戴」。
        let placeholders = sorted
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 2))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT COUNT(*) FROM spu_spec_value sv
             JOIN spu_spec_group g ON g.spec_group_id=sv.spec_group_id
             WHERE g.spu_id=?1 AND sv.spec_value_id IN ({placeholders})"
        );
        let valid_count: i64 = {
            let mut stmt = tx.prepare(&sql)?;
            let mut all_params: Vec<rusqlite::types::Value> =
                vec![rusqlite::types::Value::Integer(spu_id)];
            for id in &sorted {
                all_params.push(rusqlite::types::Value::Integer(*id));
            }
            stmt.query_row(rusqlite::params_from_iter(all_params), |r| r.get(0))?
        };
        if valid_count != sorted.len() as i64 {
            return Err(CoreError::Validation(
                "规格值不属于该商品，请重新选择".into(),
            ));
        }

        for (field, code) in [
            ("条码", v.barcode.as_deref()),
            ("店内码", v.instore_code.as_deref()),
        ] {
            if let Some(code) = code.map(str::trim).filter(|c| !c.is_empty()) {
                let dup: Option<i64> = tx
                    .query_row(
                        &format!(
                            "SELECT sku_id FROM sku WHERE {}=?1",
                            if field == "条码" {
                                "barcode"
                            } else {
                                "instore_code"
                            }
                        ),
                        [code],
                        |r| r.get(0),
                    )
                    .optional()?;
                if dup.is_some() {
                    return Err(CoreError::Validation(format!("{field} {code} 已存在")));
                }
            }
        }

        let mut spec_map = serde_json::Map::new();
        for id in &sorted {
            let (gname, val): (String, String) = tx.query_row(
                "SELECT g.name,sv.value FROM spu_spec_value sv
                 JOIN spu_spec_group g ON g.spec_group_id=sv.spec_group_id
                 WHERE sv.spec_value_id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            spec_map.insert(gname, serde_json::Value::String(val));
        }
        let spec_json = serde_json::to_string(&serde_json::Value::Object(spec_map))?;

        tx.execute(
            "INSERT INTO sku(spu_id,barcode,instore_code,spec_json,sale_price_cents,
                             cost_price_cents,member_price_cents,is_service,alert_qty)
             VALUES (?1,?2,?3,?4,?5,?6,?7,0,0)",
            params![
                spu_id,
                v.barcode
                    .as_deref()
                    .map(str::trim)
                    .filter(|b| !b.is_empty()),
                v.instore_code
                    .as_deref()
                    .map(str::trim)
                    .filter(|c| !c.is_empty()),
                spec_json,
                v.sale_price_cents,
                v.cost_price_cents,
                v.member_price_cents,
            ],
        )?;
        let sku_id = tx.last_insert_rowid();
        for id in &sorted {
            tx.execute(
                "INSERT INTO sku_spec(sku_id,spec_value_id) VALUES (?1,?2)",
                params![sku_id, id],
            )?;
        }
        if v.initial_qty > 0 {
            crate::inventory::apply_event(
                &tx,
                sku_id,
                v.initial_qty,
                crate::inventory::TransType::PURCHASE_IN,
                Some(crate::constants::TRANS_INITIAL),
                None,
                false,
            )?;
        } else {
            tx.execute(
                "INSERT INTO inv_stock(sku_id,warehouse_type,qty,version) VALUES (?1,'STORE',0,1)",
                [sku_id],
            )?;
        }
        created.push(sku_id);
    }
    tx.commit()?;
    Ok(created)
}

/// 列出一个 SPU 下的全部变体（含基础 SKU）。
pub fn list_sku_variants(conn: &Connection, spu_id: i64) -> Result<Vec<SkuVariant>> {
    let mut stmt = conn.prepare(
        "SELECT s.sku_id,s.spu_id,p.spu_name,COALESCE(s.spec_json,'{}'),
                s.sale_price_cents,s.cost_price_cents,s.member_price_cents,
                s.barcode,s.instore_code,COALESCE(st.qty,0)
         FROM sku s JOIN spu p ON p.spu_id=s.spu_id
         LEFT JOIN inv_stock st ON st.sku_id=s.sku_id AND st.warehouse_type='STORE'
         WHERE s.spu_id=?1 ORDER BY s.sku_id",
    )?;
    let rows = stmt.query_map([spu_id], |r| {
        let spec_json: String = r.get(3)?;
        Ok(SkuVariant {
            sku_id: r.get(0)?,
            spu_id: r.get(1)?,
            name: r.get(2)?,
            spec_text: spec_text_from_json(&spec_json),
            spec_json,
            sale_price_cents: r.get(4)?,
            cost_price_cents: r.get(5)?,
            member_price_cents: r.get(6)?,
            barcode: r.get(7)?,
            instore_code: r.get(8)?,
            qty: r.get(9)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

// ───────────────────────────── 母婴：月龄与批次效期 ─────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct SkuBatch {
    pub batch_id: i64,
    pub sku_id: i64,
    pub batch_no: String,
    pub production_date: Option<String>,
    pub expiry_date: Option<String>,
    pub qty: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AgeProduct {
    pub sku_id: i64,
    pub name: String,
    pub category_name: String,
    pub unit: String,
    pub price_cents: i64,
    pub member_price_cents: Option<i64>,
    pub month_min: Option<i64>,
    pub month_max: Option<i64>,
    pub shelf_life_days: i64,
    /// 该 SKU 最早批次效期，仅作临期提示，不参与库存扣减
    pub expiry_date: Option<String>,
    pub qty: i64,
}

/// 设置 SKU 适用月龄段与保质期天数。None 表示不限定/未维护。
pub fn set_product_age_range(
    conn: &Connection,
    sku_id: i64,
    month_min: Option<i64>,
    month_max: Option<i64>,
    shelf_life_days: i64,
) -> Result<()> {
    if let (Some(a), Some(b)) = (month_min, month_max) {
        if a < 0 || b > 240 || a > b {
            return Err(CoreError::Validation("月龄区间非法（0-240 月）".into()));
        }
    }
    if shelf_life_days < 0 {
        return Err(CoreError::Validation("保质期天数不能为负".into()));
    }
    let exists: Option<i64> = conn
        .query_row("SELECT sku_id FROM sku WHERE sku_id=?1", [sku_id], |r| {
            r.get(0)
        })
        .optional()?;
    if exists.is_none() {
        return Err(CoreError::Validation(format!("商品 {sku_id} 不存在")));
    }
    conn.execute(
        "UPDATE sku SET month_min=?2,month_max=?3,shelf_life_days=?4 WHERE sku_id=?1",
        params![sku_id, month_min, month_max, shelf_life_days],
    )?;
    Ok(())
}

/// 登记批次效期。批次是登记台账，不与 inv_stock 自动联动。
pub fn create_sku_batch(
    conn: &Connection,
    sku_id: i64,
    batch_no: &str,
    production_date: Option<&str>,
    expiry_date: Option<&str>,
    qty: i64,
) -> Result<i64> {
    let batch_no = batch_no.trim();
    if batch_no.is_empty() {
        return Err(CoreError::Validation("批号不能为空".into()));
    }
    if qty < 0 {
        return Err(CoreError::Validation("批次数量不能为负".into()));
    }
    for (name, d) in [("生产日期", production_date), ("失效日期", expiry_date)] {
        if let Some(d) = d {
            if d.len() != 10 || d.chars().filter(|c| *c == '-').count() != 2 {
                return Err(CoreError::Validation(format!("{name} 需为 YYYY-MM-DD")));
            }
        }
    }
    let exists: Option<i64> = conn
        .query_row("SELECT sku_id FROM sku WHERE sku_id=?1", [sku_id], |r| {
            r.get(0)
        })
        .optional()?;
    if exists.is_none() {
        return Err(CoreError::Validation(format!("商品 {sku_id} 不存在")));
    }
    conn.execute(
        "INSERT INTO sku_batch(sku_id,batch_no,production_date,expiry_date,qty,created_at)
         VALUES (?1,?2,?3,?4,?5,?6)",
        params![
            sku_id,
            batch_no,
            production_date,
            expiry_date,
            qty,
            Utc::now().to_rfc3339()
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_sku_batches(conn: &Connection, sku_id: i64) -> Result<Vec<SkuBatch>> {
    let mut stmt = conn.prepare(
        "SELECT batch_id,sku_id,batch_no,production_date,expiry_date,qty FROM sku_batch
         WHERE sku_id=?1 ORDER BY expiry_date,batch_id",
    )?;
    let rows = stmt.query_map([sku_id], |r| {
        Ok(SkuBatch {
            batch_id: r.get(0)?,
            sku_id: r.get(1)?,
            batch_no: r.get(2)?,
            production_date: r.get(3)?,
            expiry_date: r.get(4)?,
            qty: r.get(5)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

/// 按月龄筛选在售商品。month=0 表示不筛选（返回全部）。
pub fn list_products_by_age(conn: &Connection, month: i64, limit: i64) -> Result<Vec<AgeProduct>> {
    let limit = limit.clamp(1, 500);
    let mut stmt = conn.prepare(
        "SELECT s.sku_id,p.spu_name,COALESCE(c.name,'未分类'),p.unit,
                s.sale_price_cents,s.member_price_cents,s.month_min,s.month_max,
                s.shelf_life_days,
                (SELECT MIN(b.expiry_date) FROM sku_batch b WHERE b.sku_id=s.sku_id),
                COALESCE(st.qty,0)
         FROM sku s JOIN spu p ON p.spu_id=s.spu_id
         LEFT JOIN category c ON c.category_id=p.category_id
         LEFT JOIN inv_stock st ON st.sku_id=s.sku_id AND st.warehouse_type='STORE'
         WHERE p.status=1
           AND (?1=0 OR (s.month_min IS NOT NULL AND s.month_min<=?1
                          AND (s.month_max IS NULL OR s.month_max>=?1)))
         ORDER BY s.month_min,s.sku_id LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![month, limit], |r| {
        Ok(AgeProduct {
            sku_id: r.get(0)?,
            name: r.get(1)?,
            category_name: r.get(2)?,
            unit: r.get(3)?,
            price_cents: r.get(4)?,
            member_price_cents: r.get(5)?,
            month_min: r.get(6)?,
            month_max: r.get(7)?,
            shelf_life_days: r.get(8)?,
            expiry_date: r.get(9)?,
            qty: r.get(10)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

// ───────────────────────────── 餐饮：套餐与加料 ─────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct ComboItem {
    pub sku_id: i64,
    pub name: String,
    pub qty: i64,
    pub unit_price_cents: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Combo {
    pub combo_id: i64,
    pub sku_id: i64,
    pub name: String,
    pub note: Option<String>,
    /// 套餐售价取自套餐 SKU，不由明细累加
    pub price_cents: i64,
    pub items: Vec<ComboItem>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ComboItemInput {
    pub sku_id: i64,
    pub qty: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct OptionItem {
    pub option_item_id: i64,
    pub sku_id: i64,
    pub name: String,
    pub price_cents: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct OptionGroup {
    pub group_id: i64,
    pub name: String,
    pub scene: String,
    pub items: Vec<OptionItem>,
}

/// 把一个已存在的 SKU 登记为套餐主体。
pub fn create_combo(conn: &Connection, sku_id: i64, note: Option<&str>) -> Result<i64> {
    let exists: Option<i64> = conn
        .query_row("SELECT sku_id FROM sku WHERE sku_id=?1", [sku_id], |r| {
            r.get(0)
        })
        .optional()?;
    if exists.is_none() {
        return Err(CoreError::Validation(format!("套餐商品 {sku_id} 不存在")));
    }
    conn.execute(
        "INSERT INTO combo(sku_id,note,created_at) VALUES (?1,?2,?3)",
        params![sku_id, note, Utc::now().to_rfc3339()],
    )?;
    Ok(conn.last_insert_rowid())
}

/// 覆盖式设置套餐明细；明细只用于备餐/厨打，不参与计价。
pub fn set_combo_items(conn: &Connection, combo_id: i64, items: &[ComboItemInput]) -> Result<()> {
    if items.is_empty() {
        return Err(CoreError::Validation("套餐至少包含一个明细商品".into()));
    }
    let exists: Option<i64> = conn
        .query_row(
            "SELECT combo_id FROM combo WHERE combo_id=?1",
            [combo_id],
            |r| r.get(0),
        )
        .optional()?;
    if exists.is_none() {
        return Err(CoreError::Validation(format!("套餐 {combo_id} 不存在")));
    }
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM combo_item WHERE combo_id=?1", [combo_id])?;
    for item in items {
        if item.qty <= 0 || item.qty > 99 {
            return Err(CoreError::Validation("套餐明细数量须在 1-99 之间".into()));
        }
        let ok: Option<i64> = tx
            .query_row(
                "SELECT sku_id FROM sku WHERE sku_id=?1",
                [item.sku_id],
                |r| r.get(0),
            )
            .optional()?;
        if ok.is_none() {
            return Err(CoreError::Validation(format!(
                "明细商品 {} 不存在",
                item.sku_id
            )));
        }
        tx.execute(
            "INSERT INTO combo_item(combo_id,sku_id,qty) VALUES (?1,?2,?3)",
            params![combo_id, item.sku_id, item.qty],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub fn list_combos(conn: &Connection) -> Result<Vec<Combo>> {
    let mut stmt = conn.prepare(
        "SELECT c.combo_id,c.sku_id,p.spu_name,c.note,s.sale_price_cents
         FROM combo c JOIN sku s ON s.sku_id=c.sku_id JOIN spu p ON p.spu_id=s.spu_id
         ORDER BY c.combo_id",
    )?;
    let combos = stmt
        .query_map([], |r| {
            Ok(Combo {
                combo_id: r.get(0)?,
                sku_id: r.get(1)?,
                name: r.get(2)?,
                note: r.get(3)?,
                price_cents: r.get(4)?,
                items: Vec::new(),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);

    let mut out = Vec::new();
    let mut is = conn.prepare(
        "SELECT ci.sku_id,p.spu_name,ci.qty,s.sale_price_cents
         FROM combo_item ci JOIN sku s ON s.sku_id=ci.sku_id JOIN spu p ON p.spu_id=s.spu_id
         WHERE ci.combo_id=?1 ORDER BY ci.rowid",
    )?;
    for mut combo in combos {
        let items = is
            .query_map([combo.combo_id], |r| {
                Ok(ComboItem {
                    sku_id: r.get(0)?,
                    name: r.get(1)?,
                    qty: r.get(2)?,
                    unit_price_cents: r.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        combo.items = items;
        out.push(combo);
    }
    Ok(out)
}

/// 厨打/备餐展开：按套餐 SKU 查明细。非套餐返回空列表。
pub fn combo_items_by_sku(conn: &Connection, sku_id: i64) -> Result<Vec<ComboItem>> {
    let mut stmt = conn.prepare(
        "SELECT ci.sku_id,p.spu_name,ci.qty,s.sale_price_cents
         FROM combo c JOIN combo_item ci ON ci.combo_id=c.combo_id
         JOIN sku s ON s.sku_id=ci.sku_id JOIN spu p ON p.spu_id=s.spu_id
         WHERE c.sku_id=?1 ORDER BY ci.rowid",
    )?;
    let rows = stmt.query_map([sku_id], |r| {
        Ok(ComboItem {
            sku_id: r.get(0)?,
            name: r.get(1)?,
            qty: r.get(2)?,
            unit_price_cents: r.get(3)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

pub fn create_option_group(conn: &Connection, name: &str, scene: &str) -> Result<i64> {
    let name = name.trim();
    if name.is_empty() {
        return Err(CoreError::Validation("选项组名不能为空".into()));
    }
    conn.execute(
        "INSERT INTO option_group(name,scene,sort_order)
         VALUES (?1,?2,(SELECT COALESCE(MAX(sort_order),0)+1 FROM option_group WHERE scene=?2))",
        params![name, scene],
    )?;
    Ok(conn.last_insert_rowid())
}

/// 覆盖式设置选项组的候选 SKU（加料本身是独立商品，价格由 SKU 维护）。
pub fn set_option_items(conn: &Connection, group_id: i64, sku_ids: &[i64]) -> Result<()> {
    let exists: Option<i64> = conn
        .query_row(
            "SELECT group_id FROM option_group WHERE group_id=?1",
            [group_id],
            |r| r.get(0),
        )
        .optional()?;
    if exists.is_none() {
        return Err(CoreError::Validation(format!("选项组 {group_id} 不存在")));
    }
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM option_item WHERE group_id=?1", [group_id])?;
    for sku_id in sku_ids {
        let ok: Option<i64> = tx
            .query_row("SELECT sku_id FROM sku WHERE sku_id=?1", [sku_id], |r| {
                r.get(0)
            })
            .optional()?;
        if ok.is_none() {
            return Err(CoreError::Validation(format!("选项商品 {sku_id} 不存在")));
        }
        tx.execute(
            "INSERT INTO option_item(group_id,sku_id,sort_order)
             VALUES (?1,?2,(SELECT COALESCE(MAX(sort_order),0)+1 FROM option_item WHERE group_id=?1))",
            params![group_id, sku_id],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// scene 为空时返回全部行业选项组。
pub fn list_option_groups(conn: &Connection, scene: Option<&str>) -> Result<Vec<OptionGroup>> {
    let mut stmt = conn.prepare(
        "SELECT group_id,name,scene FROM option_group
         WHERE (?1 IS NULL OR scene=?1) ORDER BY sort_order,group_id",
    )?;
    let groups = stmt
        .query_map([scene], |r| {
            Ok(OptionGroup {
                group_id: r.get(0)?,
                name: r.get(1)?,
                scene: r.get(2)?,
                items: Vec::new(),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);

    let mut out = Vec::new();
    let mut is = conn.prepare(
        "SELECT oi.option_item_id,oi.sku_id,p.spu_name,s.sale_price_cents
         FROM option_item oi JOIN sku s ON s.sku_id=oi.sku_id JOIN spu p ON p.spu_id=s.spu_id
         WHERE oi.group_id=?1 ORDER BY oi.sort_order,oi.option_item_id",
    )?;
    for mut g in groups {
        let items = is
            .query_map([g.group_id], |r| {
                Ok(OptionItem {
                    option_item_id: r.get(0)?,
                    sku_id: r.get(1)?,
                    name: r.get(2)?,
                    price_cents: r.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        g.items = items;
        out.push(g);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{open_in_memory, run_migrations};

    fn setup() -> Connection {
        let mut c = open_in_memory().unwrap();
        run_migrations(&mut c).unwrap();
        c.execute(
            "INSERT INTO category(category_id,name) VALUES (1,'服装')",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO spu(spu_id,category_id,spu_name,spu_code,unit) VALUES (1,1,'测试外套','T1','件')",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO sku(sku_id,spu_id,sale_price_cents) VALUES (1,1,10000)",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO inv_stock(sku_id,warehouse_type,qty,version) VALUES (1,'STORE',0,1)",
            [],
        )
        .unwrap();
        c
    }

    #[test]
    fn sku_variants_require_full_spec_and_generate_snapshot() {
        let c = setup();
        let g1 = create_spec_group(&c, 1, "颜色".into()).unwrap();
        let g2 = create_spec_group(&c, 1, "尺码".into()).unwrap();
        let black = add_spec_value(&c, g1, "黑".into()).unwrap();
        let m = add_spec_value(&c, g2, "M".into()).unwrap();
        let l = add_spec_value(&c, g2, "L".into()).unwrap();

        let ids = create_sku_variants(
            &c,
            1,
            &[
                VariantInput {
                    spec_value_ids: vec![black, m],
                    sale_price_cents: 12000,
                    cost_price_cents: 0,
                    member_price_cents: None,
                    barcode: None,
                    instore_code: None,
                    initial_qty: 2,
                },
                VariantInput {
                    spec_value_ids: vec![black, l],
                    sale_price_cents: 12000,
                    cost_price_cents: 0,
                    member_price_cents: None,
                    barcode: None,
                    instore_code: None,
                    initial_qty: 0,
                },
            ],
        )
        .unwrap();
        assert_eq!(ids.len(), 2);

        let variants = list_sku_variants(&c, 1).unwrap();
        assert_eq!(variants.len(), 3); // 基础 SKU + 两个变体
        let v = variants.iter().find(|v| v.sku_id == ids[0]).unwrap();
        assert!(v.spec_text.contains("颜色:黑"));
        assert!(v.spec_text.contains("尺码:M"));
        assert_eq!(v.qty, 2);

        // 少选一个规格组必须拒绝，避免不完整变体流入 POS
        let bad = create_sku_variants(
            &c,
            1,
            &[VariantInput {
                spec_value_ids: vec![black],
                sale_price_cents: 1,
                cost_price_cents: 0,
                member_price_cents: None,
                barcode: None,
                instore_code: None,
                initial_qty: 0,
            }],
        );
        assert!(bad.is_err());
    }

    #[test]
    fn age_filter_and_batch_expiry_are_recorded() {
        let c = setup();
        set_product_age_range(&c, 1, Some(6), Some(12), 540).unwrap();
        create_sku_batch(&c, 1, "B2401", Some("2024-01-01"), Some("2025-07-05"), 10).unwrap();

        let six = list_products_by_age(&c, 6, 20).unwrap();
        assert_eq!(six.len(), 1);
        assert_eq!(six[0].expiry_date.as_deref(), Some("2025-07-05"));
        let thirty = list_products_by_age(&c, 30, 20).unwrap();
        assert!(thirty.is_empty());
        let all = list_products_by_age(&c, 0, 20).unwrap();
        assert_eq!(all.len(), 1);
    }

    #[test]
    fn combo_price_comes_from_sku_not_items() {
        let c = setup();
        c.execute(
            "INSERT INTO sku(sku_id,spu_id,sale_price_cents,is_service) VALUES (2,1,3000,1)",
            [],
        )
        .unwrap();
        let combo_id = create_combo(&c, 1, Some("套餐说明")).unwrap();
        set_combo_items(&c, combo_id, &[ComboItemInput { sku_id: 2, qty: 1 }]).unwrap();
        let combos = list_combos(&c).unwrap();
        assert_eq!(combos[0].price_cents, 10000);
        assert_eq!(combos[0].items.len(), 1);
        // 明细累加 3000 不能覆盖套餐 SKU 售价 10000
        assert_ne!(combos[0].price_cents, combos[0].items[0].unit_price_cents);
    }

    #[test]
    fn option_items_are_real_skus() {
        let c = setup();
        c.execute(
            "INSERT INTO sku(sku_id,spu_id,sale_price_cents,is_service) VALUES (2,1,500,1)",
            [],
        )
        .unwrap();
        let gid = create_option_group(&c, "加料", "RESTAURANT").unwrap();
        set_option_items(&c, gid, &[2]).unwrap();
        let groups = list_option_groups(&c, Some("RESTAURANT")).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].items[0].price_cents, 500);
    }
}
