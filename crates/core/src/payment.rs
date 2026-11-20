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
    /// 渠道路由依据：真实网关按渠道分流（微信/支付宝），现金/储值不经过网关。
    pub channel: Channel,
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

/// 顾客付款码识别（§6.2 付款码主通道）。
/// 微信付款码 18–19 位纯数字，支付宝付款码 26–30 位纯数字。
/// 判定规则只此一处（前端经命令复用），避免前后端各写一套导致口径不一致。
/// 非付款码返回 None，调用方按商品条码继续处理。
pub fn detect_payment_code(code: &str) -> Option<Channel> {
    let t = code.trim();
    if t.is_empty() || !t.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    match t.len() {
        18 | 19 => Some(Channel::WECHAT),
        26..=30 => Some(Channel::ALIPAY),
        _ => None,
    }
}

/// out_trade_no 规则：{order_no}P{attempt_seq}（幂等键）。
pub fn make_out_trade_no(order_no: &str, attempt: u32) -> String {
    format!("{order_no}P{attempt}")
}

/// 付款码主通道状态机（§6.2）。同步 SUCCESS → 落库；USERPAYING 轮询；
/// 超时撤销；撤销遇已支付 → 自动退款。返回完整结果（含渠道交易号）。
pub fn run_micropay<G: PaymentGateway>(
    gw: &G,
    req: &PayRequest,
    poll_ms: u64,
    timeout_s: u64,
    sleep: &mut dyn FnMut(u64),
) -> Result<PayOutcome> {
    let trade_of = |state: PaymentState| PayOutcome {
        state,
        trade_no: None,
        code_url: None,
    };
    let outcome = match gw.micropay(req) {
        Ok(o) => o,
        Err(e) => {
            // 网络异常 → 查单兜底
            let state = reconcile_timeout(
                gw,
                &req.out_trade_no,
                poll_ms,
                timeout_s,
                sleep,
                &e.to_string(),
            )?;
            return Ok(trade_of(state));
        }
    };
    match outcome.state {
        PaymentState::USERPAYING => {
            let state = reconcile_timeout(
                gw,
                &req.out_trade_no,
                poll_ms,
                timeout_s,
                sleep,
                "USERPAYING",
            )?;
            Ok(PayOutcome {
                trade_no: if state == PaymentState::SUCCESS {
                    outcome.trade_no
                } else {
                    None
                },
                code_url: None,
                state,
            })
        }
        other => Ok(PayOutcome {
            trade_no: outcome.trade_no,
            code_url: None,
            state: other,
        }),
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

/// v1 离线本地确认网关（§6.1）。桌面单机无商户密钥与网络，扫码/Native
/// 由操作员在本地确认驱动状态机，全部落库为可审计的本地流水；
/// 生产环境由真实渠道适配器（wechatpay-v3 / 支付宝）替换本实现。
/// 不伪装成真实网络支付：trade_no 前缀 `LOCAL-`，code_url 为本地伪 URL。
#[derive(Clone, Default)]
pub struct LocalConfirmGateway {
    states: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, PaymentState>>>,
}

impl LocalConfirmGateway {
    pub fn new() -> Self {
        Self::default()
    }
}

impl PaymentGateway for LocalConfirmGateway {
    fn micropay(&self, req: &PayRequest) -> Result<PayOutcome> {
        // 无付款码不构成一次真实扣款：直接失败，杜绝“没扫到码也记成功”。
        if req
            .auth_code
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
        {
            return Ok(PayOutcome {
                state: PaymentState::FAILED("未获取到顾客付款码".into()),
                trade_no: None,
                code_url: None,
            });
        }
        // 付款码在本地确认即成功；记录本地 trade_no，绝不声称真实扣款。
        let mut m = self
            .states
            .lock()
            .map_err(|e| crate::error::CoreError::Other(e.to_string()))?;
        m.insert(req.out_trade_no.clone(), PaymentState::SUCCESS);
        Ok(PayOutcome {
            state: PaymentState::SUCCESS,
            trade_no: Some(format!("LOCAL-{}", req.out_trade_no)),
            code_url: None,
        })
    }

    fn native_precreate(&self, req: &PayRequest) -> Result<PayOutcome> {
        let mut m = self
            .states
            .lock()
            .map_err(|e| crate::error::CoreError::Other(e.to_string()))?;
        m.entry(req.out_trade_no.clone())
            .or_insert(PaymentState::USERPAYING);
        Ok(PayOutcome {
            state: PaymentState::USERPAYING,
            trade_no: None,
            code_url: Some(format!("checkout-r://native/{}", req.out_trade_no)),
        })
    }

    fn query(&self, out_trade_no: &str) -> Result<PaymentState> {
        let m = self
            .states
            .lock()
            .map_err(|e| crate::error::CoreError::Other(e.to_string()))?;
        Ok(m.get(out_trade_no).cloned().unwrap_or(PaymentState::CLOSED))
    }

    fn cancel(&self, out_trade_no: &str) -> Result<CancelResult> {
        let mut m = self
            .states
            .lock()
            .map_err(|e| crate::error::CoreError::Other(e.to_string()))?;
        m.insert(out_trade_no.into(), PaymentState::CLOSED);
        Ok(CancelResult::Canceled)
    }

    fn refund(
        &self,
        out_trade_no: &str,
        _refund_no: &str,
        _amount_cents: i64,
    ) -> Result<PaymentState> {
        let mut m = self
            .states
            .lock()
            .map_err(|e| crate::error::CoreError::Other(e.to_string()))?;
        m.insert(out_trade_no.into(), PaymentState::FAILED("已退款".into()));
        Ok(PaymentState::SUCCESS)
    }
}

/// v1 本地确认一笔 Native 支付（操作员确认顾客已扫码后调用）。
pub fn confirm_native(gw: &LocalConfirmGateway, out_trade_no: &str) -> Result<()> {
    let mut m = gw
        .states
        .lock()
        .map_err(|e| crate::error::CoreError::Other(e.to_string()))?;
    m.insert(out_trade_no.into(), PaymentState::SUCCESS);
    Ok(())
}

/// 渠道路由网关：微信/支付宝凭据已配置 → 真实渠道适配器；未配置 → 本地确认兜底。
/// 记录 out_trade_no → 渠道映射，供无渠道参数的 query/cancel/refund 路由；
/// 映射缺失（如崩溃恢复跨进程）时由调用方显式选择单渠道网关。
pub struct RoutingGateway {
    pub wechat: Option<crate::gateway_wechat::WechatGateway>,
    pub alipay: Option<crate::gateway_alipay::AlipayGateway>,
    pub local: LocalConfirmGateway,
    routes: std::sync::Mutex<std::collections::HashMap<String, Channel>>,
}

impl RoutingGateway {
    pub fn new(
        wechat: Option<crate::gateway_wechat::WechatGateway>,
        alipay: Option<crate::gateway_alipay::AlipayGateway>,
    ) -> Self {
        Self {
            wechat,
            alipay,
            local: LocalConfirmGateway::new(),
            routes: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    fn remember(&self, out_trade_no: &str, channel: Channel) {
        if let Ok(mut r) = self.routes.lock() {
            r.insert(out_trade_no.into(), channel);
        }
    }

    fn route_of(&self, out_trade_no: &str) -> Option<Channel> {
        self.routes
            .lock()
            .ok()
            .and_then(|r| r.get(out_trade_no).copied())
    }

    /// 显式按渠道取网关（崩溃恢复跨进程场景：渠道从 payment_record 读取）。
    pub fn of_channel(&self, channel: Channel) -> &dyn PaymentGateway {
        match channel {
            Channel::WECHAT => match &self.wechat {
                Some(g) => g as &dyn PaymentGateway,
                None => &self.local,
            },
            Channel::ALIPAY => match &self.alipay {
                Some(g) => g as &dyn PaymentGateway,
                None => &self.local,
            },
            _ => &self.local,
        }
    }

    fn route(&self, out_trade_no: &str) -> &dyn PaymentGateway {
        match self.route_of(out_trade_no) {
            Some(ch) => self.of_channel(ch),
            None => &self.local,
        }
    }
}

impl PaymentGateway for RoutingGateway {
    fn micropay(&self, req: &PayRequest) -> Result<PayOutcome> {
        self.remember(&req.out_trade_no, req.channel);
        self.of_channel(req.channel).micropay(req)
    }

    fn native_precreate(&self, req: &PayRequest) -> Result<PayOutcome> {
        self.remember(&req.out_trade_no, req.channel);
        self.of_channel(req.channel).native_precreate(req)
    }

    fn query(&self, out_trade_no: &str) -> Result<PaymentState> {
        self.route(out_trade_no).query(out_trade_no)
    }

    fn cancel(&self, out_trade_no: &str) -> Result<CancelResult> {
        self.route(out_trade_no).cancel(out_trade_no)
    }

    fn refund(
        &self,
        out_trade_no: &str,
        refund_no: &str,
        amount_cents: i64,
    ) -> Result<PaymentState> {
        self.route(out_trade_no)
            .refund(out_trade_no, refund_no, amount_cents)
    }
}

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
            channel: Channel::WECHAT,
        };
        let mut noop = |_ms: u64| {};
        assert_eq!(
            run_micropay(&gw, &req, 1, 1, &mut noop).unwrap().state,
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
            channel: Channel::WECHAT,
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
            run_micropay(&gw, &req, 1, 10, &mut spy).unwrap().state,
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
            channel: Channel::WECHAT,
        };
        let mut noop = |_ms: u64| {};
        assert_eq!(
            run_micropay(&gw, &req, 1, 2, &mut noop).unwrap().state,
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
            channel: Channel::WECHAT,
        };
        let mut noop = |_ms: u64| {};
        let state = run_micropay(&gw, &req, 1, 2, &mut noop).unwrap().state;
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

    #[test]
    fn payment_code_detected_by_length() {
        // 微信 18 位、支付宝 28 位
        assert_eq!(
            detect_payment_code("134567890123456789"),
            Some(Channel::WECHAT)
        );
        assert_eq!(
            detect_payment_code("28123456789012345678901234567"),
            Some(Channel::ALIPAY)
        );
        // 商品条码不是付款码
        assert_eq!(detect_payment_code("6901234567892"), None);
        assert_eq!(detect_payment_code("ABC123"), None);
        assert_eq!(detect_payment_code(""), None);
    }

    #[test]
    fn local_gateway_rejects_missing_auth_code() {
        let gw = LocalConfirmGateway::new();
        let req = PayRequest {
            out_trade_no: "O1P1".into(),
            amount_cents: 1000,
            auth_code: None,
            mode: PayMode::MICROPAY,
            channel: Channel::WECHAT,
        };
        // 无付款码必须失败：不能记成已扣款
        assert!(matches!(
            gw.micropay(&req).unwrap().state,
            PaymentState::FAILED(_)
        ));
    }
}
