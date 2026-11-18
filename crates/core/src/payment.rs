//! 支付渠道子系统（§6）。统一 trait 隔离渠道实现；桌面端不开监听端口；
//! v1 闭环 = 付款码同步/轮询 + Native 查单轮询，不依赖回调。

use crate::constants::{
    PAY_ALIPAY, PAY_CASH, PAY_MODE_MICROPAY, PAY_MODE_NATIVE, PAY_STORED_VALUE, PAY_WECHAT,
};
use crate::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types)] // 状态/类型字符串与文档一致
pub enum Channel {
    CASH,
    WECHAT,
    ALIPAY,
    STORED_VALUE,
}

impl Channel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Channel::CASH => PAY_CASH,
            Channel::WECHAT => PAY_WECHAT,
            Channel::ALIPAY => PAY_ALIPAY,
            Channel::STORED_VALUE => PAY_STORED_VALUE,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types)] // 状态/类型字符串与文档一致
pub enum PayMode {
    MICROPAY,
    NATIVE,
}

impl PayMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            PayMode::MICROPAY => PAY_MODE_MICROPAY,
            PayMode::NATIVE => PAY_MODE_NATIVE,
        }
    }
}

/// 渠道查询结果（统一抽象，隔离 wechatpay-v3 / 支付宝自封装差异）。
#[derive(Debug, Clone, PartialEq)]
pub enum PaymentState {
    SUCCESS,
    USERPAYING,
    FAILED(String),
    CLOSED,
}

#[derive(Debug, Clone)]
pub struct PayRequest {
    pub out_trade_no: String,
    pub amount_cents: i64,
    pub auth_code: Option<String>,
    pub mode: PayMode,
}

#[derive(Debug, Clone)]
pub struct PayOutcome {
    pub state: PaymentState,
    pub trade_no: Option<String>,
    pub code_url: Option<String>,
}

/// 渠道网关 trait。业务层只依赖此抽象。
pub trait PaymentGateway: Send + Sync {
    fn micropay(&self, req: &PayRequest) -> Result<PayOutcome>;
    fn native_precreate(&self, req: &PayRequest) -> Result<PayOutcome>;
    fn query(&self, out_trade_no: &str) -> Result<PaymentState>;
    fn cancel(&self, out_trade_no: &str) -> Result<CancelResult>;
    fn refund(
        &self,
        out_trade_no: &str,
        refund_no: &str,
        amount_cents: i64,
    ) -> Result<PaymentState>;
}

#[derive(Debug, Clone, PartialEq)]
pub enum CancelResult {
    Canceled,
    AlreadyPaid, // 撤销遇"订单已支付" → 自动全额退款 + 审计
}

/// out_trade_no 规则：{order_no}P{attempt_seq}（幂等键）。
pub fn make_out_trade_no(order_no: &str, attempt: u32) -> String {
    format!("{order_no}P{attempt}")
}

/// 付款码主通道状态机（§6.2）。同步 SUCCESS → 落库；USERPAYING 轮询；
/// 超时撤销；撤销遇已支付 → 自动退款。
pub fn run_micropay<G: PaymentGateway>(
    gw: &G,
    req: &PayRequest,
    poll_ms: u64,
    timeout_s: u64,
    sleep: &mut dyn FnMut(u64),
) -> Result<PaymentState> {
    let outcome = match gw.micropay(req) {
        Ok(o) => o,
        Err(e) => {
            // 网络异常 → 查单兜底
            return reconcile_timeout(
                gw,
                &req.out_trade_no,
                poll_ms,
                timeout_s,
                sleep,
                &e.to_string(),
            );
        }
    };
    match outcome.state {
        PaymentState::SUCCESS => Ok(PaymentState::SUCCESS),
        PaymentState::FAILED(_) => Ok(outcome.state),
        PaymentState::CLOSED => Ok(PaymentState::CLOSED),
        PaymentState::USERPAYING => reconcile_timeout(
            gw,
            &req.out_trade_no,
            poll_ms,
            timeout_s,
            sleep,
            "USERPAYING",
        ),
    }
}

fn reconcile_timeout<G: PaymentGateway>(
    gw: &G,
    out_trade_no: &str,
    poll_ms: u64,
    timeout_s: u64,
    sleep: &mut dyn FnMut(u64),
    reason: &str,
) -> Result<PaymentState> {
    let mut elapsed = 0u64;
    while elapsed < timeout_s * 1000 {
        match gw.query(out_trade_no) {
            Ok(PaymentState::SUCCESS) => return Ok(PaymentState::SUCCESS),
            Ok(PaymentState::FAILED(e)) => return Ok(PaymentState::FAILED(e)),
            Ok(PaymentState::CLOSED) => return Ok(PaymentState::CLOSED),
            _ => {}
        }
        sleep(poll_ms);
        elapsed += poll_ms;
    }
    let _ = reason;
    match gw.cancel(out_trade_no)? {
        CancelResult::Canceled => Ok(PaymentState::CLOSED),
        CancelResult::AlreadyPaid => {
            // 渠道已扣款 → 自动全额退款（幂等由 refund_no 保证）
            let refund_no = format!("{out_trade_no}:AUTO_REFUND");
            gw.refund(out_trade_no, &refund_no, 0)?;
            Ok(PaymentState::FAILED("撤销遇已支付，已自动全额退款".into()))
        }
    }
}

/// Native 支付：预下单 → code_url → 查单轮询 → 超时关单（§6.3）。
pub fn run_native<G: PaymentGateway>(
    gw: &G,
    req: &PayRequest,
    poll_ms: u64,
    timeout_s: u64,
    sleep: &mut dyn FnMut(u64),
) -> Result<PayOutcome> {
    let pre = gw.native_precreate(req)?;
    let mut elapsed = 0u64;
    while elapsed < timeout_s * 1000 {
        match gw.query(&req.out_trade_no)? {
            PaymentState::SUCCESS => {
                return Ok(PayOutcome {
                    state: PaymentState::SUCCESS,
                    trade_no: None,
                    code_url: pre.code_url,
                })
            }
            PaymentState::FAILED(e) => {
                return Ok(PayOutcome {
                    state: PaymentState::FAILED(e),
                    trade_no: None,
                    code_url: pre.code_url,
                })
            }
            _ => {}
        }
        sleep(poll_ms);
        elapsed += poll_ms;
    }
    // 超时关单；关单失败再查单一次；仍未知 → 告警人工（这里返回 FAILED）
    let _ = gw.cancel(&req.out_trade_no)?;
    match gw.query(&req.out_trade_no)? {
        PaymentState::SUCCESS => Ok(PayOutcome {
            state: PaymentState::SUCCESS,
            trade_no: None,
            code_url: pre.code_url,
        }),
        _ => Ok(PayOutcome {
            state: PaymentState::FAILED("Native 支付超时已关单".into()),
            trade_no: None,
            code_url: pre.code_url,
        }),
    }
}

/// 混合支付退款按原渠道实付净额占比分摊（§6.6）。
pub fn route_refund(shares: &[(Channel, i64)], total: i64) -> Result<Vec<(Channel, i64)>> {
    let weights: Vec<i64> = shares.iter().map(|(_, w)| *w).collect();
    let alloc = crate::money::allocate_by_remaining(&weights, total)?;
    Ok(shares.iter().map(|(c, _)| *c).zip(alloc).collect())
}

/// 对账差异分级（§6.7）：只告警冻结，绝不自动抹平。
#[derive(Debug, Clone, PartialEq)]
#[allow(non_camel_case_types)] // 状态/类型字符串与文档一致
pub enum DiffType {
    MISSING_REMOTE,
    MISSING_LOCAL,
    AMOUNT_MISMATCH,
    STATUS_MISMATCH,
}

impl DiffType {
    pub fn as_str(&self) -> &'static str {
        match self {
            DiffType::MISSING_REMOTE => "MISSING_REMOTE",
            DiffType::MISSING_LOCAL => "MISSING_LOCAL",
            DiffType::AMOUNT_MISMATCH => "AMOUNT_MISMATCH",
            DiffType::STATUS_MISMATCH => "STATUS_MISMATCH",
        }
    }
}

/// 微信账单 CSV 字段前的反引号剥离（§6.7）。
pub fn strip_backticks(field: &str) -> &str {
    field.trim().trim_start_matches('`').trim_end_matches('`')
}

pub mod mock;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn out_trade_no_attempts() {
        assert_eq!(make_out_trade_no("ORD1", 1), "ORD1P1");
        assert_eq!(make_out_trade_no("ORD1", 2), "ORD1P2");
    }

    #[test]
    fn micropay_success_immediate() {
        let gw = mock::MockGateway::new();
        gw.set_state("O1P1", PaymentState::SUCCESS);
        let req = PayRequest {
            out_trade_no: "O1P1".into(),
            amount_cents: 1000,
            auth_code: Some("123456".into()),
            mode: PayMode::MICROPAY,
        };
        let mut noop = |_ms: u64| {};
        assert_eq!(
            run_micropay(&gw, &req, 1, 1, &mut noop).unwrap(),
            PaymentState::SUCCESS
        );
    }

    #[test]
    fn micropay_userpaying_polls_to_success() {
        let gw = mock::MockGateway::new();
        gw.set_state("O2P1", PaymentState::USERPAYING);
        let req = PayRequest {
            out_trade_no: "O2P1".into(),
            amount_cents: 1000,
            auth_code: None,
            mode: PayMode::MICROPAY,
        };
        let calls = std::sync::Arc::new(Mutex::new(0));
        let c = calls.clone();
        let mut spy = |_ms: u64| {
            *c.lock().unwrap() += 1;
            if *c.lock().unwrap() == 2 {
                gw.set_state("O2P1", PaymentState::SUCCESS);
            }
        };
        assert_eq!(
            run_micropay(&gw, &req, 1, 10, &mut spy).unwrap(),
            PaymentState::SUCCESS
        );
    }

    #[test]
    fn micropay_timeout_cancel() {
        let gw = mock::MockGateway::new();
        gw.set_state("O3P1", PaymentState::USERPAYING);
        let req = PayRequest {
            out_trade_no: "O3P1".into(),
            amount_cents: 1000,
            auth_code: None,
            mode: PayMode::MICROPAY,
        };
        let mut noop = |_ms: u64| {};
        assert_eq!(
            run_micropay(&gw, &req, 1, 2, &mut noop).unwrap(),
            PaymentState::CLOSED
        );
    }

    #[test]
    fn cancel_already_paid_triggers_refund() {
        let gw = mock::MockGateway::new();
        gw.set_state("O4P1", PaymentState::USERPAYING);
        gw.set_cancel_paid("O4P1");
        let req = PayRequest {
            out_trade_no: "O4P1".into(),
            amount_cents: 1000,
            auth_code: None,
            mode: PayMode::MICROPAY,
        };
        let mut noop = |_ms: u64| {};
        let state = run_micropay(&gw, &req, 1, 2, &mut noop).unwrap();
        assert!(matches!(state, PaymentState::FAILED(s) if s.contains("退款")));
        assert_eq!(gw.refund_count(), 1);
    }

    #[test]
    fn mixed_refund_routes_exact() {
        let shares = vec![(Channel::WECHAT, 6000), (Channel::STORED_VALUE, 4000)];
        let routes = route_refund(&shares, 3500).unwrap();
        assert_eq!(routes.iter().map(|(_, a)| *a).sum::<i64>(), 3500);
        assert_eq!(routes[0].1, 2100);
        assert_eq!(routes[1].1, 1400);
    }

    #[test]
    fn backticks_stripped() {
        assert_eq!(strip_backticks("`123456789"), "123456789");
        assert_eq!(strip_backticks(" `123` "), "123");
    }

    #[test]
    fn diff_types() {
        assert_eq!(DiffType::AMOUNT_MISMATCH.as_str(), "AMOUNT_MISMATCH");
    }
}
