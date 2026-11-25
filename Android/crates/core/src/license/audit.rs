//! 授权审计（§4.9）：只增不改，不存完整激活码明文。

use crate::error::Result;
use rusqlite::{params, Connection};

#[derive(Debug, Clone, Copy)]
pub enum AuditEvent {
    ActivationSuccess,
    ActivationFailure,
    SignatureInvalid,
    FingerprintMismatch,
    ClockRollbackDetected,
    StateTransition,
    GracePeriodEntered,
    DegradedEntered,
    RebindSuccess,
    TrialStarted,
    TrialExpired,
}

impl AuditEvent {
    pub fn as_str(&self) -> &'static str {
        match self {
            AuditEvent::ActivationSuccess => "ACTIVATION_SUCCESS",
            AuditEvent::ActivationFailure => "ACTIVATION_FAILURE",
            AuditEvent::SignatureInvalid => "SIGNATURE_INVALID",
            AuditEvent::FingerprintMismatch => "FINGERPRINT_MISMATCH",
            AuditEvent::ClockRollbackDetected => "CLOCK_ROLLBACK_DETECTED",
            AuditEvent::StateTransition => "STATE_TRANSITION",
            AuditEvent::GracePeriodEntered => "GRACE_PERIOD_ENTERED",
            AuditEvent::DegradedEntered => "DEGRADED_ENTERED",
            AuditEvent::RebindSuccess => "REBIND_SUCCESS",
            AuditEvent::TrialStarted => "TRIAL_STARTED",
            AuditEvent::TrialExpired => "TRIAL_EXPIRED",
        }
    }
}

/// 写一条审计。license_id 只存标识，detail 只存哈希前缀，不存激活码明文。
pub fn append_audit(
    conn: &Connection,
    event: AuditEvent,
    license_id: Option<&str>,
    payload_hash_prefix: Option<&str>,
    fp_digest: Option<&str>,
    detail: serde_json::Value,
    created_at: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO license_audit(event_type, license_id, payload_hash, device_fp_digest, detail_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            event.as_str(),
            license_id,
            payload_hash_prefix,
            fp_digest,
            detail.to_string(),
            created_at
        ],
    )?;
    Ok(())
}

/// 激活码字符串哈希前缀（16 字符），用于审计取证，不可逆。
pub fn token_hash_prefix(token: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(token.as_bytes());
    hex::encode(h.finalize())[..16].to_string()
}

/// 归档超期审计（≥730 天）：导出为只读 CSV 文本，不删除原记录。
pub fn export_archive(conn: &Connection, before: &str) -> Result<String> {
    let mut stmt = conn.prepare(
        "SELECT audit_id,event_type,license_id,payload_hash,device_fp_digest,detail_json,created_at
         FROM license_audit WHERE created_at < ?1 ORDER BY audit_id",
    )?;
    let rows = stmt.query_map([before], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<String>>(3)?,
            r.get::<_, Option<String>>(4)?,
            r.get::<_, Option<String>>(5)?,
            r.get::<_, String>(6)?,
        ))
    })?;
    let mut out =
        String::from("audit_id,event_type,license_id,payload_hash,fp_digest,detail,created_at\n");
    for row in rows {
        let (id, ev, lic, hash, fp, detail, at) = row?;
        out.push_str(&format!(
            "{id},{ev},{},{},{},{},{at}\n",
            lic.unwrap_or_default(),
            hash.unwrap_or_default(),
            fp.unwrap_or_default(),
            detail.unwrap_or_default()
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{open_in_memory, run_migrations};
    use crate::time::format_utc;
    use chrono::Utc;
    use serde_json::json;

    #[test]
    fn audit_appends_only() {
        let mut conn = open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();
        let now = format_utc(&Utc::now());
        append_audit(
            &conn,
            AuditEvent::SignatureInvalid,
            Some("lic_1"),
            Some("abc"),
            None,
            json!({"e":"x"}),
            &now,
        )
        .unwrap();
        append_audit(
            &conn,
            AuditEvent::ActivationSuccess,
            Some("lic_1"),
            Some("abc"),
            Some("fp"),
            json!({}),
            &now,
        )
        .unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM license_audit", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn token_hash_is_prefix_only() {
        let h = token_hash_prefix("a.b");
        assert_eq!(h.len(), 16);
        assert!(!h.contains('.'));
    }

    #[test]
    fn export_is_readonly() {
        let mut conn = open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();
        let exported = export_archive(&conn, "2099-01-01T00:00:00Z").unwrap();
        assert!(exported.starts_with("audit_id,"));
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM license_audit", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
}
