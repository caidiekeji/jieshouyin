//! 激活码载荷与编码（§4.2、§4.3）。
//! 激活码 = Base64URL(payload_json) + "." + Base64URL(signature)
//! 签名防篡改，非加密；载荷不得含明文敏感信息。

use crate::error::{CoreError, LicenseError, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

pub const MAX_LICENSE_LEN: usize = 4096;
pub const EXPECTED_PRODUCT: &str = "checkout-r";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LicensePayload {
    pub license_id: String,
    pub product: String,
    pub license_type: String,
    pub issued_at: String,
    pub valid_until: Option<String>,
    pub maintenance_until: Option<String>,
    pub grace_period_days: u32,
    pub max_devices: u32,
    pub device_fingerprint: Option<String>,
    pub entitlements: Vec<String>,
    pub store_id: String,
    #[serde(default)]
    pub customer_name: String,
}

/// 生成激活码字符串（签发侧使用，但不接触私钥）。
pub fn encode_license(payload: &LicensePayload, signature: &[u8; 64]) -> Result<String> {
    let json = serde_json::to_vec(payload)?;
    let body = URL_SAFE_NO_PAD.encode(json);
    let sig = URL_SAFE_NO_PAD.encode(signature);
    let token = format!("{body}.{sig}");
    if token.len() > MAX_LICENSE_LEN {
        return Err(LicenseError::TooLong.into());
    }
    Ok(token)
}

/// 解析并验证激活码。任何失败映射为明确错误，不 panic。
pub fn parse_and_verify(token: &str, public_key: &[u8; 32]) -> Result<(LicensePayload, [u8; 64])> {
    if token.len() > MAX_LICENSE_LEN {
        return Err(LicenseError::TooLong.into());
    }
    let (body, sig_b64) = token.split_once('.').ok_or(LicenseError::Malformed)?;
    let json = URL_SAFE_NO_PAD
        .decode(body)
        .map_err(|_| LicenseError::InvalidBase64)?;
    let sig_bytes = URL_SAFE_NO_PAD
        .decode(sig_b64)
        .map_err(|_| LicenseError::InvalidBase64)?;
    let sig_arr: [u8; 64] = sig_bytes.try_into().map_err(|_| LicenseError::Malformed)?;

    let vk = VerifyingKey::from_bytes(public_key).map_err(|_| LicenseError::SigningKey)?;
    let sig = Signature::from_bytes(&sig_arr).map_err(|_| LicenseError::SignatureInvalid)?;
    vk.verify(&json, &sig)
        .map_err(|_| LicenseError::SignatureInvalid)?;

    let payload: LicensePayload = serde_json::from_slice(&json)?;
    validate_payload(&payload)?;
    Ok((payload, sig_arr))
}

fn validate_payload(p: &LicensePayload) -> Result<()> {
    if p.product != EXPECTED_PRODUCT {
        return Err(CoreError::Validation(format!(
            "产品标识不匹配: {}",
            p.product
        )));
    }
    if p.license_id.is_empty() {
        return Err(LicenseError::MissingField("license_id".into()).into());
    }
    match p.license_type.as_str() {
        "PERPETUAL" | "ANNUAL" => {}
        other => return Err(LicenseError::UnknownType(other.into()).into()),
    }
    if p.license_type == "ANNUAL" && p.valid_until.is_none() {
        return Err(LicenseError::MissingField("valid_until".into()).into());
    }
    Ok(())
}

/// 激活码展示分组：每 64 字符换行。
pub fn format_for_display(token: &str) -> String {
    let chars: Vec<char> = token.chars().collect();
    chars
        .chunks(64)
        .map(|c| c.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use rand::rngs::OsRng;

    fn sample() -> LicensePayload {
        LicensePayload {
            license_id: "lic_01JTEST".into(),
            product: EXPECTED_PRODUCT.into(),
            license_type: "ANNUAL".into(),
            issued_at: "2026-11-17T00:00:00Z".into(),
            valid_until: Some("2027-11-17T00:00:00Z".into()),
            maintenance_until: Some("2027-11-17T00:00:00Z".into()),
            grace_period_days: 7,
            max_devices: 1,
            device_fingerprint: None,
            entitlements: vec!["pos.core".into(), "report.standard".into()],
            store_id: "STORE_001".into(),
            customer_name: "示例门店".into(),
        }
    }

    #[test]
    fn valid_license_roundtrip() {
        let mut rng = OsRng;
        let sk = SigningKey::generate(&mut rng);
        let payload = sample();
        let json = serde_json::to_vec(&payload).unwrap();
        let sig = sk.sign(&json);
        let token = encode_license(&payload, &sig.to_bytes()).unwrap();
        let (parsed, _) = parse_and_verify(&token, sk.verifying_key().as_bytes()).unwrap();
        assert_eq!(parsed, payload);
    }

    #[test]
    fn tampered_payload_rejected() {
        let sk = SigningKey::generate(&mut OsRng);
        let payload = sample();
        let json = serde_json::to_vec(&payload).unwrap();
        let sig = sk.sign(&json);
        let token = encode_license(&payload, &sig.to_bytes()).unwrap();
        let mut parts: Vec<&str> = token.split('.').collect();
        let mut decoded = URL_SAFE_NO_PAD.decode(parts[0]).unwrap();
        decoded[10] ^= 0xFF;
        parts[0] = Box::leak(URL_SAFE_NO_PAD.encode(decoded).into_boxed_str());
        let bad = parts.join(".");
        let err = parse_and_verify(&bad, sk.verifying_key().as_bytes()).unwrap_err();
        assert!(matches!(
            err,
            CoreError::License(LicenseError::SignatureInvalid)
        ));
    }

    #[test]
    fn wrong_public_key_rejected() {
        let sk = SigningKey::generate(&mut OsRng);
        let other = SigningKey::generate(&mut OsRng);
        let payload = sample();
        let json = serde_json::to_vec(&payload).unwrap();
        let sig = sk.sign(&json);
        let token = encode_license(&payload, &sig.to_bytes()).unwrap();
        let err = parse_and_verify(&token, other.verifying_key().as_bytes()).unwrap_err();
        assert!(matches!(
            err,
            CoreError::License(LicenseError::SignatureInvalid)
        ));
    }

    #[test]
    fn malformed_and_oversize_rejected() {
        let sk = SigningKey::generate(&mut OsRng);
        assert!(parse_and_verify("no-dot", sk.verifying_key().as_bytes()).is_err());
        assert!(parse_and_verify(&"x".repeat(4097), sk.verifying_key().as_bytes()).is_err());
    }

    #[test]
    fn display_groups_64() {
        let s = "a".repeat(130);
        let lines: Vec<&str> = format_for_display(&s).lines().collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].len(), 64);
        assert_eq!(lines[2].len(), 2);
    }
}
