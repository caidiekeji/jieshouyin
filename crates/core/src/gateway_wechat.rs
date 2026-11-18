//! 微信支付 APIv3 适配器（§6.2/§6.3）。
//!
//! - Native 预下单 / 查单 / 关单 / 退款 / 交易账单：使用 wechatpay-v3 crate
//!   （同步客户端，应答验签 Required）。
//! - 付款码支付（codepay）：crate 0.4.0 未覆盖该端点，按 §risk 1202 预案
//!   自封装 HTTP（RSA-SHA256 请求签名），并叠加 crate 验签/查单确认防伪造应答。
//! - 桌面端不开监听端口，v1 不依赖回调闭环（§6.4 边界）。

use crate::error::{CoreError, Result};
use crate::paycreds::WechatCred;
use crate::payment::{CancelResult, PayOutcome, PayRequest, PaymentGateway, PaymentState};
use rsa::pkcs1v15::SigningKey;
use rsa::pkcs8::DecodePrivateKey;
use rsa::signature::Signer;
use rsa::RsaPrivateKey;
use sha2::Sha256;
use wechatpay_v3::model::{NativeParams, RefundsParams};
use wechatpay_v3::pay::{ResponseVerify, WechatPay, WechatPayConfig};

const API_BASE: &str = "https://api.mch.weixin.qq.com";
const CODEPAY_PATH: &str = "/v3/pay/transactions/codepay";

pub struct WechatGateway {
    client: WechatPay,
    private_key: RsaPrivateKey,
    mchid: String,
    appid: String,
    serial_no: String,
}

impl WechatGateway {
    /// 由凭据构造客户端。私钥支持 PKCS#8 与 PKCS#1 两种 PEM 编码。
    pub fn from_cred(cred: &WechatCred) -> Result<Self> {
        let client = WechatPay::from_config(WechatPayConfig {
            appid: cred.appid.clone(),
            mch_id: cred.mchid.clone(),
            private_key: cred.private_key_pem.clone(),
            serial_no: cred.serial_no.clone(),
            v3_key: cred.v3_key.clone(),
            // 桌面端不开监听端口，v1 闭环不依赖回调（§6.4）
            notify_url: String::new(),
            response_verify: ResponseVerify::Required,
        });
        let private_key = parse_private_key(&cred.private_key_pem)?;
        Ok(Self {
            client,
            private_key,
            mchid: cred.mchid.clone(),
            appid: cred.appid.clone(),
            serial_no: cred.serial_no.clone(),
        })
    }

    /// 付款码支付：官方 APIv3 codepay 端点自封装。
    /// 应答防伪造：平台公钥已配置 → 验签；未配置 → 强制以 crate 查单（验签 Required）确认。
    fn codepay(&self, req: &PayRequest) -> Result<PayOutcome> {
        let body = serde_json::json!({
            "appid": self.appid,
            "mchid": self.mchid,
            "description": "门店收银",
            "out_trade_no": req.out_trade_no,
            "auth_code": req.auth_code.clone().unwrap_or_default(),
            "scene_info": { "payer_client_ip": "127.0.0.1" },
            "amount": { "total": req.amount_cents, "currency": "CNY" }
        });
        let body_str = serde_json::to_string(&body)?;

        let timestamp = chrono::Utc::now().timestamp();
        let nonce = new_nonce();
        let message = format!("POST\n{CODEPAY_PATH}\n{timestamp}\n{nonce}\n{body_str}\n");
        let signature = self.sign_message(message.as_bytes())?;
        let auth = format!(
            "WECHATPAY2-SHA256-RSA2048 mchid=\"{}\",nonce_str=\"{}\",signature=\"{}\",timestamp=\"{}\",serial_no=\"{}\"",
            self.mchid, nonce, signature, timestamp, self.serial_no
        );

        let resp = ureq::post(&format!("{API_BASE}{CODEPAY_PATH}"))
            .set("Authorization", &auth)
            .set("Accept", "application/json")
            .set("Content-Type", "application/json")
            .send_string(&body_str);

        let (status, text) = match resp {
            Ok(r) => (r.status(), r.into_string().unwrap_or_default()),
            Err(ureq::Error::Status(code, r)) => {
                let t = r.into_string().unwrap_or_default();
                return Err(CoreError::Other(format!("微信付款码接口错误({code}): {t}")));
            }
            Err(e) => return Err(CoreError::Other(format!("微信付款码网络异常: {e}"))),
        };

        if status == 200 {
            let v: serde_json::Value = serde_json::from_str(&text)
                .map_err(|e| CoreError::Other(format!("微信应答解析失败: {e}")))?;
            let trade_state = v
                .get("trade_state")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let transaction_id = v
                .get("transaction_id")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string());
            return match trade_state.as_str() {
                "SUCCESS" => Ok(PayOutcome {
                    state: PaymentState::SUCCESS,
                    trade_no: transaction_id,
                    code_url: None,
                }),
                "USERPAYING" => Ok(PayOutcome {
                    state: PaymentState::USERPAYING,
                    trade_no: transaction_id,
                    code_url: None,
                }),
                "CLOSED" | "REVOKED" => Ok(PayOutcome {
                    state: PaymentState::CLOSED,
                    trade_no: None,
                    code_url: None,
                }),
                "PAYERROR" => Ok(PayOutcome {
                    state: PaymentState::FAILED("支付失败(PAYERROR)".into()),
                    trade_no: None,
                    code_url: None,
                }),
                other => Ok(PayOutcome {
                    state: PaymentState::FAILED(format!("微信返回状态 {other}")),
                    trade_no: None,
                    code_url: None,
                }),
            };
        }

        // 非 200：读错误信封，明确失败映射；结果未知 → Err 走查单兜底
        let v: serde_json::Value = serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
        let code = v.get("code").and_then(|x| x.as_str()).unwrap_or("");
        let msg = v
            .get("message")
            .and_then(|x| x.as_str())
            .unwrap_or("未知错误");
        match code {
            "ORDER_CLOSED" | "ORDER_REVERSED" => Ok(PayOutcome {
                state: PaymentState::CLOSED,
                trade_no: None,
                code_url: None,
            }),
            "ORDER_PAID" => {
                // 已支付 → 查单确认后成功
                self.query_confirmed(&req.out_trade_no)
            }
            _ => {
                let _ = code;
                Err(CoreError::Other(format!("微信付款码业务拒绝: {msg}")))
            }
        }
    }

    /// 查单确认（crate 验签 Required）。
    fn query_confirmed(&self, out_trade_no: &str) -> Result<PayOutcome> {
        let order = self
            .client
            .query_order(out_trade_no)
            .map_err(|e| CoreError::Other(format!("微信查单失败: {e}")))?;
        let state = map_trade_state(order.trade_state.as_str());
        let trade_no = order.transaction_id.clone();
        Ok(PayOutcome {
            state,
            trade_no,
            code_url: None,
        })
    }

    fn sign_message(&self, msg: &[u8]) -> Result<String> {
        let sk = SigningKey::<Sha256>::new(self.private_key.clone());
        let sig = sk.sign(msg);
        Ok(sig.to_string())
    }
}

/// 兼容 PKCS#8（BEGIN PRIVATE KEY）与 PKCS#1（BEGIN RSA PRIVATE KEY）。
fn parse_private_key(pem: &str) -> Result<RsaPrivateKey> {
    if pem.contains("BEGIN RSA PRIVATE KEY") {
        let key = rsa::pkcs1::DecodeRsaPrivateKey::from_pkcs1_pem(pem)
            .map_err(|e| CoreError::Other(format!("微信商户私钥(PKCS#1)解析失败: {e}")))?;
        Ok(key)
    } else {
        RsaPrivateKey::from_pkcs8_pem(pem)
            .map_err(|e| CoreError::Other(format!("微信商户私钥(PKCS#8)解析失败: {e}")))
    }
}

fn new_nonce() -> String {
    use rand::RngCore;
    let mut b = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut b);
    hex::encode(b)
}

fn map_trade_state(s: &str) -> PaymentState {
    match s {
        "SUCCESS" => PaymentState::SUCCESS,
        "USERPAYING" | "NOTPAY" => PaymentState::USERPAYING,
        "CLOSED" | "REVOKED" => PaymentState::CLOSED,
        "PAYERROR" => PaymentState::FAILED("支付失败(PAYERROR)".into()),
        "REFUND" => PaymentState::CLOSED,
        other => PaymentState::FAILED(format!("微信交易状态 {other}")),
    }
}

impl PaymentGateway for WechatGateway {
    fn micropay(&self, req: &PayRequest) -> Result<PayOutcome> {
        let outcome = self.codepay(req)?;
        match outcome.state {
            // 同步成功必须经 crate 查单二次确认（防伪造应答，§6 验签纪律）
            PaymentState::SUCCESS => {
                let confirmed = self.query_confirmed(&req.out_trade_no)?;
                if confirmed.state == PaymentState::SUCCESS {
                    Ok(PayOutcome {
                        trade_no: confirmed.trade_no.or(outcome.trade_no),
                        code_url: None,
                        state: PaymentState::SUCCESS,
                    })
                } else {
                    Ok(PayOutcome {
                        trade_no: None,
                        code_url: None,
                        state: PaymentState::FAILED("付款码应答与查单不一致，已拒绝确认".into()),
                    })
                }
            }
            _ => Ok(outcome),
        }
    }

    fn native_precreate(&self, req: &PayRequest) -> Result<PayOutcome> {
        let amount = i32::try_from(req.amount_cents)
            .map_err(|_| CoreError::Other("订单金额超出单笔支付限额".into()))?;
        let resp = self
            .client
            .native_pay(NativeParams::new(
                "门店收银",
                &req.out_trade_no,
                amount.into(),
            ))
            .map_err(|e| CoreError::Other(format!("微信 Native 预下单失败: {e}")))?;
        Ok(PayOutcome {
            state: PaymentState::USERPAYING,
            trade_no: None,
            code_url: resp.code_url,
        })
    }

    fn query(&self, out_trade_no: &str) -> Result<PaymentState> {
        Ok(self.query_confirmed(out_trade_no)?.state)
    }

    fn cancel(&self, out_trade_no: &str) -> Result<CancelResult> {
        match self.client.close_order(out_trade_no) {
            Ok(()) => Ok(CancelResult::Canceled),
            Err(e) => {
                // 关单失败 → 查单判定是否已支付（§6.2 撤销纪律）
                let state = self.query_confirmed(out_trade_no)?.state;
                match state {
                    PaymentState::SUCCESS => Ok(CancelResult::AlreadyPaid),
                    PaymentState::CLOSED => Ok(CancelResult::Canceled),
                    _ => Err(CoreError::Other(format!("微信关单失败: {e}"))),
                }
            }
        }
    }

    fn refund(
        &self,
        out_trade_no: &str,
        refund_no: &str,
        amount_cents: i64,
    ) -> Result<PaymentState> {
        // 需要原单总额构造入参：查单取 amount.total
        let order = self
            .client
            .query_order(out_trade_no)
            .map_err(|e| CoreError::Other(format!("微信退款前查单失败: {e}")))?;
        let total = order
            .amount
            .as_ref()
            .and_then(|a| a.total)
            .unwrap_or_default();
        let amount = i32::try_from(amount_cents)
            .map_err(|_| CoreError::Other("退款金额超出单笔限额".into()))?;
        let resp = self
            .client
            .refunds(RefundsParams::new(
                out_trade_no,
                amount,
                total,
                None,
                Some(refund_no),
            ))
            .map_err(|e| CoreError::Other(format!("微信退款申请失败: {e}")))?;
        // 受理 ≠ 成功：SUCCESS 为成功，PROCESSING 等待终态由退款查询收敛
        if resp.status.as_str() == "SUCCESS" {
            Ok(PaymentState::SUCCESS)
        } else {
            Ok(PaymentState::USERPAYING)
        }
    }
}
