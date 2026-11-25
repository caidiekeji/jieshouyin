//! 养生技师：日排班、服务行指定、按折后实收比例提成。
//!
//! v1 规则：按营业日排班（在班/休息），不做钟点班次、点钟与轮排；提成在
//! 结算落单时固化，退货按剩余数量等比冲减，避免月底再按现价重算。

use crate::error::{CoreError, Result};
use crate::time::format_utc;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};

#[derive(Debug, Clone, serde::Serialize)]
pub struct Technician {
    pub technician_id: i64,
    pub name: String,
    pub phone: Option<String>,
    /// 提成比例，万分比：1200 = 12%。
    pub commission_bps: i64,
    /// PERCENT（按比例）/ FIXED（按每份固定金额）。
    pub commission_mode: String,
    /// FIXED 模式下的每份提成金额（分）。
    pub fixed_cents: i64,
    pub status: i64,
    /// 指定营业日是否在班。
    pub on_duty: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TechnicianCommission {
    pub technician_id: i64,
    pub name: String,
    pub service_lines: i64,
    pub service_qty: i64,
    pub commission_cents: i64,
}

/// 项目/分类级提成覆盖：同技师同分类只保留一条。
#[derive(Debug, Clone, serde::Serialize)]
pub struct TechnicianRule {
    pub technician_id: i64,
    pub category_id: i64,
    pub mode: String,
    pub bps: i64,
    pub fixed_cents: i64,
}

pub fn create_technician(
    conn: &Connection,
    name: &str,
    phone: Option<&str>,
    commission_bps: i64,
    commission_mode: &str,
    fixed_cents: i64,
) -> Result<i64> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 20 {
        return Err(CoreError::Validation("技师姓名必填且不超过 20 字".into()));
    }
    if !(0..=10000).contains(&commission_bps) {
        return Err(CoreError::Validation("提成比例需在 0–100%".into()));
    }
    if commission_mode != "PERCENT" && commission_mode != "FIXED" {
        return Err(CoreError::Validation(
            "提成方式只能是 PERCENT 或 FIXED".into(),
        ));
    }
    if fixed_cents < 0 {
        return Err(CoreError::Validation("固定提成金额不得为负".into()));
    }
    conn.execute(
        "INSERT INTO technician(name,phone,commission_bps,commission_mode,fixed_cents,status,created_at)
         VALUES (?1,?2,?3,?4,?5,1,?6)",
        params![
            name,
            phone.map(str::trim).filter(|p| !p.is_empty()),
            commission_bps,
            commission_mode,
            fixed_cents,
            format_utc(&Utc::now())
        ],
    )
    .map_err(|e| {
        if e.to_string().contains("UNIQUE") {
            CoreError::Validation(format!("技师「{name}」已存在"))
        } else {
            CoreError::Db(e)
        }
    })?;
    Ok(conn.last_insert_rowid())
}

/// 日排班开关：写入表示在班，删除表示休息。
pub fn set_shift(
    conn: &Connection,
    technician_id: i64,
    business_date: &str,
    on_duty: bool,
) -> Result<()> {
    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM technician WHERE technician_id=?1 AND status=1",
        [technician_id],
        |r| r.get(0),
    )?;
    if exists == 0 {
        return Err(CoreError::Validation("技师不存在或已停用".into()));
    }
    if on_duty {
        conn.execute(
            "INSERT OR IGNORE INTO technician_shift(technician_id,business_date,created_at)
             VALUES (?1,?2,?3)",
            params![technician_id, business_date, format_utc(&Utc::now())],
        )?;
    } else {
        conn.execute(
            "DELETE FROM technician_shift WHERE technician_id=?1 AND business_date=?2",
            params![technician_id, business_date],
        )?;
    }
    Ok(())
}

pub fn list_technicians(conn: &Connection, business_date: &str) -> Result<Vec<Technician>> {
    let mut stmt = conn.prepare(
        "SELECT t.technician_id,t.name,t.phone,t.commission_bps,t.commission_mode,t.fixed_cents,t.status,
                EXISTS(SELECT 1 FROM technician_shift s
                       WHERE s.technician_id=t.technician_id AND s.business_date=?1)
         FROM technician t WHERE t.status=1 ORDER BY t.technician_id",
    )?;
    let rows = stmt
        .query_map([business_date], |r| {
            Ok(Technician {
                technician_id: r.get(0)?,
                name: r.get(1)?,
                phone: r.get(2)?,
                commission_bps: r.get(3)?,
                commission_mode: r.get(4)?,
                fixed_cents: r.get(5)?,
                status: r.get(6)?,
                on_duty: r.get::<_, i64>(7)? != 0,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn set_rule(
    conn: &Connection,
    technician_id: i64,
    category_id: i64,
    mode: &str,
    bps: i64,
    fixed_cents: i64,
) -> Result<()> {
    if mode != "PERCENT" && mode != "FIXED" {
        return Err(CoreError::Validation(
            "提成方式只能是 PERCENT 或 FIXED".into(),
        ));
    }
    if !(0..=10000).contains(&bps) || fixed_cents < 0 {
        return Err(CoreError::Validation("提成比例或固定金额不合法".into()));
    }
    let tech: i64 = conn.query_row(
        "SELECT COUNT(*) FROM technician WHERE technician_id=?1 AND status=1",
        [technician_id],
        |r| r.get(0),
    )?;
    let cat: i64 = conn.query_row(
        "SELECT COUNT(*) FROM category WHERE category_id=?1",
        [category_id],
        |r| r.get(0),
    )?;
    if tech == 0 || cat == 0 {
        return Err(CoreError::Validation("技师或商品分类不存在".into()));
    }
    conn.execute(
        "INSERT INTO technician_commission_rule(technician_id,category_id,mode,bps,fixed_cents)
         VALUES (?1,?2,?3,?4,?5)
         ON CONFLICT(technician_id,category_id)
         DO UPDATE SET mode=excluded.mode,bps=excluded.bps,fixed_cents=excluded.fixed_cents",
        params![technician_id, category_id, mode, bps, fixed_cents],
    )?;
    Ok(())
}

pub fn delete_rule(conn: &Connection, technician_id: i64, category_id: i64) -> Result<()> {
    conn.execute(
        "DELETE FROM technician_commission_rule WHERE technician_id=?1 AND category_id=?2",
        params![technician_id, category_id],
    )?;
    Ok(())
}

pub fn list_rules(conn: &Connection, technician_id: Option<i64>) -> Result<Vec<TechnicianRule>> {
    let sql = match technician_id {
        Some(_) => {
            "SELECT technician_id,category_id,mode,bps,fixed_cents
             FROM technician_commission_rule WHERE technician_id=?1 ORDER BY category_id"
        }
        None => {
            "SELECT technician_id,category_id,mode,bps,fixed_cents
             FROM technician_commission_rule ORDER BY technician_id,category_id"
        }
    };
    let mut stmt = conn.prepare(sql)?;
    let map = |r: &rusqlite::Row<'_>| {
        Ok(TechnicianRule {
            technician_id: r.get(0)?,
            category_id: r.get(1)?,
            mode: r.get(2)?,
            bps: r.get(3)?,
            fixed_cents: r.get(4)?,
        })
    };
    let rows = match technician_id {
        Some(tid) => stmt
            .query_map([tid], map)?
            .collect::<std::result::Result<Vec<_>, _>>()?,
        None => stmt
            .query_map([], map)?
            .collect::<std::result::Result<Vec<_>, _>>()?,
    };
    Ok(rows)
}

/// 月度提成：只统计已结算订单，退货已按数量冲减 commission_cents。
pub fn commission_report(
    conn: &Connection,
    from_date: &str,
    to_date: &str,
) -> Result<Vec<TechnicianCommission>> {
    let mut stmt = conn.prepare(
        "SELECT t.technician_id,t.name,
                COUNT(DISTINCT CASE WHEN i.qty > i.returned_qty THEN i.item_id END),
                COALESCE(SUM(i.qty-i.returned_qty),0),
                COALESCE(SUM(i.commission_cents),0)
         FROM technician t
         LEFT JOIN sales_order_item i ON i.technician_id=t.technician_id
         LEFT JOIN sales_order o ON o.order_no=i.order_no
              AND o.business_date BETWEEN ?1 AND ?2
              AND o.status IN ('COMPLETED','PARTIAL_REFUND','REFUNDED')
         WHERE t.status=1 AND (i.item_id IS NULL OR o.order_no IS NOT NULL)
         GROUP BY t.technician_id,t.name
         ORDER BY 5 DESC, t.technician_id",
    )?;
    let rows = stmt
        .query_map(params![from_date, to_date], |r| {
            Ok(TechnicianCommission {
                technician_id: r.get(0)?,
                name: r.get(1)?,
                service_lines: r.get(2)?,
                service_qty: r.get(3)?,
                commission_cents: r.get(4)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// 单行提成：先取“技师+商品分类”的覆盖规则，没有再回退技师默认规则。
/// PERCENT 按该行折后金额占整单实收比例计提；FIXED 按每份固定金额计提。
pub fn line_commission(
    conn: &Connection,
    technician_id: Option<i64>,
    sku_id: i64,
    qty: i64,
    line_net_cents: i64,
    order_total_cents: i64,
    order_pay_cents: i64,
) -> Result<i64> {
    let Some(tid) = technician_id else {
        return Ok(0);
    };
    let (default_mode, default_bps, default_fixed): (String, i64, i64) = conn
        .query_row(
            "SELECT commission_mode,commission_bps,fixed_cents
             FROM technician WHERE technician_id=?1 AND status=1",
            [tid],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| CoreError::Validation("指定的技师不存在或已停用".into()))?;
    let category_id: Option<i64> = conn
        .query_row(
            "SELECT p.category_id FROM sku s JOIN spu p ON p.spu_id=s.spu_id WHERE s.sku_id=?1",
            [sku_id],
            |r| r.get(0),
        )
        .optional()?;
    let rule: Option<(String, i64, i64)> = match category_id {
        Some(cid) => conn
            .query_row(
                "SELECT mode,bps,fixed_cents FROM technician_commission_rule
                 WHERE technician_id=?1 AND category_id=?2",
                params![tid, cid],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?,
        None => None,
    };
    let (mode, bps, fixed) = rule.unwrap_or((default_mode, default_bps, default_fixed));
    if mode == "FIXED" {
        return Ok((fixed.max(0) as i128 * qty.max(0) as i128) as i64);
    }
    if order_total_cents <= 0 || line_net_cents <= 0 {
        return Ok(0);
    }
    let paid_line = (line_net_cents as i128 * order_pay_cents as i128) / order_total_cents as i128;
    let commission = (paid_line * bps as i128 + 5000) / 10000;
    Ok(commission as i64)
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
    fn create_list_and_shift() {
        let c = setup();
        let id = create_technician(&c, "小雅", Some("13800000000"), 1200, "PERCENT", 0).unwrap();
        assert!(create_technician(&c, "小雅", None, 1200, "PERCENT", 0).is_err());
        assert!(create_technician(&c, "坏比例", None, 10001, "PERCENT", 0).is_err());
        assert!(create_technician(&c, "坏方式", None, 0, "X", 0).is_err());
        let list = list_technicians(&c, "2026-11-22").unwrap();
        assert_eq!(list.len(), 1);
        assert!(!list[0].on_duty);
        set_shift(&c, id, "2026-11-22", true).unwrap();
        assert!(list_technicians(&c, "2026-11-22").unwrap()[0].on_duty);
        set_shift(&c, id, "2026-11-22", false).unwrap();
        assert!(!list_technicians(&c, "2026-11-22").unwrap()[0].on_duty);
    }

    #[test]
    fn commission_uses_discounted_payable_and_ratio() {
        let c = setup();
        let id = create_technician(&c, "小雅", None, 1200, "PERCENT", 0).unwrap();
        // 行折后 100 元，整单实收 90 元（整单九折），12% → 10.8 元。
        let commission = line_commission(&c, Some(id), 1, 1, 10000, 10000, 9000).unwrap();
        assert_eq!(commission, 1080);
        assert_eq!(
            line_commission(&c, None, 1, 1, 10000, 10000, 9000).unwrap(),
            0
        );
        assert!(line_commission(&c, Some(999), 1, 1, 10000, 10000, 9000).is_err());
        // 分类覆盖为每份固定 5 元，2 份 → 10 元，不受折扣影响。
        // 规则按 SKU 所属分类匹配，先补一条分类 1 的商品用于关联。
        c.execute(
            "INSERT INTO spu(category_id,spu_name,spu_code,status) VALUES (1,'按摩','T-2',1)",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO sku(spu_id,barcode,sale_price_cents) VALUES (1,'2001',10000)",
            [],
        )
        .unwrap();
        set_rule(&c, id, 1, "FIXED", 0, 500).unwrap();
        assert_eq!(
            line_commission(&c, Some(id), 1, 2, 10000, 10000, 9000).unwrap(),
            1000
        );
    }
}
