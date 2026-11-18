//! 支付宝适配器（§6）：自封装 RSA2 签名（官方规范，验签自持有）。
//!
//! 接口：alipay.trade.pay（付款码 bar_code）/ query / cancel / refund。
//! 签名：非空参数按 key 字典序 k=v&k2=v2（不含 sign/sign_type 为空值），
//! SHA256WithRSA + Base64；应答验签使用支付宝公钥（凭据库）。
//! total_amount 是元字符串，必须由分转换，禁止浮点。

use crate::error::{CoreError, Result};
use crate::paycreds::AlipayCred;
use crate::payment::{CancelResult, PayOutcome, PayRequest, PaymentGateway, PaymentState};
use rsa::pkcs1v15::{Signature, SigningKey, VerifyingKey};
use rsa::pkcs8::DecodePrivateKey;
use rsa::signature::{RandomizedSigner, SignatureEncoding, Verifier};
use rsa::{RsaPrivateKey, RsaPublicKey};
use serde_json::Value;
use sha2::Sha256;

const GATEWAY_URL: &str = "https://openapi.alipay.com/gateway.do";

pub struct AlipayGateway {
    app_id: String,
    private_key: RsaPrivateKey,
    alipay_public_key: RsaPublicKey,
}

impl AlipayGateway {
    pub fn from_cred(cred: &AlipayCred) -> Result<Self> {
        let private_key = RsaPrivateKey::from_pkcs8_pem(&cred.private_key_pem)
            .or_else(|_| rsa::pkcs1::DecodeRsaPrivateKey::from_pkcs1_pem(&cred.private_key_pem))
            .map_err(|e| CoreError::Other(format!("支付宝商户私钥解析失败: {e}")))?;
        let alipay_public_key =
            rsa::pkcs8::DecodePublicKey::from_public_key_pem(&cred.public_key_pem)
                .map_err(|e| CoreError::Other(format!("支付宝公钥解析失败: {e}")))?;
        Ok(Self {
            app_id: cred.app_id.clone(),
            private_key,
            alipay_public_key,
        })
    }

    /// 统一请求：构造公共参数 → 签名 → 表单提交 → 验签 → 返回响应对象。
    fn call(&self, method: &str, biz: &Value) -> Result<Value> {
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let mut params: Vec<(String, String)> = vec![
            ("app_id".into(), self.app_id.clone()),
            ("method".into(), method.into()),
            ("format".into(), "JSON".into()),
            ("charset".into(), "UTF-8".into()),
            ("sign_type".into(), "RSA2".into()),
            ("timestamp".into(), timestamp),
            ("version".into(), "1.0".into()),
            ("biz_content".into(), serde_json::to_string(biz)?),
        ];
        let sign = self.sign(&params)?;
        params.push(("sign".into(), sign));

        let form: Vec<String> = params
            .iter()
            .map(|(k, v)| format!("{}={}", k, percent_encode(v)))
            .collect();
        let body = form.join("&");

        let resp = ureq::post(GATEWAY_URL)
            .set("Content-Type", "application/x-www-form-urlencoded")
            .send_string(&body);
        let text = match resp {
            Ok(r) => r.into_string().unwrap_or_default(),
            Err(ureq::Error::Status(code, r)) => {
                // 支付宝网关异常也返回 JSON 信封
                let t = r.into_string().unwrap_or_default();
                return Err(CoreError::Other(format!("支付宝网关错误({code}): {t}")));
            }
            Err(e) => return Err(CoreError::Other(format!("支付宝网络异常: {e}"))),
        };

        let root: Value = serde_json::from_str(&text)
            .map_err(|e| CoreError::Other(format!("支付宝应答解析失败: {e}")))?;
        let resp_key = root
            .as_object()
            .and_then(|o| {
                o.keys()
                    .find(|k| k.ends_with("_response") && k != "error_response")
            })
            .cloned()
            .ok_or_else(|| CoreError::Other(format!("支付宝应答缺少响应对象: {text}")))?;
        let resp_obj = root.get(&resp_key).cloned().unwrap_or(Value::Null);

        // 应答验签：对响应对象的原始序列化串验签（§6 验签自持有纪律）
        let sign_b64 = root
            .get("sign")
            .and_then(|s| s.as_str())
            .ok_or_else(|| CoreError::Other("支付宝应答缺少签名".into()))?;
        let msg = serde_json::to_string(&resp_obj)?;
        self.verify(&msg, sign_b64)?;

        Ok(resp_obj)
    }

    /// 按字典序拼接非空参数并 RSA2 签名。
    fn sign(&self, params: &[(String, String)]) -> Result<String> {
        let mut kv: Vec<String> = params
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(k, v)| format!("{k}={v}"))
            .collect();
        kv.sort();
        let msg = kv.join("&");
        let sk = SigningKey::<Sha256>::new(self.private_key.clone());
        let sig = sk.sign(&mut rand::rngs::OsRng, msg.as_bytes());
        Ok(sig.to_string())
    }

    fn verify(&self, msg: &str, sign_b64: &str) -> Result<()> {
        use base64::Engine;
        let raw = base64::engine::general_purpose::STANDARD
            .decode(sign_b64)
            .map_err(|e| CoreError::Other(format!("支付宝签名解码失败: {e}")))?;
        let sig = Signature::try_from(raw.as_slice())
            .map_err(|e| CoreError::Other(format!("支付宝签名格式错误: {e}")))?;
        let vk = VerifyingKey::<Sha256>::new(self.alipay_public_key.clone());
        vk.verify(msg.as_bytes(), &sig)
            .map_err(|_| CoreError::Other("支付宝应答验签失败，拒绝确认".into()))
    }
}

/// 交易结果统一判定：code=10000 且 trade_status 为成功态。
fn state_of(resp: &Value) -> Result<PayOutcome> {
    let code = resp.get("code").and_then(|c| c.as_str()).unwrap_or("");
    let sub_code = resp.get("sub_code").and_then(|c| c.as_str()).unwrap_or("");
    let sub_msg = resp.get("sub_msg").and_then(|c| c.as_str()).unwrap_or("");
    let trade_no = resp
        .get("trade_no")
        .and_then(|t| t.as_str())
        .map(|s| s.to_string());

    if code == "10000" {
        let status = resp
            .get("trade_status")
            .and_then(|s| s.as_str())
            .unwrap_or("");
        let state = match status {
            "TRADE_SUCCESS" | "TRADE_FINISHED" => PaymentState::SUCCESS,
            "WAIT_BUYER_PAY" => PaymentState::USERPAYING,
            "TRADE_CLOSED" => PaymentState::CLOSED,
            "" => PaymentState::SUCCESS, // cancel/refund 接口无 trade_status，code=10000 即成功
            other => PaymentState::FAILED(format!("支付宝交易状态 {other}")),
        };
        return Ok(PayOutcome {
            state,
            trade_no,
            code_url: None,
        });
    }

    match sub_code {
        // 明确业务结果：非 Err，避免触发不必要的查单兜底
        "ACQ.TRADE_HAS_SUCCESS" => Ok(PayOutcome {
            state: PaymentState::SUCCESS,
            trade_no,
            code_url: None,
        }),
        "ACQ.TRADE_NOT_EXIST" | "ACQ.TRADE_STATUS_ERROR" => Ok(PayOutcome {
            state: PaymentState::CLOSED,
            trade_no,
            code_url: None,
        }),
        other => {
            let _ = other;
            Err(CoreError::Other(if sub_msg.is_empty() {
                format!("支付宝业务拒绝({code})")
            } else {
                format!("支付宝业务拒绝({sub_code}): {sub_msg}")
            }))
        }
    }
}

impl PaymentGateway for AlipayGateway {
    fn micropay(&self, req: &PayRequest) -> Result<PayOutcome> {
        let auth_code = req
            .auth_code
            .clone()
            .ok_or_else(|| CoreError::Validation("缺少顾客付款码".into()))?;
        let biz = serde_json::json!({
            "out_trade_no": req.out_trade_no,
            "scene": "bar_code",
            "auth_code": auth_code,
            "subject": "门店收银",
            "total_amount": crate::money::cents_to_yuan(req.amount_cents),
        });
        let resp = self.call("alipay.trade.pay", &biz)?;
        state_of(&resp)
    }

    fn native_precreate(&self, req: &PayRequest) -> Result<PayOutcome> {
        // 支付宝二维码支付：预下单生成二维码（qr_code_offline_status 场景），
        // 使用 alipay.trade.precreate（当面付扫码）
        let biz = serde_json::json!({
            "out_trade_no": req.out_trade_no,
            "subject": "门店收银",
            "total_amount": crate::money::cents_to_yuan(req.amount_cents),
        });
        let resp = self.call("alipay.trade.precreate", &biz)?;
        let code_url = resp
            .get("qr_code")
            .and_then(|c| c.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| CoreError::Other("支付宝预下单未返回二维码".into()))?;
        Ok(PayOutcome {
            state: PaymentState::USERPAYING,
            trade_no: None,
            code_url: Some(code_url),
        })
    }

    fn query(&self, out_trade_no: &str) -> Result<PaymentState> {
        let biz = serde_json::json!({ "out_trade_no": out_trade_no });
        let resp = self.call("alipay.trade.query", &biz)?;
        Ok(state_of(&resp)?.state)
    }

    fn cancel(&self, out_trade_no: &str) -> Result<CancelResult> {
        let biz = serde_json::json!({ "out_trade_no": out_trade_no });
        let resp = self.call("alipay.trade.cancel", &biz)?;
        // retry_flag=Y 表示交易已支付、需异步退款处理 → 撤销遇已支付
        let retry = resp
            .get("retry_flag")
            .and_then(|r| r.as_str())
            .unwrap_or("N");
        if retry == "Y" {
            Ok(CancelResult::AlreadyPaid)
        } else {
            Ok(CancelResult::Canceled)
        }
    }

    fn refund(
        &self,
        out_trade_no: &str,
        refund_no: &str,
        amount_cents: i64,
    ) -> Result<PaymentState> {
        let biz = serde_json::json!({
            "out_trade_no": out_trade_no,
            "refund_amount": crate::money::cents_to_yuan(amount_cents),
            "out_request_no": refund_no,
        });
        let resp = self.call("alipay.trade.refund", &biz)?;
        // fund_change=Y 表示资金发生变动（退款成功）
        let fund_change = resp
            .get("fund_change")
            .and_then(|f| f.as_str())
            .unwrap_or("N");
        if fund_change == "Y" {
            Ok(PaymentState::SUCCESS)
        } else {
            Ok(state_of(&resp)?.state)
        }
    }
}

/// 最小百分号编码（RFC 3968 unreserved 之外全部转义）。
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_encode_form() {
        assert_eq!(percent_encode("abc-_.~123"), "abc-_.~123");
        assert_eq!(percent_encode("a b"), "a%20b");
        assert_eq!(percent_encode("收银"), "%E6%94%B6%E9%93%B6");
        assert_eq!(percent_encode("k=v&x"), "k%3Dv%26x");
    }
}
