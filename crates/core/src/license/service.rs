//! 启动验证序列与激活/续期/换绑服务（§4.6、§4.7）。

use crate::constants::{
    CLOCK_SKEW_TOLERANCE_HOURS, LICENSE_NONE, LICENSE_TRIAL, STATE_KEY_TRIAL, STATUS_TRIAL_ACTIVE,
    STATUS_UNACTIVATED, TRIAL_DAYS, TRIAL_ENTITLEMENTS,
};
use crate::error::{CoreError, LicenseError, Result};
use crate::time::{format_utc, parse_rfc3339, Clock, MonotonicClock};
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
#[derive(Debug, Clone, serde::Serialize)]
pub struct LicenseSnapshot {
    pub status: LicenseStatus,
    pub payload: Option<LicensePayload>,
    pub entitlements: Vec<String>,
    pub valid_until: Option<String>,
    pub maintenance_until: Option<String>,
    /// 试用到期时间（仅 TRIAL_ACTIVE 状态有值）
    pub trial_ends_at: Option<String>,
}

impl LicenseSnapshot {
    pub fn unactivated() -> Self {
        Self {
            status: LicenseStatus::UNACTIVATED,
            payload: None,
            entitlements: vec![],
            valid_until: None,
            maintenance_until: None,
            trial_ends_at: None,
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
        self.activate_with_key(conn, token, None)
    }

    /// 首次激活并记录激活码。
    /// 在线激活时传入 activation_key：后续可凭它联网向激活服务刷新授权
    /// （撤销/过期/续期以服务端为准）；离线导入传 None。
    pub fn activate_with_key(
        &self,
        conn: &Connection,
        token: &str,
        activation_key: Option<&str>,
    ) -> Result<LicenseSnapshot> {
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
            activation_key: activation_key
                .map(str::trim)
                .filter(|k| !k.is_empty())
                .map(str::to_string),
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
            json!({"from":STATUS_UNACTIVATED,"to":status.as_str()}),
            &now,
        )?;
        // 激活成功后删除试用记录：授权失效回到 UNACTIVATED 时试用不得复活
        conn.execute(
            "DELETE FROM license_state WHERE state_key = ?1",
            [STATE_KEY_TRIAL],
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
            activation_key: previous.activation_key,
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
    /// 顺序不可颠倒。无许可证时进入 15 天全功能试用（§4.5 TRIAL_ACTIVE）。
    pub fn verify_on_launch(&self, conn: &Connection) -> Result<LicenseSnapshot> {
        let bundle = match self.store.load()? {
            Some(b) => b,
            None => return self.handle_trial(conn),
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

        let stored_watermark = bundle.last_known_dt().unwrap_or(DateTime::<Utc>::MIN_UTC);
        // 重建单调水位：max(存储水位, 系统时间)
        let system_now = Utc::now();
        let effective = if system_now > stored_watermark {
            system_now
        } else {
            stored_watermark
        };
        if system_now < stored_watermark - chrono::Duration::hours(CLOCK_SKEW_TOLERANCE_HOURS) {
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

    /// 联网刷新授权：用本地保存的激活码向激活服务幂等换票。
    ///
    /// 语义（服务端为准）：
    /// - 授权被撤销 / 已过期 / 设备超限 → 服务端返回 4xx，此处返回错误，由调用方决定降级；
    /// - 授权正常 → 服务端重新签发载荷，本地验签后更新存储（到期日、权益随之刷新）；
    /// - 网络不可达 → 返回错误，调用方保留本地缓存继续工作（前台收银不得因此中断）。
    ///
    /// 没有激活码（离线导入的授权）时直接返回错误，因为无从联网刷新。
    pub fn refresh_online(
        &self,
        conn: &Connection,
        endpoint: &str,
        store_id: &str,
        customer_name: &str,
    ) -> Result<LicenseSnapshot> {
        let bundle = self
            .store
            .load()?
            .ok_or(CoreError::License(LicenseError::Unactivated))?;
        let activation_key = bundle
            .activation_key
            .as_deref()
            .map(str::trim)
            .filter(|k| !k.is_empty())
            .ok_or_else(|| {
                CoreError::Validation("当前授权不是通过在线激活获得的，无法联网刷新".into())
            })?;

        let req = super::online::OnlineActivateRequest {
            store_id: store_id.to_string(),
            customer_name: customer_name.to_string(),
            device_fingerprint: bundle.device_fingerprint.clone(),
            activation_key: activation_key.to_string(),
        };
        let resp = super::online::request_activation(endpoint, &req)?;
        // 服务端返回的仍是签名载荷，走与本地激活完全相同的验签链路。
        let snap = self.activate_with_key(conn, &resp.token, Some(activation_key))?;
        let license_id = snap.payload.as_ref().map(|p| p.license_id.clone());
        append_audit(
            conn,
            AuditEvent::StateTransition,
            license_id.as_deref(),
            None,
            None,
            json!({"event":"ONLINE_REFRESH","valid_until":snap.valid_until}),
            &format_utc(&Utc::now()),
        )?;
        Ok(snap)
    }

    /// 服务端明确拒绝授权（已撤销 / 已过期 / 设备超限）时，把本地状态同步降级。
    ///
    /// 降级为 OFFLINE_DEGRADED：前台收银仍可用（架构硬约束），后台管理受限，
    /// 不再显示「已激活」以免误导。原始载荷保留，便于恢复后重新刷新。
    pub fn mark_revoked(&self, conn: &Connection, reason: &str) -> Result<LicenseSnapshot> {
        let bundle = self.store.load()?;
        let payload = bundle.as_ref().map(|b| b.payload.clone());
        let snap = LicenseSnapshot {
            status: LicenseStatus::OFFLINE_DEGRADED,
            entitlements: payload
                .as_ref()
                .map(|p| p.entitlements.clone())
                .unwrap_or_default(),
            valid_until: payload.as_ref().and_then(|p| p.valid_until.clone()),
            maintenance_until: payload.as_ref().and_then(|p| p.maintenance_until.clone()),
            trial_ends_at: None,
            payload,
        };
        let license_id = snap.payload.as_ref().map(|p| p.license_id.clone());
        append_audit(
            conn,
            AuditEvent::DegradedEntered,
            license_id.as_deref(),
            None,
            None,
            json!({"event":"ONLINE_REJECTED","reason":reason}),
            &format_utc(&Utc::now()),
        )?;
        self.persist_state(conn, &snap)?;
        Ok(snap)
    }

    /// 15 天全功能试用（需求变更，文档 §4.5 补记）：
    /// - 首次启动无许可证 → 记录试用（license_state state_key='trial'），TRIAL_ACTIVE
    /// - 重启 → 试用继续，开始时间不变（单调水位防回拨延长）
    /// - 到期 → UNACTIVATED，激活界面；激活成功 → 删除试用记录，永不复活
    /// - 边界（如实声明）：纯离线架构下删除数据目录可重置试用，无法技术强制
    fn handle_trial(&self, conn: &Connection) -> Result<LicenseSnapshot> {
        let system_now = Utc::now();
        let now = format_utc(&system_now);

        let row: Option<(String, Option<String>)> = conn
            .query_row(
                "SELECT updated_at, valid_until FROM license_state WHERE state_key = ?1",
                [STATE_KEY_TRIAL],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok();

        match row {
            Some((started_at, ends_at)) => {
                let start = parse_rfc3339(&started_at)?;
                // 单调水位：试用期判定不早于开始时间，回拨不延长试用
                let effective = if system_now > start {
                    system_now
                } else {
                    start
                };
                if system_now < start - chrono::Duration::hours(CLOCK_SKEW_TOLERANCE_HOURS) {
                    append_audit(
                        conn,
                        AuditEvent::ClockRollbackDetected,
                        None,
                        None,
                        None,
                        json!({"scope": "trial", "effective_now": format_utc(&effective)}),
                        &now,
                    )?;
                }
                let ends = match ends_at.as_deref().map(parse_rfc3339) {
                    Some(Ok(t)) => t,
                    // 记录损坏：按 start + 15 天重算，宁可多给不可提前锁定
                    _ => start + chrono::Duration::days(TRIAL_DAYS),
                };
                if effective >= ends {
                    append_audit(
                        conn,
                        AuditEvent::TrialExpired,
                        None,
                        None,
                        None,
                        json!({"trial_ends_at": format_utc(&ends)}),
                        &now,
                    )?;
                    let snap = LicenseSnapshot::unactivated();
                    self.persist_state(conn, &snap)?;
                    return Ok(snap);
                }
                let snap = Self::trial_snapshot(ends);
                self.persist_state(conn, &snap)?;
                Ok(snap)
            }
            None => {
                // 首次启动：开始 15 天全功能试用
                let ends = system_now + chrono::Duration::days(TRIAL_DAYS);
                append_audit(
                    conn,
                    AuditEvent::TrialStarted,
                    None,
                    None,
                    None,
                    json!({"trial_ends_at": format_utc(&ends)}),
                    &now,
                )?;
                conn.execute(
                    "INSERT INTO license_state(state_key,state,license_type,entitlements_json,valid_until,maintenance_until,updated_at)
                     VALUES (?1,?2,?3,?4,?5,NULL,?6)",
                    rusqlite::params![
                        STATE_KEY_TRIAL,
                        STATUS_TRIAL_ACTIVE,
                        LICENSE_TRIAL,
                        serde_json::to_string(&TRIAL_ENTITLEMENTS)?,
                        format_utc(&ends),
                        &now,
                    ],
                )?;
                let snap = Self::trial_snapshot(ends);
                self.persist_state(conn, &snap)?;
                Ok(snap)
            }
        }
    }

    fn trial_snapshot(ends: DateTime<Utc>) -> LicenseSnapshot {
        LicenseSnapshot {
            status: LicenseStatus::TRIAL_ACTIVE,
            payload: None,
            entitlements: TRIAL_ENTITLEMENTS.iter().map(|s| s.to_string()).collect(),
            valid_until: Some(format_utc(&ends)),
            maintenance_until: None,
            trial_ends_at: Some(format_utc(&ends)),
        }
    }

    fn snapshot(&self, status: LicenseStatus, payload: &LicensePayload) -> LicenseSnapshot {
        LicenseSnapshot {
            status,
            payload: Some(payload.clone()),
            entitlements: payload.entitlements.clone(),
            valid_until: payload.valid_until.clone(),
            maintenance_until: payload.maintenance_until.clone(),
            trial_ends_at: None,
        }
    }

    fn persist_state(&self, conn: &Connection, snap: &LicenseSnapshot) -> Result<()> {
        let license_type = match snap.payload.as_ref() {
            Some(p) => p.license_type.as_str().to_string(),
            None => {
                if snap.status == LicenseStatus::TRIAL_ACTIVE {
                    LICENSE_TRIAL.to_string()
                } else {
                    LICENSE_NONE.to_string()
                }
            }
        };
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
    use crate::constants::{ENT_POS_CORE, ENT_REPORT_STANDARD, LICENSE_ANNUAL};
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
                valid_until: if license_type == LICENSE_ANNUAL {
                    Some(valid_until.into())
                } else {
                    None
                },
                maintenance_until: Some(valid_until.into()),
                grace_period_days: grace,
                max_devices: 1,
                device_fingerprint: None,
                entitlements: vec![ENT_POS_CORE.into(), ENT_REPORT_STANDARD.into()],
                store_id: "S".into(),
                customer_name: "T".into(),
            };
            let json = serde_json::to_vec(&p).unwrap();
            let sig = self.sk.sign(&json);
            crate::license::payload::encode_license(&p, &sig.to_bytes()).unwrap()
        }
    }

    #[test]
    fn first_launch_starts_trial() {
        let f = Fixture::new("trial1");
        let svc = f.service("2026-01-01T00:00:00Z");
        let conn = f.conn();
        let snap = svc.verify_on_launch(&conn).unwrap();
        assert_eq!(snap.status, LicenseStatus::TRIAL_ACTIVE);
        assert!(snap.trial_ends_at.is_some());
        // 全功能：前台 + 后台均放行
        assert!(snap.gate("pos.core").is_ok());
        assert!(snap.gate("report.standard").is_ok());
        // 审计与持久化
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM license_audit WHERE event_type='TRIAL_STARTED'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
        let (state, ltype): (String, String) = conn
            .query_row(
                "SELECT state, license_type FROM license_state WHERE state_key='trial'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(state, "TRIAL_ACTIVE");
        assert_eq!(ltype, "TRIAL");
    }

    #[test]
    fn trial_persists_across_restart() {
        let f = Fixture::new("trial2");
        let svc = f.service("2026-01-01T00:00:00Z");
        let conn = f.conn();
        let s1 = svc.verify_on_launch(&conn).unwrap();
        let s2 = svc.verify_on_launch(&conn).unwrap();
        assert_eq!(s1.status, LicenseStatus::TRIAL_ACTIVE);
        assert_eq!(s2.status, LicenseStatus::TRIAL_ACTIVE);
        // 到期时间不变：重启不延长试用
        assert_eq!(s1.trial_ends_at, s2.trial_ends_at);
    }

    #[test]
    fn trial_expires_after_15_days() {
        let f = Fixture::new("trial3");
        let svc = f.service("2026-01-01T00:00:00Z");
        let conn = f.conn();
        svc.verify_on_launch(&conn).unwrap();
        // 模拟 16 天后：把试用记录的开始/到期时间整体前移
        let started = format_utc(&(Utc::now() - chrono::Duration::days(16)));
        let ended = format_utc(&(Utc::now() - chrono::Duration::days(1)));
        conn.execute(
            "UPDATE license_state SET updated_at=?1, valid_until=?2 WHERE state_key='trial'",
            rusqlite::params![started, ended],
        )
        .unwrap();
        let snap = svc.verify_on_launch(&conn).unwrap();
        assert_eq!(snap.status, LicenseStatus::UNACTIVATED);
        assert!(snap.gate("pos.core").is_err());
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM license_audit WHERE event_type='TRIAL_EXPIRED'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn trial_within_15_days_still_active() {
        let f = Fixture::new("trial4");
        let svc = f.service("2026-01-01T00:00:00Z");
        let conn = f.conn();
        svc.verify_on_launch(&conn).unwrap();
        let started = format_utc(&(Utc::now() - chrono::Duration::days(14)));
        let ended = format_utc(&(Utc::now() + chrono::Duration::days(1)));
        conn.execute(
            "UPDATE license_state SET updated_at=?1, valid_until=?2 WHERE state_key='trial'",
            rusqlite::params![started, ended],
        )
        .unwrap();
        let snap = svc.verify_on_launch(&conn).unwrap();
        assert_eq!(snap.status, LicenseStatus::TRIAL_ACTIVE);
    }

    #[test]
    fn activation_consumes_trial() {
        let f = Fixture::new("trial5");
        let svc = f.service("2026-01-01T00:00:00Z");
        let conn = f.conn();
        let trial = svc.verify_on_launch(&conn).unwrap();
        assert_eq!(trial.status, LicenseStatus::TRIAL_ACTIVE);
        let token = f.token("2027-06-01T00:00:00Z", 7, "ANNUAL");
        let snap = svc.activate(&conn, &token).unwrap();
        assert_eq!(snap.status, LicenseStatus::ACTIVE);
        // 试用记录已删除：授权失效后不得复活试用
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM license_state WHERE state_key='trial'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn unactivated_on_fresh_start() {
        let f = Fixture::new("fresh");
        let svc = f.service("2026-01-01T00:00:00Z");
        let conn = f.conn();
        // 试用到期后回到 UNACTIVATED：激活界面，收银不可达（文档 A-01）
        svc.verify_on_launch(&conn).unwrap();
        let started = format_utc(&(Utc::now() - chrono::Duration::days(16)));
        let ended = format_utc(&(Utc::now() - chrono::Duration::days(1)));
        conn.execute(
            "UPDATE license_state SET updated_at=?1, valid_until=?2 WHERE state_key='trial'",
            rusqlite::params![started, ended],
        )
        .unwrap();
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
        assert!(
            err.to_string().contains("签名")
                || err.to_string().contains("格式")
                || err.to_string().contains("Base64")
        );
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

    #[test]
    fn online_activation_records_activation_key() {
        let f = Fixture::new("onlinekey");
        let svc = f.service("2026-01-01T00:00:00Z");
        let conn = f.conn();
        let token = f.token("2027-06-01T00:00:00Z", 7, "ANNUAL");
        let snap = svc
            .activate_with_key(&conn, &token, Some("CKR-AAAA-BBBB-CCCC-DDDD"))
            .unwrap();
        assert_eq!(snap.status, LicenseStatus::ACTIVE);
        // 激活码已落库，后续可凭它联网刷新授权
        let bundle = svc.store.load().unwrap().unwrap();
        assert_eq!(
            bundle.activation_key.as_deref(),
            Some("CKR-AAAA-BBBB-CCCC-DDDD")
        );
    }

    #[test]
    fn offline_activation_has_no_activation_key() {
        let f = Fixture::new("offlinekey");
        let svc = f.service("2026-01-01T00:00:00Z");
        let conn = f.conn();
        let token = f.token("2027-06-01T00:00:00Z", 7, "ANNUAL");
        svc.activate(&conn, &token).unwrap();
        let bundle = svc.store.load().unwrap().unwrap();
        assert!(bundle.activation_key.is_none());
    }

    #[test]
    fn renew_keeps_activation_key() {
        let f = Fixture::new("renewkey");
        let svc = f.service("2026-01-01T00:00:00Z");
        let conn = f.conn();
        let old = f.token("2026-12-01T00:00:00Z", 3, "ANNUAL");
        svc.activate_with_key(&conn, &old, Some("CKR-KEEP-0000-0000-0000"))
            .unwrap();
        let new = f.token("2028-01-01T00:00:00Z", 7, "ANNUAL");
        svc.renew(&conn, &new).unwrap();
        let bundle = svc.store.load().unwrap().unwrap();
        assert_eq!(
            bundle.activation_key.as_deref(),
            Some("CKR-KEEP-0000-0000-0000"),
            "续期不得丢失激活码，否则后续无法联网刷新"
        );
    }

    #[test]
    fn revoked_marks_degraded_but_keeps_pos_core() {
        let f = Fixture::new("revoked");
        let svc = f.service("2026-01-01T00:00:00Z");
        let conn = f.conn();
        let token = f.token("2027-06-01T00:00:00Z", 7, "ANNUAL");
        svc.activate(&conn, &token).unwrap();
        // 服务端明确拒绝 → 本地降级
        let snap = svc.mark_revoked(&conn, "该激活码已停用").unwrap();
        assert_eq!(snap.status, LicenseStatus::OFFLINE_DEGRADED);
        assert!(snap.gate("pos.core").is_ok(), "降级不得阻断前台收银");
        assert!(snap.gate("report.standard").is_err());
        // 状态已落库，重启后不会又显示「已激活」
        let persisted: String = conn
            .query_row(
                "SELECT state FROM license_state WHERE state_key='current'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(persisted, "OFFLINE_DEGRADED");
    }

    #[test]
    fn refresh_without_activation_key_rejected() {
        let f = Fixture::new("norefresh");
        let svc = f.service("2026-01-01T00:00:00Z");
        let conn = f.conn();
        let token = f.token("2027-06-01T00:00:00Z", 7, "ANNUAL");
        svc.activate(&conn, &token).unwrap();
        // 离线导入的授权没有激活码，无从联网刷新
        let err = svc
            .refresh_online(&conn, "https://example.invalid", "S", "S")
            .unwrap_err();
        assert!(err.to_string().contains("在线激活"));
    }
}
