//! 统一错误类型。所有领域错误都映射为明确变体，禁止用 panic 处理可预期失败。

use serde::Serializer;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("数据库错误: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("序列化错误: {0}")]
    Json(#[from] serde_json::Error),

    #[error("时间解析错误: {0}")]
    Time(String),

    #[error("许可证: {0}")]
    License(#[from] LicenseError),

    #[error("权限拒绝: {0}")]
    PermissionDenied(String),

    #[error("权限不足: {0}")]
    EntitlementDenied(String),

    #[error("未认证")]
    NotAuthenticated,

    #[error("会话已过期")]
    SessionExpired,

    #[error("敏感操作需要二次校验")]
    SensitiveAuthRequired,

    #[error("敏感操作二次校验失败")]
    SensitiveAuthFailed,

    #[error("授权人权限不足")]
    ApproverPermissionDenied,

    #[error("超出限额: {0}")]
    AmountLimitExceeded(String),

    #[error("业务规则校验失败: {0}")]
    Validation(String),

    #[error("金额运算失败: {0}")]
    Money(String),

    #[error("打印失败: {0}")]
    Print(String),

    #[error("支付渠道错误: {0}")]
    Payment(String),

    #[error("安全存储错误: {0}")]
    SecureStorage(String),

    #[error("未知变量: {0}")]
    UnknownVariable(String),

    #[error("其他: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, CoreError>;

/// Tauri IPC 要求命令错误可序列化：统一序列化为人类可读的错误消息字符串，
/// 前端按 { message } 形态接收（Tauri 2 对 Serialize 错误会包装为字符串值）。
impl serde::Serialize for CoreError {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

#[derive(Debug, Error)]
pub enum LicenseError {
    #[error("激活码格式非法")]
    Malformed,
    #[error("激活码过长")]
    TooLong,
    #[error("Base64 解码失败")]
    InvalidBase64,
    #[error("签名无效")]
    SignatureInvalid,
    #[error("设备指纹不匹配")]
    FingerprintMismatch,
    #[error("许可证类型未知: {0}")]
    UnknownType(String),
    #[error("载荷字段缺失: {0}")]
    MissingField(String),
    #[error("未激活")]
    Unactivated,
    #[error("签名密钥错误")]
    SigningKey,
}
