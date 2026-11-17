//! 激活码签发领域逻辑。此 crate 只被独立 CLI 二进制依赖，
//! 客户端 checkout-app 不依赖它；私钥离线保管，不提交仓库。

use checkout_core::error::{CoreError, Result};
use checkout_core::license::payload::{encode_license, LicensePayload};
use ed25519_dalek::{Signer, SigningKey};

/// 从 32 字节种子加载签发私钥。
pub fn signing_key_from_hex(hex_key: &str) -> Result<SigningKey> {
    let bytes = hex::decode(hex_key).map_err(|e| CoreError::Other(format!("hex 解码失败: {e}")))?;
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| CoreError::Other("私钥须为 32 字节".into()))?;
    Ok(SigningKey::from_bytes(&arr))
}

/// 生成新密钥对（init 用）。返回 (私钥 hex, 公钥 hex)。
pub fn generate_keypair() -> (String, String) {
    let mut rng = rand::rngs::OsRng;
    let sk = SigningKey::generate(&mut rng);
    (
        hex::encode(sk.to_bytes()),
        hex::encode(sk.verifying_key().to_bytes()),
    )
}

/// 签发激活码。
pub fn issue(sk: &SigningKey, payload: &LicensePayload) -> Result<String> {
    let json = serde_json::to_vec(payload)?;
    let sig = sk.sign(&json);
    encode_license(payload, &sig.to_bytes())
}

/// 续期：复制原载荷，更新 valid_until / maintenance_until。
pub fn renew_payload(payload: &LicensePayload, valid_until: String, maintenance_until: String) -> LicensePayload {
    let mut p = payload.clone();
    p.valid_until = Some(valid_until);
    p.maintenance_until = Some(maintenance_until);
    p
}

/// 换绑：复制原载荷，写入新设备指纹。
pub fn rebind_payload(payload: &LicensePayload, new_fp: String) -> LicensePayload {
    let mut p = payload.clone();
    p.device_fingerprint = Some(new_fp);
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use checkout_core::license::payload::parse_and_verify;

    fn sample() -> LicensePayload {
        LicensePayload {
            license_id: "lic_01JTEST".into(),
            product: "checkout-r".into(),
            license_type: "ANNUAL".into(),
            issued_at: "2026-11-17T00:00:00Z".into(),
            valid_until: Some("2027-11-17T00:00:00Z".into()),
            maintenance_until: Some("2027-11-17T00:00:00Z".into()),
            grace_period_days: 7,
            max_devices: 1,
            device_fingerprint: None,
            entitlements: vec!["pos.core".into()],
            store_id: "STORE_001".into(),
            customer_name: "示例门店".into(),
        }
    }

    #[test]
    fn keypair_signs_and_verifies() {
        let (sk_hex, pk_hex) = generate_keypair();
        let sk = signing_key_from_hex(&sk_hex).unwrap();
        let pk: [u8; 32] = hex::decode(&pk_hex).unwrap().try_into().unwrap();
        let token = issue(&sk, &sample()).unwrap();
        let (parsed, _) = parse_and_verify(&token, &pk).unwrap();
        assert_eq!(parsed.license_id, "lic_01JTEST");
    }

    #[test]
    fn renew_and_rebind_keep_identity() {
        let p = sample();
        let renewed = renew_payload(&p, "2028-01-01T00:00:00Z".into(), "2028-01-01T00:00:00Z".into());
        assert_eq!(renewed.license_id, p.license_id);
        let rebound = rebind_payload(&p, "abcdef".into());
        assert_eq!(rebound.device_fingerprint.as_deref(), Some("abcdef"));
    }
}
