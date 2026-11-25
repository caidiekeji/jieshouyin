//! 激活码载荷与编码（§4.2、§4.3）。
//! 激活码 = Base64URL(payload_json) + "." + Base64URL(signature)
//! 签名防篡改，非加密；载荷不得含明文敏感信息。

use crate::constants::{LICENSE_ANNUAL, LICENSE_PERPETUAL};
use crate::error::{CoreError, LicenseError, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

pub const MAX_LICENSE_LEN: usize = 4096;
pub const EXPECTED_PRODUCT: &str = "checkout-r";
/// 默认密钥编号：存量激活码没有 key_id 字段，按 1 号公钥验签，平滑兼容。
pub const DEFAULT_KEY_ID: u32 = 1;

fn default_key_id() -> u32 {
    DEFAULT_KEY_ID
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LicensePayload {
    /// 签发密钥编号，随载荷一起被签名，验签时据此选择公钥。
    #[serde(default = "default_key_id")]
    pub key_id: u32,
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

/// 解析并验证激活码，用默认 1 号公钥（单钥场景：签发工具、旧调用方）。
pub fn parse_and_verify(token: &str, public_key: &[u8; 32]) -> Result<(LicensePayload, [u8; 64])> {
    parse_and_verify_with_keys(token, &[(DEFAULT_KEY_ID, *public_key)])
}

/// 按载荷内的 key_id 从公钥环中选择验签公钥。
/// 未知编号直接拒绝（fail closed），这是密钥轮换能平滑进行的前提：
/// 新公钥先随客户端发布，jihuo 再切换签发编号，旧编号公钥保留一整个重叠窗口。
pub fn parse_and_verify_with_keys(
    token: &str,
    keys: &[(u32, [u8; 32])],
) -> Result<(LicensePayload, [u8; 64])> {
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

    // key_id 在验签前仅用于选钥匙；载荷完整性仍由随后对原始 json 的验签保证，
    // 篡改 key_id 只会导致选错钥匙 → 签名无效，不会被信任。
    #[derive(Deserialize)]
    struct KeyHint {
        #[serde(default = "default_key_id")]
        key_id: u32,
    }
    let hint: KeyHint = serde_json::from_slice(&json)?;
    let public_key = keys
        .iter()
        .find(|(id, _)| *id == hint.key_id)
        .map(|(_, k)| k)
        .ok_or(LicenseError::UnknownKeyId(hint.key_id))?;

    let vk = VerifyingKey::from_bytes(public_key).map_err(|_| LicenseError::SigningKey)?;
    let sig = Signature::from_bytes(&sig_arr);
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
        LICENSE_PERPETUAL | LICENSE_ANNUAL => {}
        other => return Err(LicenseError::UnknownType(other.into()).into()),
    }
    if p.license_type == LICENSE_ANNUAL && p.valid_until.is_none() {
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
    use rand::RngCore;

    fn test_key() -> SigningKey {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        SigningKey::from_bytes(&bytes)
    }

    fn sample() -> LicensePayload {
        LicensePayload {
            key_id: 1,
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
        let sk = test_key();
        let payload = sample();
        let json = serde_json::to_vec(&payload).unwrap();
        let sig = sk.sign(&json);
        let token = encode_license(&payload, &sig.to_bytes()).unwrap();
        let (parsed, _) = parse_and_verify(&token, sk.verifying_key().as_bytes()).unwrap();
        assert_eq!(parsed, payload);
    }

    #[test]
    fn tampered_payload_rejected() {
        let sk = test_key();
        let payload = sample();
        let json = serde_json::to_vec(&payload).unwrap();
        let sig = sk.sign(&json);
        let token = encode_license(&payload, &sig.to_bytes()).unwrap();
        let mut parts: Vec<&str> = token.split('.').collect();
        let mut decoded = URL_SAFE_NO_PAD.decode(parts[0]).unwrap();
        // 篡改 license_id 的 ASCII 值（避开开头的 key_id 字段，否则会先被 key_id 解析
        // 拦下返回 serde 错误；也避开末尾中文的 UTF-8 字节，避免非法 UTF-8）。
        // 签名覆盖整个载荷，改值必然验签失败。
        let pos = decoded
            .windows(4)
            .position(|w| w == b"lic_")
            .expect("sample 载荷应含 lic_")
            + 4;
        decoded[pos] = if decoded[pos] == b'X' { b'Y' } else { b'X' };
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
        let sk = test_key();
        let other = test_key();
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
        let sk = test_key();
        assert!(parse_and_verify("no-dot", sk.verifying_key().as_bytes()).is_err());
        assert!(parse_and_verify(&"x".repeat(4097), sk.verifying_key().as_bytes()).is_err());
    }

    #[test]
    fn display_groups_64() {
        let s = "a".repeat(130);
        let formatted = format_for_display(&s);
        let lines: Vec<&str> = formatted.lines().collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].len(), 64);
        assert_eq!(lines[2].len(), 2);
    }

    /// 轮换核心：公钥环按 payload.key_id 选钥匙，新旧钥匙共存时各自验签成功。
    #[test]
    fn key_ring_selects_by_id() {
        let sk1 = test_key();
        let sk2 = test_key();
        let ring = vec![
            (1u32, *sk1.verifying_key().as_bytes()),
            (2u32, *sk2.verifying_key().as_bytes()),
        ];

        let mut p1 = sample();
        p1.key_id = 1;
        let p2 = LicensePayload {
            key_id: 2,
            ..sample()
        };
        let t1 =
            encode_license(&p1, &sk1.sign(&serde_json::to_vec(&p1).unwrap()).to_bytes()).unwrap();
        let t2 =
            encode_license(&p2, &sk2.sign(&serde_json::to_vec(&p2).unwrap()).to_bytes()).unwrap();

        assert_eq!(
            parse_and_verify_with_keys(&t1, &ring).unwrap().0.key_id,
            1u32
        );
        assert_eq!(
            parse_and_verify_with_keys(&t2, &ring).unwrap().0.key_id,
            2u32
        );
        // 用错钥匙验签必须失败
        assert!(parse_and_verify_with_keys(&t2, &ring[..1]).is_err());
    }

    /// 未知编号 fail closed：不回退到默认公钥，直接拒绝。
    #[test]
    fn unknown_key_id_rejected() {
        let sk = test_key();
        let p = LicensePayload {
            key_id: 99,
            ..sample()
        };
        let token =
            encode_license(&p, &sk.sign(&serde_json::to_vec(&p).unwrap()).to_bytes()).unwrap();
        let ring = [(1u32, *sk.verifying_key().as_bytes())];
        let err = parse_and_verify_with_keys(&token, &ring).unwrap_err();
        assert!(matches!(
            err,
            CoreError::License(LicenseError::UnknownKeyId(99))
        ));
    }

    /// 存量激活码无 key_id 字段，按默认 1 号公钥验签通过。
    #[test]
    fn legacy_token_without_key_id_uses_default() {
        let sk = test_key();
        let v = serde_json::to_value(sample()).unwrap();
        let obj = v.as_object().unwrap();
        let mut legacy = serde_json::Map::new();
        for (k, val) in obj {
            if k != "key_id" {
                legacy.insert(k.clone(), val.clone());
            }
        }
        let json = serde_json::to_vec(&legacy).unwrap();
        let token = {
            let body = URL_SAFE_NO_PAD.encode(&json);
            let sig = URL_SAFE_NO_PAD.encode(sk.sign(&json).to_bytes());
            format!("{body}.{sig}")
        };
        let ring = [(1u32, *sk.verifying_key().as_bytes())];
        let parsed = parse_and_verify_with_keys(&token, &ring).unwrap().0;
        assert_eq!(parsed.key_id, DEFAULT_KEY_ID);
        assert_eq!(parsed.license_id, "lic_01JTEST");
    }
}
