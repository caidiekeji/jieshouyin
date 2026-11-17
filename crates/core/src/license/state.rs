//! 授权状态机（§4.5）。所有判定以 effective_now 为时间基准。

use crate::error::{CoreError, Result};
use crate::time::parse_rfc3339;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use super::payload::LicensePayload;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[allow(non_camel_case_types)] // 状态/类型字符串与文档一致
pub enum LicenseStatus {
    UNACTIVATED,
    ACTIVE,
    GRACE_PERIOD,
    OFFLINE_DEGRADED,
}

impl LicenseStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            LicenseStatus::UNACTIVATED => "UNACTIVATED",
            LicenseStatus::ACTIVE => "ACTIVE",
            LicenseStatus::GRACE_PERIOD => "GRACE_PERIOD",
            LicenseStatus::OFFLINE_DEGRADED => "OFFLINE_DEGRADED",
        }
    }

    /// 前台六项生存性操作：除 UNACTIVATED 外一律允许。
    /// 这是架构硬约束，不可被任何授权路径违反。
    pub fn allows_pos_core(&self) -> bool {
        !matches!(self, LicenseStatus::UNACTIVATED)
    }

    /// 后台管理：降级态仅查看。
    pub fn allows_admin(&self) -> bool {
        matches!(self, LicenseStatus::ACTIVE | LicenseStatus::GRACE_PERIOD)
    }
}

/// 按 §4.6 判定状态。
pub fn decide_status(
    payload: &LicensePayload,
    effective_now: DateTime<Utc>,
) -> Result<LicenseStatus> {
    match payload.license_type.as_str() {
        "PERPETUAL" => Ok(LicenseStatus::ACTIVE),
        "ANNUAL" => {
            let valid_until = payload.valid_until.as_ref().ok_or(CoreError::License(
                crate::error::LicenseError::MissingField("valid_until".into()),
            ))?;
            let until = parse_rfc3339(valid_until)?;
            if effective_now < until {
                Ok(LicenseStatus::ACTIVE)
            } else {
                let overdue = effective_now.signed_duration_since(until);
                let grace = Duration::days(payload.grace_period_days as i64);
                if overdue <= grace {
                    Ok(LicenseStatus::GRACE_PERIOD)
                } else {
                    Ok(LicenseStatus::OFFLINE_DEGRADED)
                }
            }
        }
        other => Err(CoreError::License(crate::error::LicenseError::UnknownType(
            other.into(),
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::license::payload::LicensePayload;

    fn annual(grace: u32) -> LicensePayload {
        LicensePayload {
            license_id: "lic_t".into(),
            product: "checkout-r".into(),
            license_type: "ANNUAL".into(),
            issued_at: "2026-01-01T00:00:00Z".into(),
            valid_until: Some("2027-01-01T00:00:00Z".into()),
            maintenance_until: Some("2027-01-01T00:00:00Z".into()),
            grace_period_days: grace,
            max_devices: 1,
            device_fingerprint: None,
            entitlements: vec!["pos.core".into()],
            store_id: "S1".into(),
            customer_name: "t".into(),
        }
    }

    fn at(s: &str) -> DateTime<Utc> {
        parse_rfc3339(s).unwrap()
    }

    #[test]
    fn active_before_expiry() {
        assert_eq!(
            decide_status(&annual(7), at("2026-12-31T23:59:59Z")).unwrap(),
            LicenseStatus::ACTIVE
        );
    }

    #[test]
    fn grace_within_7_days() {
        assert_eq!(
            decide_status(&annual(7), at("2027-01-03T00:00:00Z")).unwrap(),
            LicenseStatus::GRACE_PERIOD
        );
        assert_eq!(
            decide_status(&annual(7), at("2027-01-08T00:00:00Z")).unwrap(),
            LicenseStatus::GRACE_PERIOD
        );
    }

    #[test]
    fn degraded_after_grace() {
        assert_eq!(
            decide_status(&annual(7), at("2027-01-08T00:00:01Z")).unwrap(),
            LicenseStatus::OFFLINE_DEGRADED
        );
        assert_eq!(
            decide_status(&annual(7), at("2027-02-01T00:00:00Z")).unwrap(),
            LicenseStatus::OFFLINE_DEGRADED
        );
    }

    #[test]
    fn perpetual_never_degrades() {
        let mut p = annual(0);
        p.license_type = "PERPETUAL".into();
        assert_eq!(
            decide_status(&p, at("2099-01-01T00:00:00Z")).unwrap(),
            LicenseStatus::ACTIVE
        );
    }

    #[test]
    fn pos_core_allowed_in_all_but_unactivated() {
        assert!(!LicenseStatus::UNACTIVATED.allows_pos_core());
        assert!(LicenseStatus::ACTIVE.allows_pos_core());
        assert!(LicenseStatus::GRACE_PERIOD.allows_pos_core());
        assert!(LicenseStatus::OFFLINE_DEGRADED.allows_pos_core());
    }

    #[test]
    fn degraded_blocks_admin_only() {
        assert!(LicenseStatus::GRACE_PERIOD.allows_admin());
        assert!(!LicenseStatus::OFFLINE_DEGRADED.allows_admin());
    }

    #[test]
    fn different_grace_periods() {
        assert_eq!(
            decide_status(&annual(3), at("2027-01-03T00:00:00Z")).unwrap(),
            LicenseStatus::OFFLINE_DEGRADED
        );
        assert_eq!(
            decide_status(&annual(30), at("2027-01-03T00:00:00Z")).unwrap(),
            LicenseStatus::GRACE_PERIOD
        );
    }
}
