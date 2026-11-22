//! RBAC 权限控制（§12）。门禁顺序：entitlement → 会话 → 权限 → 二次校验 → 限额。

use crate::error::{CoreError, Result};
use crate::license::LicenseSnapshot;
use crate::time::{format_utc, parse_rfc3339};
use argon2::password_hash::{PasswordHash, SaltString};
use argon2::{Algorithm, Argon2, Params, PasswordHasher, PasswordVerifier, Version};
use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, Connection, OptionalExtension};

/// argon2id 参数（§2、§12.3）：m=19456 KiB, t=2, p=1。
pub fn hash_password(password: &str) -> Result<String> {
    let params = Params::new(19456, 2, 1, None)
        .map_err(|e| CoreError::Other(format!("argon2 参数错误: {e}")))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let salt = SaltString::generate(&mut rand::rngs::OsRng);
    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| CoreError::Other(format!("密码哈希失败: {e}")))?;
    Ok(hash.to_string())
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    let parsed = match PasswordHash::new(hash) {
        Ok(p) => p,
        Err(_) => return false,
    };
    let argon2 = Argon2::default();
    argon2.verify_password(password.as_bytes(), &parsed).is_ok()
}

/// 校验管理员初始密码策略：≥8 位，含字母和数字。
pub fn validate_password_policy(password: &str) -> Result<()> {
    let has_letter = password.chars().any(|c| c.is_ascii_alphabetic());
    let has_digit = password.chars().any(|c| c.is_ascii_digit());
    if password.chars().count() < 8 || !has_letter || !has_digit {
        return Err(CoreError::Validation(
            "密码须 ≥8 位且同时包含字母和数字".into(),
        ));
    }
    Ok(())
}

/// 初始化管理员（仅 user_account 为空时调用）。
pub fn ensure_admin(conn: &Connection, username: &str, password: &str) -> Result<()> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM user_account", [], |r| r.get(0))?;
    if count > 0 {
        return Ok(());
    }
    validate_password_policy(password)?;
    let role_id: i64 = conn.query_row(
        "SELECT role_id FROM role WHERE role_name='ADMIN'",
        [],
        |r| r.get(0),
    )?;
    conn.execute(
        "INSERT INTO user_account(username,password_hash,role_id,status) VALUES (?1,?2,?3,1)",
        params![username, hash_password(password)?, role_id],
    )?;
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Session {
    pub user_id: i64,
    pub role_id: i64,
    pub role_name: String,
    pub login_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

/// 登录：连续失败 5 次锁定 5 分钟。
pub fn login(
    conn: &Connection,
    username: &str,
    password: &str,
    now: DateTime<Utc>,
) -> Result<Session> {
    let row: rusqlite::Result<(i64, String, i64, String, i32, Option<String>)> = conn.query_row(
        "SELECT u.user_id,u.password_hash,u.role_id,r.role_name,u.failed_attempts,u.locked_until
         FROM user_account u JOIN role r ON r.role_id=u.role_id
         WHERE u.username=?1 AND u.status=1",
        [username],
        |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
            ))
        },
    );
    let (user_id, hash, role_id, role_name, mut failed, locked) = match row {
        Ok(v) => v,
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return Err(CoreError::PermissionDenied("账号或密码错误".into()))
        }
        Err(e) => return Err(e.into()),
    };

    if let Some(lock) = locked {
        let lock_until = parse_rfc3339(&lock)?;
        if now < lock_until {
            return Err(CoreError::PermissionDenied("账号已锁定，请稍后再试".into()));
        }
        conn.execute(
            "UPDATE user_account SET failed_attempts=0, locked_until=NULL WHERE user_id=?1",
            [user_id],
        )?;
        // 内存里的计数也要清零：否则解禁后首次输错就被当成第 6 次，立刻再次锁死。
        failed = 0;
    }

    if !verify_password(password, &hash) {
        let failed = failed + 1;
        let lock = if failed >= 5 {
            Some(format_utc(&(now + Duration::minutes(5))))
        } else {
            None
        };
        conn.execute(
            "UPDATE user_account SET failed_attempts=?1, locked_until=?2 WHERE user_id=?3",
            params![failed, lock, user_id],
        )?;
        return Err(CoreError::PermissionDenied("账号或密码错误".into()));
    }

    conn.execute(
        "UPDATE user_account SET failed_attempts=0, locked_until=NULL WHERE user_id=?1",
        [user_id],
    )?;
    Ok(Session {
        user_id,
        role_id,
        role_name,
        login_at: now,
        expires_at: now + Duration::hours(12),
    })
}

/// 二次授权凭据：60 秒 TTL、一次性 nonce。
#[derive(Debug, Clone)]
pub struct SensitiveAuth {
    pub approver_id: i64,
    pub nonce: String,
    pub expires_at: DateTime<Utc>,
}

/// 敏感操作授权请求：校验授权人密码 + 权限，返回一次性凭据。
pub fn authorize_sensitive(
    conn: &Connection,
    approver_username: &str,
    approver_password: &str,
    permission: &str,
    current_nonce: &str,
    now: DateTime<Utc>,
) -> Result<SensitiveAuth> {
    // 二次授权只校验凭据，不累计失败次数：否则收银员可故意输错把店长账号锁死。
    let (user_id, role_id) = verify_credentials(conn, approver_username, approver_password)?;
    if !can(conn, role_id, permission)? {
        return Err(CoreError::ApproverPermissionDenied);
    }
    Ok(SensitiveAuth {
        approver_id: user_id,
        nonce: current_nonce.to_string(),
        expires_at: now + Duration::seconds(60),
    })
}

/// 校验账号密码但不写失败计数/锁定（二次授权专用）。
fn verify_credentials(conn: &Connection, username: &str, password: &str) -> Result<(i64, i64)> {
    let row: rusqlite::Result<(i64, String, i64, Option<String>)> = conn.query_row(
        "SELECT u.user_id,u.password_hash,u.role_id,u.locked_until
         FROM user_account u WHERE u.username=?1 AND u.status=1",
        [username],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    );
    let (user_id, hash, role_id, locked) = match row {
        Ok(v) => v,
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return Err(CoreError::PermissionDenied("账号或密码错误".into()))
        }
        Err(e) => return Err(e.into()),
    };
    if let Some(lock) = locked {
        if Utc::now() < parse_rfc3339(&lock)? {
            return Err(CoreError::PermissionDenied("账号已锁定，请稍后再试".into()));
        }
    }
    if !verify_password(password, &hash) {
        return Err(CoreError::PermissionDenied("账号或密码错误".into()));
    }
    Ok((user_id, role_id))
}

/// 查询角色是否拥有权限。
pub fn can(conn: &Connection, role_id: i64, permission: &str) -> Result<bool> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM role_permission WHERE role_id=?1 AND perm_key=?2",
        params![role_id, permission],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

/// 读取角色限额；NULL 或不存在 = 无限额。
pub fn role_limit(conn: &Connection, role_id: i64, limit_key: &str) -> Result<Option<i64>> {
    let v: Option<i64> = conn
        .query_row(
            "SELECT max_cents FROM role_limit WHERE role_id=?1 AND limit_key=?2",
            params![role_id, limit_key],
            |r| r.get(0),
        )
        .optional()?;
    Ok(v)
}

/// 门禁请求描述。
#[derive(Debug, Clone)]
pub struct GuardRequest<'a> {
    pub entitlement: &'a str,
    pub permission: &'a str,
    pub session: &'a Session,
    pub license: &'a LicenseSnapshot,
    pub sensitive_auth: Option<&'a SensitiveAuth>,
    pub current_nonce: &'a str,
    pub now: DateTime<Utc>,
    pub amount_cents: i64,
    pub limit_key: Option<&'a str>,
}

/// 统一门禁（§12.2）：顺序不可变更。
pub fn guard(conn: &Connection, req: &GuardRequest<'_>) -> Result<()> {
    // 1. 激活门禁
    req.license.gate(req.entitlement)?;

    // 2. 会话校验
    if req.session.expires_at < req.now {
        return Err(CoreError::SessionExpired);
    }
    // 会话期间账号被停用：必须立刻失效，不能靠 12 小时会话自然过期。
    let active: i64 = conn.query_row(
        "SELECT COUNT(*) FROM user_account WHERE user_id=?1 AND status=1",
        [req.session.user_id],
        |r| r.get(0),
    )?;
    if active == 0 {
        return Err(CoreError::PermissionDenied("账号已停用".into()));
    }

    // 3. RBAC 权限
    if !can(conn, req.session.role_id, req.permission)? {
        return Err(CoreError::PermissionDenied(req.permission.into()));
    }

    // 4. 敏感操作二次校验（TTL 60s + 单次 nonce）
    if let Some(auth) = req.sensitive_auth {
        if auth.expires_at <= req.now {
            return Err(CoreError::SensitiveAuthRequired);
        }
        if auth.nonce != req.current_nonce {
            return Err(CoreError::SensitiveAuthRequired); // 重放被拒
        }
        if !can(
            conn,
            role_of_approver(conn, auth.approver_id)?,
            req.permission,
        )? {
            return Err(CoreError::ApproverPermissionDenied);
        }
    }

    // 5. 限额校验：有二次授权时按批准人角色的限额（授权即提额），否则按操作员角色。
    if let Some(key) = req.limit_key {
        let limit_role = match req.sensitive_auth {
            Some(auth) => role_of_approver(conn, auth.approver_id)?,
            None => req.session.role_id,
        };
        if let Some(limit) = role_limit(conn, limit_role, key)? {
            if req.amount_cents > limit {
                return Err(CoreError::AmountLimitExceeded(format!(
                    "{key}: {} > {limit}",
                    req.amount_cents
                )));
            }
        }
    }
    Ok(())
}

fn role_of_approver(conn: &Connection, user_id: i64) -> Result<i64> {
    conn.query_row(
        "SELECT role_id FROM user_account WHERE user_id=?1",
        [user_id],
        |r| r.get(0),
    )
    .map_err(|e| e.into())
}

/// 写操作审计。
pub fn audit_operation(
    conn: &Connection,
    command: &str,
    operator_id: Option<i64>,
    approver_id: Option<i64>,
    detail: serde_json::Value,
    now: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO operation_audit(command,operator_id,approver_id,detail_json,created_at)
         VALUES (?1,?2,?3,?4,?5)",
        params![command, operator_id, approver_id, detail.to_string(), now],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{open_in_memory, run_migrations};
    use crate::license::{LicensePayload, LicenseSnapshot, LicenseStatus};
    use serde_json::json;

    fn setup() -> Connection {
        let mut c = open_in_memory().unwrap();
        run_migrations(&mut c).unwrap();
        ensure_admin(&c, "admin", "Admin123").unwrap();
        c
    }

    fn active_license() -> LicenseSnapshot {
        LicenseSnapshot {
            status: LicenseStatus::ACTIVE,
            payload: Some(LicensePayload {
                key_id: 1,
                license_id: "l".into(),
                product: "checkout-r".into(),
                license_type: "PERPETUAL".into(),
                issued_at: "2026-01-01T00:00:00Z".into(),
                valid_until: None,
                maintenance_until: None,
                grace_period_days: 0,
                max_devices: 1,
                device_fingerprint: None,
                entitlements: vec!["pos.core".into(), "report.standard".into()],
                store_id: "S".into(),
                customer_name: "T".into(),
            }),
            entitlements: vec!["pos.core".into(), "report.standard".into()],
            valid_until: None,
            maintenance_until: None,
            trial_ends_at: None,
        }
    }

    #[test]
    fn password_policy() {
        assert!(validate_password_policy("short1").is_err());
        assert!(validate_password_policy("abcdefgh").is_err());
        assert!(validate_password_policy("Admin123").is_ok());
    }

    #[test]
    fn argon2_hash_verifies() {
        let h = hash_password("Admin123").unwrap();
        assert!(h.starts_with("$argon2id$"));
        assert!(verify_password("Admin123", &h));
        assert!(!verify_password("wrong", &h));
    }

    #[test]
    fn login_success_and_failure() {
        let c = setup();
        let now = parse_rfc3339("2026-11-17T00:00:00Z").unwrap();
        let s = login(&c, "admin", "Admin123", now).unwrap();
        assert_eq!(s.role_name, "ADMIN");
        assert!(login(&c, "admin", "bad", now).is_err());
    }

    #[test]
    fn lock_after_five_failures() {
        let c = setup();
        let t0 = parse_rfc3339("2026-11-17T00:00:00Z").unwrap();
        for _ in 0..5 {
            let _ = login(&c, "admin", "bad", t0);
        }
        // 第 6 次即使密码正确也被锁定
        let err = login(&c, "admin", "Admin123", t0).unwrap_err();
        assert!(err.to_string().contains("锁定"));
    }

    #[test]
    fn cashier_refund_limit() {
        let c = setup();
        c.execute(
            "INSERT INTO user_account(username,password_hash,role_id) VALUES ('c','x',
             (SELECT role_id FROM role WHERE role_name='CASHIER'))",
            [],
        )
        .unwrap();
        let cashier_id: i64 = c
            .query_row(
                "SELECT user_id FROM user_account WHERE username='c'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let role_id: i64 = c
            .query_row(
                "SELECT role_id FROM user_account WHERE user_id=?1",
                [cashier_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            role_limit(&c, role_id, "refund.max_cents").unwrap(),
            Some(5000)
        );
        let admin_role: i64 = c
            .query_row(
                "SELECT role_id FROM role WHERE role_name='ADMIN'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            role_limit(&c, admin_role, "refund.max_cents").unwrap(),
            None
        );
    }

    #[test]
    fn guard_order_entitlement_first() {
        let c = setup();
        let now = parse_rfc3339("2026-11-17T00:00:00Z").unwrap();
        let session = login(&c, "admin", "Admin123", now).unwrap();
        let lic = active_license();
        let req = GuardRequest {
            entitlement: "no.such.ent",
            permission: "pos.checkout",
            session: &session,
            license: &lic,
            sensitive_auth: None,
            current_nonce: "n1",
            now,
            amount_cents: 0,
            limit_key: None,
        };
        match guard(&c, &req) {
            Err(CoreError::EntitlementDenied(_)) => {}
            other => panic!("期望 EntitlementDenied, 得到 {other:?}"),
        }
    }

    #[test]
    fn guard_sensitive_nonce_replay_rejected() {
        let c = setup();
        let now = parse_rfc3339("2026-11-17T00:00:00Z").unwrap();
        let session = login(&c, "admin", "Admin123", now).unwrap();
        let lic = active_license();
        let auth = authorize_sensitive(&c, "admin", "Admin123", "pos.void", "nonce1", now).unwrap();
        let mut req = GuardRequest {
            entitlement: "pos.core",
            permission: "pos.void",
            session: &session,
            license: &lic,
            sensitive_auth: Some(&auth),
            current_nonce: "nonce1",
            now,
            amount_cents: 0,
            limit_key: None,
        };
        assert!(guard(&c, &req).is_ok());
        req.current_nonce = "nonce2";
        assert!(matches!(
            guard(&c, &req),
            Err(CoreError::SensitiveAuthRequired)
        ));
    }

    #[test]
    fn guard_amount_limit() {
        let c = setup();
        let now = parse_rfc3339("2026-11-17T00:00:00Z").unwrap();
        c.execute(
            "INSERT INTO user_account(username,password_hash,role_id) VALUES ('c','x',
             (SELECT role_id FROM role WHERE role_name='CASHIER'))",
            [],
        )
        .unwrap();
        let cs = login(&c, "c", "", now);
        assert!(cs.is_err()); // 密码非空校验失败路径
        let lic = active_license();
        let session = Session {
            user_id: 1,
            role_id: 1,
            role_name: "CASHIER".into(),
            login_at: now,
            expires_at: now + Duration::hours(1),
        };
        let req = GuardRequest {
            entitlement: "pos.core",
            permission: "pos.refund",
            session: &session,
            license: &lic,
            sensitive_auth: None,
            current_nonce: "n",
            now,
            amount_cents: 6000,
            limit_key: Some("refund.max_cents"),
        };
        // CASHIER(role_id=1) 有 pos.refund 权限，限额 refund.max_cents=5000
        assert!(guard(&c, &req).is_err(), "6000 > 5000 必须触发限额");
        let req_ok = GuardRequest {
            amount_cents: 5000, // 等于限额：允许（超限才是拒绝）
            ..req
        };
        assert!(guard(&c, &req_ok).is_ok());
    }

    #[test]
    fn json_macro_used() {
        let v = json!({"a": 1});
        assert_eq!(v["a"], 1);
    }
}
