//! 支付商户凭据管理（§6.8）。
//!
//! 纪律：商户号 / API 密钥 / apiv3_key / 私钥一律存操作系统凭据库
//! （Windows Credential Manager），不落 SQLite、不进备份、不进日志。
//! 本模块只暴露"是否已配置"状态与加载结果，任何路径都不得返回明文。

use crate::error::{CoreError, Result};

/// 凭据库服务名：Windows Credential Manager 中以该前缀归组。
pub const SERVICE_NAME: &str = "checkout-r.pay";

// ── 微信支付凭据字段（§6.8）──────────────────────────────────
pub const W_MCHID: &str = "wechat.mchid";
pub const W_APPID: &str = "wechat.appid";
pub const W_SERIAL_NO: &str = "wechat.serial_no";
pub const W_APIV3_KEY: &str = "wechat.apiv3_key";
pub const W_PRIVATE_KEY: &str = "wechat.private_key_pem";
/// 微信支付平台公钥 ID（PUB_KEY_ID_...，公钥模式）与公钥 PEM：应答验签用。
pub const W_PLATFORM_KEY_ID: &str = "wechat.platform_key_id";
pub const W_PLATFORM_PUBLIC_KEY: &str = "wechat.platform_public_key_pem";

// ── 支付宝凭据字段（§6.8）────────────────────────────────────
pub const A_APP_ID: &str = "alipay.app_id";
pub const A_PRIVATE_KEY: &str = "alipay.private_key_pem";
/// 支付宝公钥 PEM：应答验签自持有（§6 验签纪律）。
pub const A_PUBLIC_KEY: &str = "alipay.public_key_pem";

/// 可写白名单：防止任意凭据项注入。命令层写入前必须校验。
pub const ALLOWED_FIELDS: [&str; 10] = [
    W_MCHID,
    W_APPID,
    W_SERIAL_NO,
    W_APIV3_KEY,
    W_PRIVATE_KEY,
    W_PLATFORM_KEY_ID,
    W_PLATFORM_PUBLIC_KEY,
    A_APP_ID,
    A_PRIVATE_KEY,
    A_PUBLIC_KEY,
];

/// 凭据存取抽象：生产 = 操作系统凭据库；测试 = 内存实现。
/// 抽象层的目的是让核心逻辑可测（CI 无凭据库），而非替换生产实现。
pub trait SecretStore: Send + Sync {
    fn set_secret(&self, account: &str, value: &str) -> Result<()>;
    fn get_secret(&self, account: &str) -> Result<Option<String>>;
    fn delete_secret(&self, account: &str) -> Result<()>;
}

/// 生产实现：Windows Credential Manager（keyring）。
pub struct KeyringStore;

#[cfg(windows)]
impl SecretStore for KeyringStore {
    fn set_secret(&self, account: &str, value: &str) -> Result<()> {
        let entry = keyring::Entry::new(SERVICE_NAME, account)
            .map_err(|e| CoreError::Other(format!("凭据库不可用: {e}")))?;
        entry
            .set_password(value)
            .map_err(|e| CoreError::Other(format!("凭据写入失败: {e}")))
    }

    fn get_secret(&self, account: &str) -> Result<Option<String>> {
        let entry = keyring::Entry::new(SERVICE_NAME, account)
            .map_err(|e| CoreError::Other(format!("凭据库不可用: {e}")))?;
        match entry.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(CoreError::Other(format!("凭据读取失败: {e}"))),
        }
    }

    fn delete_secret(&self, account: &str) -> Result<()> {
        let entry = keyring::Entry::new(SERVICE_NAME, account)
            .map_err(|e| CoreError::Other(format!("凭据库不可用: {e}")))?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(CoreError::Other(format!("凭据删除失败: {e}"))),
        }
    }
}

#[cfg(not(windows))]
impl SecretStore for KeyringStore {
    fn set_secret(&self, _account: &str, _value: &str) -> Result<()> {
        Err(CoreError::Other("凭据库仅支持 Windows（v1 目标平台）".into()))
    }
    fn get_secret(&self, _account: &str) -> Result<Option<String>> {
        Err(CoreError::Other("凭据库仅支持 Windows（v1 目标平台）".into()))
    }
    fn delete_secret(&self, _account: &str) -> Result<()> {
        Err(CoreError::Other("凭据库仅支持 Windows（v1 目标平台）".into()))
    }
}

/// 微信商户凭据集合。全部字段就绪才算"已配置"。
#[derive(Debug, Clone)]
pub struct WechatCred {
    pub appid: String,
    pub mchid: String,
    pub serial_no: String,
    pub v3_key: String,
    pub private_key_pem: String,
    pub platform_key_id: Option<String>,
    pub platform_public_key_pem: Option<String>,
}

/// 支付宝商户凭据集合。
#[derive(Debug, Clone)]
pub struct AlipayCred {
    pub app_id: String,
    pub private_key_pem: String,
    pub public_key_pem: String,
}

/// 加载微信凭据：任一必填缺失 → None（未配置），不半配置。
pub fn load_wechat(store: &dyn SecretStore) -> Result<Option<WechatCred>> {
    let mut vals: Vec<Option<String>> = Vec::new();
    for f in [W_MCHID, W_APPID, W_SERIAL_NO, W_APIV3_KEY, W_PRIVATE_KEY] {
        match store.get_secret(f)? {
            Some(v) if !v.trim().is_empty() => vals.push(Some(v)),
            _ => return Ok(None),
        }
    }
    let platform_key_id = store.get_secret(W_PLATFORM_KEY_ID)?;
    let platform_public_key_pem = store.get_secret(W_PLATFORM_PUBLIC_KEY)?;
    let (platform_key_id, platform_public_key_pem) = match (platform_key_id, platform_public_key_pem) {
        (Some(a), Some(b)) if !a.trim().is_empty() && !b.trim().is_empty() => (Some(a), Some(b)),
        _ => (None, None),
    };
    Ok(Some(WechatCred {
        mchid: vals[0].clone().unwrap(),
        appid: vals[1].clone().unwrap(),
        serial_no: vals[2].clone().unwrap(),
        v3_key: vals[3].clone().unwrap(),
        private_key_pem: vals[4].clone().unwrap(),
        platform_key_id,
        platform_public_key_pem,
    }))
}

/// 加载支付宝凭据。
pub fn load_alipay(store: &dyn SecretStore) -> Result<Option<AlipayCred>> {
    let mut vals: Vec<Option<String>> = Vec::new();
    for f in [A_APP_ID, A_PRIVATE_KEY, A_PUBLIC_KEY] {
        match store.get_secret(f)? {
            Some(v) if !v.trim().is_empty() => vals.push(Some(v)),
            _ => return Ok(None),
        }
    }
    Ok(Some(AlipayCred {
        app_id: vals[0].clone().unwrap(),
        private_key_pem: vals[1].clone().unwrap(),
        public_key_pem: vals[2].clone().unwrap(),
    }))
}

/// 渠道凭据字段清单（设置页展示用，只给字段名不给值）。
pub fn fields_of(channel: &str) -> &'static [&'static str] {
    if channel == "WECHAT" {
        &[
            W_MCHID,
            W_APPID,
            W_SERIAL_NO,
            W_APIV3_KEY,
            W_PRIVATE_KEY,
            W_PLATFORM_KEY_ID,
            W_PLATFORM_PUBLIC_KEY,
        ]
    } else {
        &[A_APP_ID, A_PRIVATE_KEY, A_PUBLIC_KEY]
    }
}

#[cfg(test)]
pub(crate) mod mem_store {
    //! 测试内存凭据库（仅测试编译）。
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    pub struct MemStore(pub Mutex<HashMap<String, String>>);

    impl SecretStore for MemStore {
        fn set_secret(&self, account: &str, value: &str) -> Result<()> {
            self.0
                .lock()
                .map_err(|e| CoreError::Other(e.to_string()))?
                .insert(account.into(), value.into());
            Ok(())
        }
        fn get_secret(&self, account: &str) -> Result<Option<String>> {
            Ok(self
                .0
                .lock()
                .map_err(|e| CoreError::Other(e.to_string()))?
                .get(account)
                .cloned())
        }
        fn delete_secret(&self, account: &str) -> Result<()> {
            self.0
                .lock()
                .map_err(|e| CoreError::Other(e.to_string()))?
                .remove(account);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mem_store::MemStore;

    #[test]
    fn wechat_cred_requires_all_required_fields() {
        let store = MemStore::default();
        assert!(load_wechat(&store).unwrap().is_none());
        store.set_secret(W_MCHID, "1900000001").unwrap();
        assert!(load_wechat(&store).unwrap().is_none());
        store.set_secret(W_APPID, "wx888").unwrap();
        store.set_secret(W_SERIAL_NO, "SER001").unwrap();
        store.set_secret(W_APIV3_KEY, "0123456789abcdef0123456789abcdef").unwrap();
        store.set_secret(W_PRIVATE_KEY, "-----BEGIN PRIVATE KEY-----").unwrap();
        let cred = load_wechat(&store).unwrap().unwrap();
        assert_eq!(cred.mchid, "1900000001");
        assert!(cred.platform_public_key_pem.is_none());
    }

    #[test]
    fn alipay_cred_loads_when_complete() {
        let store = MemStore::default();
        assert!(load_alipay(&store).unwrap().is_none());
        store.set_secret(A_APP_ID, "2021000000000000").unwrap();
        store.set_secret(A_PRIVATE_KEY, "-----BEGIN RSA PRIVATE KEY-----").unwrap();
        store.set_secret(A_PUBLIC_KEY, "-----BEGIN PUBLIC KEY-----").unwrap();
        let cred = load_alipay(&store).unwrap().unwrap();
        assert_eq!(cred.app_id, "2021000000000000");
    }

    #[test]
    fn delete_then_load_is_none() {
        let store = MemStore::default();
        store.set_secret(A_APP_ID, "x").unwrap();
        store.set_secret(A_PRIVATE_KEY, "y").unwrap();
        store.set_secret(A_PUBLIC_KEY, "z").unwrap();
        assert!(load_alipay(&store).unwrap().is_some());
        store.delete_secret(A_PUBLIC_KEY).unwrap();
        assert!(load_alipay(&store).unwrap().is_none());
    }

    #[test]
    fn whitelist_covers_all_fields() {
        for f in [W_MCHID, W_APPID, W_SERIAL_NO, W_APIV3_KEY, W_PRIVATE_KEY, W_PLATFORM_KEY_ID, W_PLATFORM_PUBLIC_KEY, A_APP_ID, A_PRIVATE_KEY, A_PUBLIC_KEY] {
            assert!(ALLOWED_FIELDS.contains(&f), "字段 {f} 未进白名单");
        }
    }
}
