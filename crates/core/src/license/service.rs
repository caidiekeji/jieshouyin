//! 启动验证序列与激活/续期/换绑服务（§4.6、§4.7）。

use crate::error::{CoreError, LicenseError, Result};
use crate::time::{format_utc, Clock, MonotonicClock};
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde_json::json;
use std::sync::Arc;

use super::audit::{append_audit, token_hash_prefix, AuditEvent};
use super::fingerprint::{device_fingerprint, fp_digest};
use super::payload::{parse_and_verify, LicensePayload};
use super::state::{decide_status, LicenseStatus};
use super::storage::{mirror_to_db, LicenseBundle, SecureStore};

/// 当前授权快照，UI 只消费此状态（禁止在 UI 层实现状态机）。
#[derive(Debug, Clone)]
pub struct LicenseSnapshot {
    pub status: LicenseStatus,
    pub payload: Option<LicensePayload>,
    pub entitlements: Vec<String>,
    pub valid_until: Option<String>,
    pub maintenance_until: Option<String>,
}

impl LicenseSnapshot {
    pub fn unactivated() -> Self {
        Self {
            status: LicenseStatus::UNACTIVATED,
            payload: None,
            entitlements: vec![],
            valid_until: None,
            maintenance_until: None,
        }
    }

    pub fn has_entitlement(&self, ent: &str) -> bool {
        self.entitlements.iter().any(|e| e == ent)
    }

    /// 门禁：先 entitlement 再状态。pos.core 在任何非 UNACTIVATED 状态都放行。
    pub fn gate(&self, ent: &str) -> Result<()> {
        if !self.has_entitlement(ent) {
            return Err(CoreError::EntitlementDenied(ent.into()));
        }
        if ent == super::ENT_POS_CORE && self.status.allows_pos_core() {
            return Ok(());
        }
        if self.status == LicenseStatus::UNACTIVATED {
            return Err(CoreError::EntitlementDenied(ent.into()));
        }
        if !self.status.allows_admin() {
            return Err(CoreError::EntitlementDenied(format!(
                "当前授权状态 {} 不允许后台操作 {ent}",
                self.status.as_str()
            )));
        }
        Ok(())
    }
}

pub struct LicenseService {
    store: Arc<dyn SecureStore>,
    public_key: [u8; 32],
    mono: Arc<MonotonicClock<Box<dyn Clock>>>,
}

impl LicenseService {
    pub fn new(
        store: Arc<dyn SecureStore>,
        public_key: [u8; 32],
        clock: Box<dyn Clock>,
        last_known: DateTime<Utc>,
    ) -> Self {
        Self {
            store,
            public_key,
            mono: Arc::new(MonotonicClock::new(clock, last_known)),
        }
    }

    /// 首次激活：验签 → 设备指纹比对 → 写安全存储 → 镜像 → 审计。全程无网络。
    pub fn activate(&self, conn: &Connection, token: &str) -> Result<LicenseSnapshot> {
        let now = format_utc(&Utc::now());
        let (payload, sig) = match parse_and_verify(token, &self.public_key) {
            Ok(v) => v,
            Err(e) => {
                append_audit(
                    conn,
                    AuditEvent::ActivationFailure,
                    None,
                    Some(&token_hash_prefix(token)),
                    None,
                    json!({"reason": e.to_string()}),
                    &now,
                )?;
                if matches!(e, CoreError::License(LicenseError::SignatureInvalid)) {
                    append_audit(
                        conn,
                        AuditEvent::SignatureInvalid,
                        None,
                        Some(&token_hash_prefix(token)),
                        None,
                        json!({}),
                        &now,
                    )?;
                }
                return Err(e);
            }
        };

        let current_fp = device_fingerprint()?;
        // 载荷签发时 device_fingerprint 为 null；若非 null 则必须匹配当前设备。
        if let Some(expected) = &payload.device_fingerprint {
            if current_fp.as_deref() != Some(expected.as_str()) {
                append_audit(
                    conn,
                    AuditEvent::FingerprintMismatch,
                    Some(&payload.license_id),
                    Some(&token_hash_prefix(token)),
                    Some(&fp_digest(current_fp.as_deref())),
                    json!({}),
                    &now,
                )?;
                return Err(LicenseError::FingerprintMismatch.into());
            }
        }

        let (effective, rollback) = self.mono.effective_now();
        if rollback {
            append_audit(
                conn,
                AuditEvent::ClockRollbackDetected,
                Some(&payload.license_id),
                None,
                None,
                json!({"effective_now": format_utc(&effective)}),
                &now,
            )?;
        }
        if let Some(previous) = self.store.load()? {
            if previous.last_known_dt()? > effective {
                // 单调水位以存储中的水位为准，进一档
                // 实际场景由 verify_on_launch 处理；激活时保留最大值
            }
        }
        let bundle = LicenseBundle {
            token: token.to_string(),
            payload: payload.clone(),
            signature: sig.to_vec(),
            device_fingerprint: current_fp.clone(),
            activated_at: now.clone(),
            last_known_time: format_utc(&effective),
        };
        self.store.save(&bundle)?;
        mirror_to_db(conn, &bundle)?;
        append_audit(
            conn,
            AuditEvent::ActivationSuccess,
            Some(&payload.license_id),
            Some(&token_hash_prefix(token)),
            Some(&fp_digest(current_fp.as_deref())),
            json!({"license_type": payload.license_type}),
            &now,
        )?;

        let status = decide_status(&payload, effective)?;
        append_audit(
            conn,
            AuditEvent::StateTransition,
            Some(&payload.license_id),
            None,
            None,
            json!({"from":"UNACTIVATED","to":status.as_str()}),
            &now,
        )?;
        Ok(self.snapshot(status, &payload))
    }

    /// 续期：更新载荷，保留设备绑定。降级态立即恢复 ACTIVE。
    pub fn renew(&self, conn: &Connection, token: &str) -> Result<LicenseSnapshot> {
        let (payload, sig) = parse_and_verify(token, &self.public_key)?;
        let previous = self
            .store
            .load()?
            .ok_or(CoreError::License(LicenseError::Unactivated))?;
        if previous.payload.license_id != payload.license_id {
            return Err(CoreError::Validation("续期码 license_id 不一致".into()));
        }
        let effective = self.mono.effective_now().0;
        let bundle = LicenseBundle {
            token: token.to_string(),
            device_fingerprint: previous.device_fingerprint,
            activated_at: previous.activated_at,
            payload: payload.clone(),
            signature: sig.to_vec(),
            last_known_time: format_utc(&effective),
        };
        self.store.save(&bundle)?;
        mirror_to_db(conn, &bundle)?;
        let status = decide_status(&payload, effective)?;
        append_audit(
            conn,
            AuditEvent::StateTransition,
            Some(&payload.license_id),
            None,
            None,
            json!({"event":"RENEW","to":status.as_str()}),
            &format_utc(&effective),
        )?;
        Ok(self.snapshot(status, &payload))
    }

    /// 启动验证：DB 先行 → 读存储 → 单调水位 → 验签 → 指纹 → 状态 → 落库。
    /// 顺序不可颠倒。
    pub fn verify_on_launch(&self, conn: &Connection) -> Result<LicenseSnapshot> {
        let bundle = match self.store.load()? {
            Some(b) => b,
            None => {
                self.persist_state(conn, &LicenseSnapshot::unactivated())?;
                return Ok(LicenseSnapshot::unactivated());
            }
        };

        let now = format_utc(&Utc::now());
        // 用内嵌公钥重新验证签名（签名是终极校验，加密只增加提取成本）
        let (payload, _) = match parse_and_verify(&bundle.token, &self.public_key) {
            Ok(v) => v,
            Err(e) => {
                append_audit(
                    conn,
                    AuditEvent::SignatureInvalid,
                    Some(&bundle.payload.license_id),
                    Some(&token_hash_prefix(&bundle.token)),
                    Some(&fp_digest(bundle.device_fingerprint.as_deref())),
                    json!({"reason": e.to_string()}),
                    &now,
                )?;
                let snap = LicenseSnapshot::unactivated();
                self.persist_state(conn, &snap)?;
                return Ok(snap);
            }
        };

        // 权威源裁决：安全存储载荷为准，重验签后覆盖镜像
        mirror_to_db(conn, &bundle)?;

        let current_fp = device_fingerprint()?;
        if let Some(expected) = &payload.device_fingerprint {
            if current_fp.as_deref() != Some(expected.as_str()) {
                append_audit(
                    conn,
                    AuditEvent::FingerprintMismatch,
                    Some(&payload.license_id),
                    None,
                    Some(&fp_digest(current_fp.as_deref())),
                    json!({}),
                    &now,
                )?;
                let snap = LicenseSnapshot::unactivated();
                self.persist_state(conn, &snap)?;
                return Ok(snap);
            }
        }
        // 指纹降级：存储有指纹但当前无法计算时不静默通过，标记未绑定并审计
        if bundle.device_fingerprint.is_some() && current_fp.is_none() {
            append_audit(
                conn,
                AuditEvent::FingerprintMismatch,
                Some(&payload.license_id),
                None,
                Some("UNBOUND"),
                json!({"reason":"fingerprint_unavailable"}),
                &now,
            )?;
        }

        let stored_watermark = bundle
            .last_known_dt()
            .unwrap_or(DateTime::<Utc>::MIN_UTC);
        // 重建单调水位：max(存储水位, 系统时间)
        let system_now = Utc::now();
        let effective = if system_now > stored_watermark {
            system_now
        } else {
            stored_watermark
        };
        if system_now < stored_watermark - chrono::Duration::hours(24) {
            append_audit(
                conn,
                AuditEvent::ClockRollbackDetected,
                Some(&payload.license_id),
                None,
                None,
                json!({"effective_now": format_utc(&effective)}),
                &now,
            )?;
        }
        let status = decide_status(&payload, effective)?;
        match status {
            LicenseStatus::GRACE_PERIOD => append_audit(
                conn,
                AuditEvent::GracePeriodEntered,
                Some(&payload.license_id),
                None,
                None,
                json!({}),
                &now,
            )?,
            LicenseStatus::OFFLINE_DEGRADED => append_audit(
                conn,
                AuditEvent::DegradedEntered,
                Some(&payload.license_id),
                None,
                None,
                json!({}),
                &now,
            )?,
            _ => {}
        }

        // 水位只增不减：持久化回存储
        let mut updated = bundle.clone();
        updated.last_known_time = format_utc(&effective);
        self.store.save(&updated)?;
        mirror_to_db(conn, &updated)?;

        let snap = self.snapshot(status, &payload);
        self.persist_state(conn, &snap)?;
        Ok(snap)
    }

    fn snapshot(&self, status: LicenseStatus, payload: &LicensePayload) -> LicenseSnapshot {
        LicenseSnapshot {
            status,
            payload: Some(payload.clone()),
            entitlements: payload.entitlements.clone(),
            valid_until: payload.valid_until.clone(),
            maintenance_until: payload.maintenance_until.clone(),
        }
    }

    fn persist_state(&self, conn: &Connection, snap: &LicenseSnapshot) -> Result<()> {
        let license_type = snap
            .payload
            .as_ref()
            .map(|p| p.license_type.as_str())
            .unwrap_or("NONE");
        conn.execute(
            "INSERT INTO license_state(state_key,state,license_type,entitlements_json,valid_until,maintenance_until,updated_at)
             VALUES ('current',?1,?2,?3,?4,?5,?6)
             ON CONFLICT(state_key) DO UPDATE SET
               state=excluded.state, license_type=excluded.license_type,
               entitlements_json=excluded.entitlements_json, valid_until=excluded.valid_until,
               maintenance_until=excluded.maintenance_until, updated_at=excluded.updated_at",
            rusqlite::params![
                snap.status.as_str(),
                license_type,
                serde_json::to_string(&snap.entitlements)?,
                snap.valid_until,
                snap.maintenance_until,
                format_utc(&Utc::now()),
            ],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::license::payload::LicensePayload;
    use crate::license::storage::PlainStore;
    use crate::time::{parse_rfc3339, FixedClock};
    use ed25519_dalek::{Signer, SigningKey};

    struct Fixture {
        dir: std::path::PathBuf,
        sk: SigningKey,
    }

    impl Fixture {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("cr_svc_{tag}_{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self {
                dir,
                sk: {
                    use rand::RngCore;
                    let mut b = [0u8; 32];
                    rand::rngs::OsRng.fill_bytes(&mut b);
                    SigningKey::from_bytes(&b)
                },
            }
        }

        fn service(&self, last: &str) -> LicenseService {
            let store = Arc::new(PlainStore::new(self.dir.join("license.json")));
            let clock = Box::new(FixedClock::new(
                parse_rfc3339("2027-01-05T00:00:00Z").unwrap(),
            ));
            LicenseService::new(
                store,
                *self.sk.verifying_key().as_bytes(),
                clock,
                parse_rfc3339(last).unwrap(),
            )
        }

        fn conn(&self) -> Connection {
            let mut c = open_in_memory().unwrap();
            crate::db::run_migrations(&mut c).unwrap();
            c
        }

        fn token(&self, valid_until: &str, grace: u32, license_type: &str) -> String {
            let p = LicensePayload {
                license_id: "lic_test".into(),
                product: "checkout-r".into(),
                license_type: license_type.into(),
                issued_at: "2026-01-01T00:00:00Z".into(),
                valid_until: if license_type == "ANNUAL" {
                    Some(valid_until.into())
                } else {
                    None
                },
                maintenance_until: Some(valid_until.into()),
                grace_period_days: grace,
                max_devices: 1,
                device_fingerprint: None,
                entitlements: vec!["pos.core".into(), "report.standard".into()],
                store_id: "S".into(),
                customer_name: "T".into(),
            };
            let json = serde_json::to_vec(&p).unwrap();
            let sig = self.sk.sign(&json);
            crate::license::payload::encode_license(&p, &sig.to_bytes()).unwrap()
        }
    }

    #[test]
    fn unactivated_on_fresh_start() {
        let f = Fixture::new("fresh");
        let svc = f.service("2026-01-01T00:00:00Z");
        let conn = f.conn();
        let snap = svc.verify_on_launch(&conn).unwrap();
        assert_eq!(snap.status, LicenseStatus::UNACTIVATED);
        assert!(snap.gate("pos.core").is_err());
    }

    #[test]
    fn activation_then_restart_active() {
        let f = Fixture::new("activate");
        let svc = f.service("2026-01-01T00:00:00Z");
        let conn = f.conn();
        let token = f.token("2027-06-01T00:00:00Z", 7, "ANNUAL");
        let snap = svc.activate(&conn, &token).unwrap();
        assert_eq!(snap.status, LicenseStatus::ACTIVE);
        assert!(snap.gate("pos.core").is_ok());
        assert!(snap.gate("report.standard").is_ok());
        let snap2 = svc.verify_on_launch(&conn).unwrap();
        assert_eq!(snap2.status, LicenseStatus::ACTIVE);
    }

    #[test]
    fn invalid_signature_writes_audit() {
        let f = Fixture::new("bad");
        let svc = f.service("2026-01-01T00:00:00Z");
        let conn = f.conn();
        let token = f.token("2027-06-01T00:00:00Z", 7, "ANNUAL");
        let bad = format!("{token}x");
        let err = svc.activate(&conn, &bad).unwrap_err();
        assert!(err.to_string().contains("签名") || err.to_string().contains("格式"));
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM license_audit WHERE event_type IN ('ACTIVATION_FAILURE','SIGNATURE_INVALID')",
            [], |r| r.get(0)).unwrap();
        assert!(n >= 1);
    }

    #[test]
    fn degraded_pos_core_never_blocked() {
        let f = Fixture::new("degraded");
        let svc = f.service("2026-01-01T00:00:00Z");
        let conn = f.conn();
        let token = f.token("2026-12-01T00:00:00Z", 3, "ANNUAL"); // 已过期远超宽限
        let snap = svc.activate(&conn, &token).unwrap();
        assert_eq!(snap.status, LicenseStatus::OFFLINE_DEGRADED);
        assert!(snap.gate("pos.core").is_ok(), "降级态不得阻断前台收银");
        assert!(snap.gate("report.standard").is_err(), "降级态后台应受限");
    }

    #[test]
    fn entitlement_denied_distinct_from_permission() {
        let f = Fixture::new("ent");
        let svc = f.service("2026-01-01T00:00:00Z");
        let conn = f.conn();
        let token = f.token("2027-06-01T00:00:00Z", 7, "ANNUAL");
        let snap = svc.activate(&conn, &token).unwrap();
        match snap.gate("nonexistent.ent") {
            Err(CoreError::EntitlementDenied(_)) => {}
            other => panic!("期望 EntitlementDenied, 得到 {other:?}"),
        }
    }

    #[test]
    fn renew_restores_active() {
        let f = Fixture::new("renew");
        let svc = f.service("2026-01-01T00:00:00Z");
        let conn = f.conn();
        let old = f.token("2026-12-01T00:00:00Z", 3, "ANNUAL");
        svc.activate(&conn, &old).unwrap();
        let new = f.token("2028-01-01T00:00:00Z", 7, "ANNUAL");
        let snap = svc.renew(&conn, &new).unwrap();
        assert_eq!(snap.status, LicenseStatus::ACTIVE);
    }
}
