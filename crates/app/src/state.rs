//! 应用状态：数据库连接、授权快照、会话。

use checkout_core::db::{open_connection, run_migrations};
use checkout_core::license::{LicenseService, LicenseSnapshot};
use checkout_core::rbac::Session;
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct AppState {
    pub conn: Mutex<Connection>,
    pub license: Mutex<LicenseSnapshot>,
    pub session: Mutex<Option<Session>>,
    pub public_key: [u8; 32],
    pub db_path: PathBuf,
    pub license_path: PathBuf,
}

/// 构建授权服务：使用加密文件存储（keyring 不可用时的文档允许降级路径）。
pub fn build_license_service(state: &AppState) -> LicenseService {
    use checkout_core::license::fingerprint::device_fingerprint;
    use checkout_core::license::storage::EncryptedFileStore;
    use checkout_core::time::SystemClock;
    use std::sync::Arc;

    let fp = device_fingerprint().ok().flatten();
    let store = Arc::new(EncryptedFileStore::new(
        state.license_path.clone(),
        fp.as_deref(),
    ));
    let last_known = chrono::DateTime::from_timestamp(0, 0).unwrap();
    LicenseService::new(store, state.public_key, Box::new(SystemClock), last_known)
}

/// 初始化数据库（启动顺序：先连接 + 迁移，再许可证验证）。
pub fn init_db(path: &PathBuf) -> checkout_core::Result<Connection> {
    let mut conn = open_connection(path)?;
    run_migrations(&mut conn)?;
    Ok(conn)
}
