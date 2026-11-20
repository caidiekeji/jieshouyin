//! 安全存储（§4.8）：keyring 首选 → 加密文件降级 → 明文调试。
//! 权威源 = 安全存储中的签名载荷；license_local 表仅为镜像缓存。

use crate::error::{CoreError, Result};
use crate::time::{format_utc, parse_rfc3339};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::payload::LicensePayload;

/// 存储内容：签名完整载荷 + 设备指纹 + 激活时间 + 单调水位。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LicenseBundle {
    pub token: String,
    pub payload: LicensePayload,
    pub signature: Vec<u8>,
    pub device_fingerprint: Option<String>,
    pub activated_at: String,
    pub last_known_time: String,
    /// 在线激活时使用的激活码（CKR-XXXX-XXXX-XXXX-XXXX）。
    /// 保存它是为了后续能联网向激活服务刷新授权（撤销/过期/续期以服务端为准）；
    /// 离线导入的授权没有激活码，此处为空。
    #[serde(default)]
    pub activation_key: Option<String>,
}

impl LicenseBundle {
    pub fn last_known_dt(&self) -> Result<DateTime<Utc>> {
        parse_rfc3339(&self.last_known_time)
    }
}

/// 安全存储抽象。
pub trait SecureStore: Send + Sync {
    fn load(&self) -> Result<Option<LicenseBundle>>;
    fn save(&self, bundle: &LicenseBundle) -> Result<()>;
}

/// 加密文件存储（ChaCha20-Poly1305）。
/// 密钥由设备指纹 + 内嵌盐派生；非 Windows/无指纹时用固定派生串，仍可加解密。
pub struct EncryptedFileStore {
    path: std::path::PathBuf,
    key_material: String,
}

const EMBEDDED_SALT: &str = "checkout-r-license-storage-v1";

impl EncryptedFileStore {
    pub fn new(path: std::path::PathBuf, fingerprint: Option<&str>) -> Self {
        let key_material = match fingerprint {
            Some(f) => format!("{EMBEDDED_SALT}:{f}"),
            None => format!("{EMBEDDED_SALT}:UNBOUND"),
        };
        Self { path, key_material }
    }

    fn derive_key(&self) -> [u8; 32] {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(self.key_material.as_bytes());
        let arr: [u8; 32] = h.finalize().into();
        arr
    }
}

impl SecureStore for EncryptedFileStore {
    fn load(&self) -> Result<Option<LicenseBundle>> {
        use chacha20poly1305::aead::{Aead, KeyInit};
        use chacha20poly1305::{ChaCha20Poly1305, Nonce};

        let bytes = match std::fs::read(&self.path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(CoreError::SecureStorage(e.to_string())),
        };
        if bytes.len() < 12 {
            return Err(CoreError::SecureStorage("加密许可证文件过短".into()));
        }
        let (nonce_bytes, ciphertext) = bytes.split_at(12);
        let cipher = ChaCha20Poly1305::new_from_slice(&self.derive_key())
            .map_err(|e| CoreError::SecureStorage(e.to_string()))?;
        let nonce = Nonce::from_slice(nonce_bytes);
        let plain = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| CoreError::SecureStorage("许可证文件解密失败".into()))?;
        let bundle: LicenseBundle = serde_json::from_slice(&plain)?;
        Ok(Some(bundle))
    }

    fn save(&self, bundle: &LicenseBundle) -> Result<()> {
        use chacha20poly1305::aead::{Aead, KeyInit, OsRng};
        use chacha20poly1305::{ChaCha20Poly1305, Nonce};
        use rand::RngCore;

        let plain = serde_json::to_vec(bundle)?;
        let cipher = ChaCha20Poly1305::new_from_slice(&self.derive_key())
            .map_err(|e| CoreError::SecureStorage(e.to_string()))?;
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, plain.as_slice())
            .map_err(|e| CoreError::SecureStorage(e.to_string()))?;
        let mut out = Vec::with_capacity(12 + ciphertext.len());
        out.extend_from_slice(&nonce_bytes);
        out.extend_from_slice(&ciphertext);
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| CoreError::SecureStorage(e.to_string()))?;
        }
        std::fs::write(&self.path, out).map_err(|e| CoreError::SecureStorage(e.to_string()))?;
        Ok(())
    }
}

/// 明文调试存储（仅极端降级/测试）。签名验证始终是终极校验。
pub struct PlainStore {
    path: std::path::PathBuf,
}

impl PlainStore {
    pub fn new(path: std::path::PathBuf) -> Self {
        Self { path }
    }
}

impl SecureStore for PlainStore {
    fn load(&self) -> Result<Option<LicenseBundle>> {
        let bytes = match std::fs::read(&self.path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(CoreError::SecureStorage(e.to_string())),
        };
        Ok(Some(serde_json::from_slice(&bytes)?))
    }

    fn save(&self, bundle: &LicenseBundle) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| CoreError::SecureStorage(e.to_string()))?;
        }
        std::fs::write(&self.path, serde_json::to_vec(bundle)?)
            .map_err(|e| CoreError::SecureStorage(e.to_string()))
    }
}

#[cfg(windows)]
pub struct KeyringStore {
    service: String,
    user: String,
}

#[cfg(windows)]
impl KeyringStore {
    pub fn new() -> Self {
        Self {
            service: "CheckoutR".into(),
            user: "license".into(),
        }
    }
}

#[cfg(windows)]
impl Default for KeyringStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(windows)]
impl SecureStore for KeyringStore {
    fn load(&self) -> Result<Option<LicenseBundle>> {
        use keyring::Entry;
        let entry = Entry::new(&self.service, &self.user)
            .map_err(|e| CoreError::SecureStorage(e.to_string()))?;
        match entry.get_password() {
            Ok(s) => Ok(Some(serde_json::from_str(&s)?)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(CoreError::SecureStorage(e.to_string())),
        }
    }

    fn save(&self, bundle: &LicenseBundle) -> Result<()> {
        use keyring::Entry;
        let entry = Entry::new(&self.service, &self.user)
            .map_err(|e| CoreError::SecureStorage(e.to_string()))?;
        entry
            .set_password(&serde_json::to_string(bundle)?)
            .map_err(|e| CoreError::SecureStorage(e.to_string()))
    }
}

/// 把安全存储中的权威载荷镜像写入 license_local 表。
pub fn mirror_to_db(conn: &rusqlite::Connection, bundle: &LicenseBundle) -> Result<()> {
    conn.execute(
        "INSERT INTO license_local(license_id,payload_json,signature,device_fingerprint,activated_at,last_known_time,last_verified_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7)
         ON CONFLICT(license_id) DO UPDATE SET
           payload_json=excluded.payload_json, signature=excluded.signature,
           device_fingerprint=excluded.device_fingerprint,
           activated_at=excluded.activated_at, last_known_time=excluded.last_known_time,
           last_verified_at=excluded.last_verified_at",
        rusqlite::params![
            bundle.payload.license_id,
            serde_json::to_string(&bundle.payload)?,
            hex::encode(&bundle.signature),
            bundle.device_fingerprint,
            bundle.activated_at,
            bundle.last_known_time,
            format_utc(&Utc::now()),
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::license::payload::LicensePayload;

    fn bundle() -> LicenseBundle {
        LicenseBundle {
            token: "a.b".into(),
            payload: LicensePayload {
                license_id: "lic_x".into(),
                product: "checkout-r".into(),
                license_type: "PERPETUAL".into(),
                issued_at: "2026-01-01T00:00:00Z".into(),
                valid_until: None,
                maintenance_until: None,
                grace_period_days: 0,
                max_devices: 1,
                device_fingerprint: None,
                entitlements: vec!["pos.core".into()],
                store_id: "S".into(),
                customer_name: "C".into(),
            },
            signature: vec![7u8; 64],
            device_fingerprint: None,
            activated_at: "2026-01-01T00:00:00Z".into(),
            last_known_time: "2026-01-01T00:00:00Z".into(),
            activation_key: None,
        }
    }

    #[test]
    fn encrypted_file_roundtrip() {
        let dir = std::env::temp_dir().join(format!("cr_license_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = EncryptedFileStore::new(dir.join("license.enc"), Some("fp123"));
        assert!(store.load().unwrap().is_none());
        let b = bundle();
        store.save(&b).unwrap();
        let loaded = store.load().unwrap().unwrap();
        assert_eq!(loaded, b);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn encrypted_file_rejects_corruption() {
        let dir = std::env::temp_dir().join(format!("cr_license_corrupt_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("license.enc");
        let store = EncryptedFileStore::new(path.clone(), None);
        let b = bundle();
        store.save(&b).unwrap();
        let mut bytes = std::fs::read(&path).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xFF;
        std::fs::write(&path, bytes).unwrap();
        assert!(store.load().is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mirror_to_db_upserts() {
        let mut conn = crate::db::open_in_memory().unwrap();
        crate::db::run_migrations(&mut conn).unwrap();
        let b = bundle();
        mirror_to_db(&conn, &b).unwrap();
        mirror_to_db(&conn, &b).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM license_local", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }
}
