//! 多行业适配与门店档案（行业维度）。
//!
//! 行业模板预置在 category_template 表（M0009），选定行业时按模板路径
//! 逐级写入 category 表（幂等：同名同级不重复插入）。
//! 门店档案（店名 + 行业）落 system_config，界面与票据统一消费。

use crate::constants::{
    CFG_INDUSTRY, CFG_STORE_NAME, INDUSTRIES, INDUSTRY_CLOTHING, INDUSTRY_RESTAURANT,
    INDUSTRY_SUPERMARKET,
};
use crate::error::{CoreError, Result};
use rusqlite::{params, Connection};

/// 行业选项（启动选择页与设置页共用）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct IndustryOption {
    pub id: String,
    pub name: String,
    pub description: String,
    /// 该行业默认计量单位。
    pub default_unit: String,
    /// 是否默认启用称重商品（生鲜/散装场景）。
    pub supports_weighed: bool,
}

/// 门店档案。
#[derive(Debug, Clone, serde::Serialize)]
pub struct StoreProfile {
    /// 行业标识；未选择时为空串。
    pub industry: String,
    /// 行业中文名；未选择时为空串。
    pub industry_name: String,
    pub store_name: String,
    /// 是否已完成初始化（行业已选且店名非空）。
    pub configured: bool,
}

/// 行业静态元数据。分类树在数据库中，本表只放「界面语义」——
/// 名称、说明、默认单位与称重能力。
fn industry_meta(id: &str) -> Option<(&'static str, &'static str, &'static str, bool)> {
    match id {
        INDUSTRY_SUPERMARKET => Some((
            "超市便利店",
            "生鲜日用全品类，含称重商品与散装称重",
            "件",
            true,
        )),
        INDUSTRY_CLOTHING => Some((
            "服装鞋帽",
            "服饰鞋包按男女童分类，规格属性驱动",
            "件",
            false,
        )),
        INDUSTRY_RESTAURANT => Some((
            "餐饮门店",
            "菜品按中餐小吃饮品分类，支持堂食外带",
            "份",
            true,
        )),
        _ => None,
    }
}

/// 列出全部行业选项（顺序为选择页展示顺序）。
pub fn list_industries() -> Vec<IndustryOption> {
    INDUSTRIES
        .iter()
        .filter_map(|id| {
            industry_meta(id).map(|(name, desc, unit, weighed)| IndustryOption {
                id: (*id).to_string(),
                name: name.to_string(),
                description: desc.to_string(),
                default_unit: unit.to_string(),
                supports_weighed: weighed,
            })
        })
        .collect()
}

/// 读取配置值（系统配置表的薄封装）。
fn cfg_get(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row(
        "SELECT config_value FROM system_config WHERE config_key=?1",
        [key],
        |r| r.get::<_, String>(0),
    )
    .ok()
}

/// 写入配置值。
fn cfg_set(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO system_config(config_key,config_value,updated_at) VALUES (?1,?2,?3)
         ON CONFLICT(config_key) DO UPDATE SET config_value=excluded.config_value, updated_at=excluded.updated_at",
        params![key, value, crate::time::format_utc(&chrono::Utc::now())],
    )?;
    Ok(())
}

/// 门店档案：行业 + 店名。
pub fn store_profile(conn: &Connection) -> Result<StoreProfile> {
    let industry = cfg_get(conn, CFG_INDUSTRY).unwrap_or_default();
    let store_name = cfg_get(conn, CFG_STORE_NAME).unwrap_or_default();
    let industry_name = industry_meta(&industry)
        .map(|(name, _, _, _)| name.to_string())
        .unwrap_or_default();
    Ok(StoreProfile {
        configured: !industry.is_empty() && !store_name.trim().is_empty(),
        industry,
        industry_name,
        store_name,
    })
}

/// 设置店名（空白视为未设置，由界面提示）。
pub fn set_store_name(conn: &Connection, name: &str) -> Result<()> {
    let n = name.trim();
    if n.chars().count() > 30 {
        return Err(CoreError::Validation("店名不超过 30 个字符".into()));
    }
    cfg_set(conn, CFG_STORE_NAME, n)
}

/// 选定行业并把该行业的分类模板写入 category 表。
/// 幂等：已存在的同名同级分类不重复插入，重复选定无副作用。
pub fn apply_industry(conn: &Connection, industry: &str) -> Result<usize> {
    if industry_meta(industry).is_none() {
        return Err(CoreError::Validation(format!("未知行业: {industry}")));
    }
    let mut stmt = conn.prepare(
        "SELECT path FROM category_template WHERE industry=?1 ORDER BY sort_no, template_id",
    )?;
    let paths: Vec<String> = stmt
        .query_map([industry], |r| r.get(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    drop(stmt);

    let mut inserted = 0usize;
    for path in paths {
        let mut parent: Option<i64> = None;
        for name in path.split('/').filter(|s| !s.trim().is_empty()) {
            let name = name.trim();
            let existing: Option<i64> = conn
                .query_row(
                    "SELECT category_id FROM category WHERE name=?1 AND parent_id IS ?2 AND status=1",
                    params![name, parent],
                    |r| r.get(0),
                )
                .ok();
            parent = match existing {
                Some(id) => Some(id),
                None => {
                    conn.execute(
                        "INSERT INTO category(parent_id,name) VALUES (?1,?2)",
                        params![parent, name],
                    )?;
                    inserted += 1;
                    Some(conn.last_insert_rowid())
                }
            };
        }
    }
    cfg_set(conn, CFG_INDUSTRY, industry)?;
    Ok(inserted)
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
    fn template_seeded_for_three_industries() {
        let c = setup();
        for ind in INDUSTRIES {
            let n: i64 = c
                .query_row(
                    "SELECT COUNT(*) FROM category_template WHERE industry=?1",
                    [ind],
                    |r| r.get(0),
                )
                .unwrap();
            assert!(n > 0, "{ind} 缺少分类模板");
        }
    }

    #[test]
    fn apply_industry_writes_tree_and_is_idempotent() {
        let c = setup();
        let inserted = apply_industry(&c, INDUSTRY_SUPERMARKET).unwrap();
        assert!(inserted > 10);
        // 幂等：再选一次不新增
        let again = apply_industry(&c, INDUSTRY_SUPERMARKET).unwrap();
        assert_eq!(again, 0);
        // 二级分类挂在对应一级下
        let parent: String = c
            .query_row(
                "SELECT p.name FROM category c JOIN category p ON p.category_id=c.parent_id
                 WHERE c.name='休闲零食'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(parent, "食品饮料");
    }

    #[test]
    fn unknown_industry_rejected() {
        let c = setup();
        assert!(apply_industry(&c, "BAKERY").is_err());
    }

    #[test]
    fn profile_reflects_industry_and_name() {
        let c = setup();
        let p = store_profile(&c).unwrap();
        assert!(!p.configured);
        apply_industry(&c, INDUSTRY_CLOTHING).unwrap();
        set_store_name(&c, " 城南服饰店 ").unwrap();
        let p = store_profile(&c).unwrap();
        assert!(p.configured);
        assert_eq!(p.industry, INDUSTRY_CLOTHING);
        assert_eq!(p.industry_name, "服装鞋帽");
        assert_eq!(p.store_name, "城南服饰店");
    }

    #[test]
    fn store_name_length_limited() {
        let c = setup();
        assert!(set_store_name(&c, &"x".repeat(31)).is_err());
        assert!(set_store_name(&c, "旺旺超市").is_ok());
    }
}
