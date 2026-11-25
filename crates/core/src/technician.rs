//! 养生技师：日排班、钟态（排钟/点钟/加钟/收钟）、服务行指定、按折后实收比例提成。
//!
//! 提成在结算落单时固化，退货按剩余数量等比冲减，避免月底再按现价重算。
//! 钟态只记录服务时长与技师占用，不改变包间房费；房费仍由 service::chess_fee 按桌台计时。

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

// ───────────────────────── 钟态：排钟 / 点钟 / 加钟 / 收钟 ─────────────────────────

/// ACTIVE=正在上钟；DONE=已收钟；CANCELLED=已取消。
#[derive(Debug, Clone, serde::Serialize)]
pub struct TechnicianBell {
    pub bell_id: i64,
    pub technician_id: i64,
    pub technician_name: String,
    pub table_id: Option<i64>,
    pub table_code: Option<String>,
    pub business_date: String,
    pub start_at: String,
    pub end_at: Option<String>,
    /// 计划服务时长（分钟）。加钟增加此值；不改变房费。
    pub minutes: i64,
    pub is_pointed: bool,
    pub status: String,
    /// 已经过的分钟数；收钟后按 end_at 固化。
    pub elapsed_minutes: i64,
}

fn parse_utc(s: &str) -> Option<chrono::DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

fn bell_elapsed(start_at: &str, end_at: Option<&str>) -> i64 {
    let Some(start) = parse_utc(start_at) else {
        return 0;
    };
    let end = end_at.and_then(parse_utc).unwrap_or_else(Utc::now);
    ((end - start).num_seconds().max(0) + 59) / 60
}

fn bell_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<TechnicianBell> {
    let start_at: String = r.get(6)?;
    let end_at: Option<String> = r.get(7)?;
    Ok(TechnicianBell {
        bell_id: r.get(0)?,
        technician_id: r.get(1)?,
        technician_name: r.get(2)?,
        table_id: r.get(3)?,
        table_code: r.get(4)?,
        business_date: r.get(5)?,
        start_at: start_at.clone(),
        end_at: end_at.clone(),
        minutes: r.get(8)?,
        is_pointed: r.get::<_, i64>(9)? != 0,
        status: r.get(10)?,
        elapsed_minutes: bell_elapsed(&start_at, end_at.as_deref()),
    })
}

const BELL_SELECT: &str = "SELECT b.bell_id,b.technician_id,t.name,b.table_id,tb.code,
        b.business_date,b.start_at,b.end_at,b.minutes,b.is_pointed,b.status
     FROM technician_bell b
     JOIN technician t ON t.technician_id=b.technician_id
     LEFT JOIN service_table tb ON tb.table_id=b.table_id";

/// 指派技师上钟。is_pointed=true 为顾客点钟，false 为系统排钟。
/// 技师必须在册、启用且当日已排班在班；同一技师不能同时挂两个 ACTIVE 钟。
pub fn assign_bell(
    conn: &Connection,
    technician_id: i64,
    table_id: i64,
    business_date: &str,
    minutes: i64,
    is_pointed: bool,
) -> Result<i64> {
    if !(1..=480).contains(&minutes) {
        return Err(CoreError::Validation("上钟时长须在 1-480 分钟之间".into()));
    }
    let on_duty: i64 = conn.query_row(
        "SELECT COUNT(*) FROM technician t
         WHERE t.technician_id=?1 AND t.status=1
           AND EXISTS(SELECT 1 FROM technician_shift s
                      WHERE s.technician_id=t.technician_id AND s.business_date=?2)",
        params![technician_id, business_date],
        |r| r.get(0),
    )?;
    if on_duty == 0 {
        return Err(CoreError::Validation("技师当日未排班或已停用，不能上钟".into()));
    }
    let table_ok: i64 = conn.query_row(
        "SELECT COUNT(*) FROM service_table WHERE table_id=?1",
        [table_id],
        |r| r.get(0),
    )?;
    if table_ok == 0 {
        return Err(CoreError::Validation("服务间不存在".into()));
    }
    let busy: Option<i64> = conn
        .query_row(
            "SELECT bell_id FROM technician_bell WHERE technician_id=?1 AND status='ACTIVE'",
            [technician_id],
            |r| r.get(0),
        )
        .optional()?;
    if busy.is_some() {
        return Err(CoreError::Validation("该技师正在上钟，请先收钟".into()));
    }

    let now = format_utc(&Utc::now());
    let tx = conn.unchecked_transaction()?;
    // 同一服务间换技师：旧钟自动收钟，避免两个 ACTIVE 钟并存。
    tx.execute(
        "UPDATE technician_bell SET status='DONE',end_at=?2
         WHERE table_id=?1 AND status='ACTIVE'",
        params![table_id, now],
    )?;
    tx.execute(
        "INSERT INTO technician_bell(technician_id,table_id,business_date,start_at,minutes,
                                     is_pointed,status,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,'ACTIVE',?4)",
        params![technician_id, table_id, business_date, now, minutes, is_pointed as i64],
    )?;
    let bell_id = tx.last_insert_rowid();
    tx.execute(
        "UPDATE table_session SET current_technician_id=?2
         WHERE table_id=?1 AND status='OPEN'",
        params![table_id, technician_id],
    )?;
    tx.commit()?;
    Ok(bell_id)
}

/// 自动排钟：取当日排班且空闲的第一位技师；无人可排时返回业务错误。
pub fn auto_assign_bell(
    conn: &Connection,
    table_id: i64,
    business_date: &str,
    minutes: i64,
) -> Result<i64> {
    let candidate: Option<i64> = conn
        .query_row(
            "SELECT t.technician_id FROM technician t
             WHERE t.status=1
               AND EXISTS(SELECT 1 FROM technician_shift s
                          WHERE s.technician_id=t.technician_id AND s.business_date=?1)
               AND NOT EXISTS(SELECT 1 FROM technician_bell b
                              WHERE b.technician_id=t.technician_id AND b.status='ACTIVE')
             ORDER BY t.technician_id LIMIT 1",
            [business_date],
            |r| r.get(0),
        )
        .optional()?;
    let tid = candidate.ok_or_else(|| CoreError::Validation("当前没有可排的空闲技师".into()))?;
    assign_bell(conn, tid, table_id, business_date, minutes, false)
}

/// 加钟：只延长技师服务时长，不动房费与桌台计时。
pub fn add_bell_minutes(conn: &Connection, bell_id: i64, extra_minutes: i64) -> Result<()> {
    if !(1..=240).contains(&extra_minutes) {
        return Err(CoreError::Validation("加钟时长须在 1-240 分钟之间".into()));
    }
    let affected = conn.execute(
        "UPDATE technician_bell SET status='ACTIVE',minutes=minutes+?2
         WHERE bell_id=?1 AND status='ACTIVE'",
        params![bell_id, extra_minutes],
    )?;
    if affected == 0 {
        return Err(CoreError::Validation("钟位不存在或已收钟，不能加钟".into()));
    }
    Ok(())
}

/// 收钟：固化实际结束时刻与经过分钟，并解除桌台会话的技师绑定。
pub fn finish_bell(conn: &Connection, bell_id: i64) -> Result<()> {
    let (table_id, start_at): (Option<i64>, String) = conn
        .query_row(
            "SELECT table_id,start_at FROM technician_bell WHERE bell_id=?1 AND status='ACTIVE'",
            [bell_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| CoreError::Validation("钟位不存在或已收钟".into()))?;
    let now = format_utc(&Utc::now());
    let elapsed = bell_elapsed(&start_at, Some(&now));
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE technician_bell SET status='DONE',end_at=?2,minutes=MAX(minutes,?3)
         WHERE bell_id=?1",
        params![bell_id, now, elapsed],
    )?;
    if let Some(tid) = table_id {
        tx.execute(
            "UPDATE table_session SET current_technician_id=NULL
             WHERE table_id=?1 AND status='OPEN'",
            [tid],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub fn list_bells(
    conn: &Connection,
    business_date: &str,
    status: Option<&str>,
) -> Result<Vec<TechnicianBell>> {
    let sql = format!(
        "{BELL_SELECT} WHERE b.business_date=?1 AND (?2 IS NULL OR b.status=?2)
         ORDER BY b.bell_id"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![business_date, status], bell_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
}

pub fn active_bell_for_table(conn: &Connection, table_id: i64) -> Result<Option<TechnicianBell>> {
    let sql = format!("{BELL_SELECT} WHERE b.table_id=?1 AND b.status='ACTIVE' LIMIT 1");
    conn.query_row(&sql, [table_id], bell_row).optional().map_err(Into::into)
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

    #[test]
    fn bells_require_duty_and_support_point_add_finish() {
        let c = setup();
        let t1 = create_technician(&c, "小雅", None, 1200, "PERCENT", 0).unwrap();
        let t2 = create_technician(&c, "小美", None, 1200, "PERCENT", 0).unwrap();
        let date = "2026-11-26";
        set_shift(&c, t1, date, true).unwrap();
        set_shift(&c, t2, date, true).unwrap();
        c.execute(
            "INSERT INTO service_table(table_id,code,scene,name,created_at)
             VALUES (1,'W01','WELLNESS','养生1','2026-11-26T00:00:00Z')",
            [],
        )
        .unwrap();

        // 未排班的技师不能点钟
        assert!(assign_bell(&c, 999, 1, date, 60, true).is_err());
        let bell = assign_bell(&c, t1, 1, date, 60, true).unwrap();
        assert_eq!(list_bells(&c, date, Some("ACTIVE")).unwrap().len(), 1);
        assert!(active_bell_for_table(&c, 1).unwrap().is_some());
        // 同一技师不能同时挂两个钟
        assert!(assign_bell(&c, t1, 1, date, 60, false).is_err());
        // 自动排钟会跳过忙碌的 t1，选中 t2
        c.execute(
            "INSERT INTO service_table(table_id,code,scene,name,created_at)
             VALUES (2,'W02','WELLNESS','养生2','2026-11-26T00:00:00Z')",
            [],
        )
        .unwrap();
        let auto = auto_assign_bell(&c, 2, date, 90).unwrap();
        let rows = list_bells(&c, date, None).unwrap();
        assert!(rows.iter().any(|b| b.bell_id == auto && !b.is_pointed));
        add_bell_minutes(&c, bell, 30).unwrap();
        assert_eq!(
            list_bells(&c, date, None)
                .unwrap()
                .iter()
                .find(|b| b.bell_id == bell)
                .unwrap()
                .minutes,
            90
        );
        finish_bell(&c, bell).unwrap();
        assert_eq!(list_bells(&c, date, Some("ACTIVE")).unwrap().len(), 1);
        assert!(add_bell_minutes(&c, bell, 30).is_err());
    }
}
