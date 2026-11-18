//! 在线激活客户端（§4.7 激活方式之二）。
//!
//! 流程：客户端 → 激活服务（Cloudflare Worker）换取签名激活码 → 本地验签落库。
//! 关键纪律：**签发权只在服务端**；客户端拿到的激活码仍走本地验签，
//! 离线导入与在线激活共用同一条验签链路，客户端不含任何签发私钥。
//!
//! 网络不可达时不阻断离线导入路径（降级处理），由调用方决定提示。

use crate::error::{CoreError, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// 在线激活请求体。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnlineActivateRequest {
    /// 门店编号（由激活服务核验绑定关系）。
    pub store_id: String,
    /// 客户名称（写入载荷，便于后台识别）。
    pub customer_name: String,
    /// 设备指纹：绑定本机，服务端可据此限制换绑次数。
    pub device_fingerprint: Option<String>,
    /// 激活码（购买后获得的兑换码）。
    pub activation_key: String,
}

/// 服务端响应体。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnlineActivateResponse {
    /// 签名激活码（Base64URL(payload).Base64URL(sig)）。
    pub token: String,
    /// 授权到期日（展示用，不参与校验）。
    #[serde(default)]
    pub valid_until: Option<String>,
    /// 服务端提示信息。
    #[serde(default)]
    pub message: Option<String>,
}

/// 激活服务连接超时（秒）：收银机处于弱网环境时不应长时间挂起。
pub const ACTIVATION_TIMEOUT_SECS: u64 = 15;

/// 向激活服务请求签名激活码。返回激活码字符串。
pub fn request_activation(
    endpoint: &str,
    req: &OnlineActivateRequest,
) -> Result<OnlineActivateResponse> {
    let base = endpoint.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err(CoreError::Validation("未配置在线激活服务地址".into()));
    }
    if !(base.starts_with("http://") || base.starts_with("https://")) {
        return Err(CoreError::Validation(
            "激活服务地址必须以 http:// 或 https:// 开头".into(),
        ));
    }
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(ACTIVATION_TIMEOUT_SECS))
        .timeout_read(Duration::from_secs(ACTIVATION_TIMEOUT_SECS))
        .timeout_write(Duration::from_secs(ACTIVATION_TIMEOUT_SECS))
        .build();
    let body = serde_json::to_string(req)?;
    let resp = agent
        .post(&format!("{base}/api/activate"))
        .set("Content-Type", "application/json")
        .send_string(&body);
    match resp {
        Ok(r) => {
            let text = r.into_string().unwrap_or_default();
            serde_json::from_str::<OnlineActivateResponse>(&text)
                .map_err(|e| CoreError::Other(format!("激活服务返回内容无法解析: {e}")))
        }
        Err(ureq::Error::Status(code, r)) => {
            let t = r.into_string().unwrap_or_default();
            Err(CoreError::Other(format!("激活服务返回错误({code}): {t}")))
        }
        Err(e) => Err(CoreError::Other(format!("激活服务不可达: {e}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_endpoint_rejected() {
        let req = OnlineActivateRequest {
            store_id: "S1".into(),
            customer_name: "测试门店".into(),
            device_fingerprint: None,
            activation_key: "KEY".into(),
        };
        assert!(request_activation("", &req).is_err());
        assert!(request_activation("ftp://x", &req).is_err());
    }
}
