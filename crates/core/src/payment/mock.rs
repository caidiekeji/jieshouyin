//! 支付 Mock 网关（§23.2）。仅测试使用，不进 release 业务路径。
//! 支持剧本：SUCCESS / USERPAYING→成功 / 超时 / 明确失败 / 撤销成功 / 撤销遇已支付。

use super::{CancelResult, PayOutcome, PayRequest, PaymentGateway, PaymentState};
use crate::error::Result;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Inner {
    states: HashMap<String, PaymentState>,
    cancel_paid: std::collections::HashSet<String>,
    refunds: Vec<(String, String, i64)>,
}

#[derive(Clone, Default)]
pub struct MockGateway {
    inner: Arc<Mutex<Inner>>,
}

impl MockGateway {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_state(&self, out_trade_no: &str, state: PaymentState) {
        self.inner
            .lock()
            .unwrap()
            .states
            .insert(out_trade_no.into(), state);
    }

    /// 设置撤销时返回"订单已支付"的剧本。
    pub fn set_cancel_paid(&self, out_trade_no: &str) {
        self.inner
            .lock()
            .unwrap()
            .cancel_paid
            .insert(out_trade_no.into());
        self.set_state(out_trade_no, PaymentState::SUCCESS);
    }

    pub fn refund_count(&self) -> usize {
        self.inner.lock().unwrap().refunds.len()
    }

    pub fn refunds(&self) -> Vec<(String, String, i64)> {
        self.inner.lock().unwrap().refunds.clone()
    }
}

impl PaymentGateway for MockGateway {
    fn micropay(&self, req: &PayRequest) -> Result<PayOutcome> {
        let inner = self.inner.lock().unwrap();
        let state = inner
            .states
            .get(&req.out_trade_no)
            .cloned()
            .unwrap_or(PaymentState::FAILED("未知订单".into()));
        Ok(PayOutcome {
            state,
            trade_no: Some(format!("TRD_{}", req.out_trade_no)),
            code_url: None,
        })
    }

    fn native_precreate(&self, req: &PayRequest) -> Result<PayOutcome> {
        let mut inner = self.inner.lock().unwrap();
        inner
            .states
            .entry(req.out_trade_no.clone())
            .or_insert(PaymentState::USERPAYING);
        Ok(PayOutcome {
            state: PaymentState::USERPAYING,
            trade_no: None,
            code_url: Some(format!("weixin://wxpay/bizpayurl?pr={}", req.out_trade_no)),
        })
    }

    fn query(&self, out_trade_no: &str) -> Result<PaymentState> {
        let inner = self.inner.lock().unwrap();
        Ok(inner
            .states
            .get(out_trade_no)
            .cloned()
            .unwrap_or(PaymentState::CLOSED))
    }

    fn cancel(&self, out_trade_no: &str) -> Result<CancelResult> {
        let mut inner = self.inner.lock().unwrap();
        if inner.cancel_paid.contains(out_trade_no) {
            return Ok(CancelResult::AlreadyPaid);
        }
        inner
            .states
            .insert(out_trade_no.into(), PaymentState::CLOSED);
        Ok(CancelResult::Canceled)
    }

    /// 幂等：同一 refund_no 重复调用不重复记账。
    fn refund(
        &self,
        out_trade_no: &str,
        refund_no: &str,
        amount_cents: i64,
    ) -> Result<PaymentState> {
        let mut inner = self.inner.lock().unwrap();
        if inner.refunds.iter().any(|(_, n, _)| n == refund_no) {
            return Ok(PaymentState::SUCCESS);
        }
        inner
            .refunds
            .push((out_trade_no.into(), refund_no.into(), amount_cents));
        inner
            .states
            .insert(out_trade_no.into(), PaymentState::FAILED("已退款".into()));
        Ok(PaymentState::SUCCESS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refund_is_idempotent() {
        let gw = MockGateway::new();
        gw.set_cancel_paid("O1");
        assert_eq!(gw.refund("O1", "R1", 100).unwrap(), PaymentState::SUCCESS);
        assert_eq!(gw.refund("O1", "R1", 100).unwrap(), PaymentState::SUCCESS);
        assert_eq!(gw.refund_count(), 1);
    }
}
