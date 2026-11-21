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

pub fn create_technician(
    conn: &Connection,
    name: &str,
    phone: Option<&str>,
    commission_bps: i64,
) -> Result<i64> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 20 {
        return Err(CoreError::Validation(
            "技师姓名必填且不超过 20 字".into(),
        ));
    }
    if !(0..=10000).contains(&commission_bps) {
        return Err(CoreError::Validation("提成比例需在 0–100%".into()));
    }
    conn.execute(
        "INSERT INTO technician(name,phone,commission_bps,status,created_at)
         VALUES (?1,?2,?3,1,?4)",
        params![
            name,
            phone.map(str::trim).filter(|p| !p.is_empty()),
            commission_bps,
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
        "SELECT t.technician_id,t.name,t.phone,t.commission_bps,t.status,
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
                status: r.get(4)?,
                on_duty: r.get::<_, i64>(5)? != 0,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
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

/// 单行提成：按该行折后金额占整单实收的比例，乘以技师比例；赠品不计。
pub fn line_commission(
    conn: &Connection,
    technician_id: Option<i64>,
    line_net_cents: i64,
    order_total_cents: i64,
    order_pay_cents: i64,
) -> Result<i64> {
    let Some(tid) = technician_id else {
        return Ok(0);
    };
    if order_total_cents <= 0 || line_net_cents <= 0 {
        return Ok(0);
    }
    let bps: i64 = conn
        .query_row(
            "SELECT commission_bps FROM technician WHERE technician_id=?1 AND status=1",
            [tid],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(|| CoreError::Validation("指定的技师不存在或已停用".into()))?;
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
        let id = create_technician(&c, "小雅", Some("13800000000"), 1200).unwrap();
        assert!(create_technician(&c, "小雅", None, 1200).is_err());
        assert!(create_technician(&c, "坏比例", None, 10001).is_err());
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
        let id = create_technician(&c, "小雅", None, 1200).unwrap();
        // 行折后 100 元，整单实收 90 元（整单九折），12% → 10.8 元。
        let commission = line_commission(&c, Some(id), 10000, 10000, 9000).unwrap();
        assert_eq!(commission, 1080);
        assert_eq!(line_commission(&c, None, 10000, 10000, 9000).unwrap(), 0);
        assert!(line_commission(&c, Some(999), 10000, 10000, 9000).is_err());
    }
}
