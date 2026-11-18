//! 激活授权子系统（§4）。架构级硬约束：授权校验失败只影响后台管理，
//! 任何代码路径不得以授权为由阻断前台六项生存性操作。

pub mod audit;
pub mod fingerprint;
pub mod payload;
pub mod service;
pub mod state;
pub mod storage;

pub use audit::AuditEvent;
pub use fingerprint::device_fingerprint;
pub use payload::{encode_license, parse_and_verify, LicensePayload};
pub use service::{LicenseService, LicenseSnapshot};
pub use state::{decide_status, LicenseStatus};
pub use storage::{LicenseBundle, SecureStore};

/// 前台生存性操作所需 entitlement。任何非 UNACTIVATED 状态都不得阻断。
/// 值定义在 constants.rs，此处重导出以保持 `license::ENT_POS_CORE` 兼容。
pub use crate::constants::ENT_POS_CORE;
