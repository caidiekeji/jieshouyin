//! 时间注入（§23.1）与单调时钟水位（§4.6）。
//! 除 Clock 实现本身外，禁止直接调用 SystemTime/Local::now()。

use crate::error::{CoreError, Result};
use chrono::{DateTime, Duration, Utc};
use std::sync::{Arc, Mutex};

/// 可注入时钟。所有时间读取必须经过此 trait。
pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

/// 生产时钟：读取系统 UTC 时间。这是唯一允许直接读系统时间的地方。
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// 测试注入时钟：返回固定时刻，可手动推进。
#[derive(Debug, Clone)]
pub struct FixedClock {
    inner: Arc<Mutex<DateTime<Utc>>>,
}

impl FixedClock {
    pub fn new(t: DateTime<Utc>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(t)),
        }
    }

    pub fn set(&self, t: DateTime<Utc>) {
        let mut guard = self.inner.lock().expect("FixedClock mutex poisoned");
        *guard = t;
    }

    pub fn advance(&self, d: Duration) {
        let mut guard = self.inner.lock().expect("FixedClock mutex poisoned");
        *guard = *guard + d;
    }
}

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        *self.inner.lock().expect("FixedClock mutex poisoned")
    }
}

/// 单调水位：effective_now = max(last_known_time, system_now)。
/// 只增不减，防止系统时间回拨导致的"免费续期"。
#[derive(Debug, Clone)]
pub struct MonotonicClock<C: Clock> {
    source: C,
    watermark: Arc<Mutex<DateTime<Utc>>>,
    rollback_threshold: Duration,
}

impl<C: Clock> MonotonicClock<C> {
    pub fn new(source: C, last_known: DateTime<Utc>) -> Self {
        Self {
            source,
            watermark: Arc::new(Mutex::new(last_known)),
            rollback_threshold: Duration::hours(24),
        }
    }

    /// 计算 effective_now 并持久化新水位。
    /// 返回 (effective_now, 是否检测到回拨)。
    pub fn effective_now(&self) -> (DateTime<Utc>, bool) {
        let system_now = self.source.now();
        let mut guard = self.watermark.lock().expect("watermark mutex poisoned");
        let rollback = system_now < *guard - self.rollback_threshold;
        let effective = if system_now > *guard { system_now } else { *guard };
        *guard = effective;
        (effective, rollback)
    }

    /// 读取当前水位但不推进（供测试断言）。
    pub fn peek(&self) -> DateTime<Utc> {
        *self.watermark.lock().expect("watermark mutex poisoned")
    }
}

/// 解析 RFC3339 字符串，错误映射为明确变体而不是 panic。
pub fn parse_rfc3339(s: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| CoreError::Time(format!("无法解析时间 {s:?}: {e}")))
}

/// 格式化为存储统一的 UTC RFC3339。
pub fn format_utc(t: &DateTime<Utc>) -> String {
    t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> DateTime<Utc> {
        parse_rfc3339(s).unwrap()
    }

    #[test]
    fn monotonic_blocks_rollback() {
        let clock = FixedClock::new(t("2026-11-17T00:00:00Z"));
        let mono = MonotonicClock::new(clock.clone(), t("2026-11-17T00:00:00Z"));
        assert_eq!(mono.effective_now().0, t("2026-11-17T00:00:00Z"));

        clock.set(t("2026-11-20T00:00:00Z"));
        assert_eq!(mono.effective_now().0, t("2026-11-20T00:00:00Z"));

        // 回拨 48 小时：水位不降，且检出回拨
        clock.set(t("2026-11-18T00:00:00Z"));
        let (eff, rollback) = mono.effective_now();
        assert_eq!(eff, t("2026-11-20T00:00:00Z"));
        assert!(rollback);
        assert_eq!(mono.peek(), t("2026-11-20T00:00:00Z"));
    }

    #[test]
    fn monotonic_advances_on_normal_time() {
        let clock = FixedClock::new(t("2026-11-18T12:00:00Z"));
        let mono = MonotonicClock::new(clock, t("2026-11-17T00:00:00Z"));
        let (eff, rollback) = mono.effective_now();
        assert_eq!(eff, t("2026-11-18T12:00:00Z"));
        assert!(!rollback);
    }

    #[test]
    fn small_rollback_within_24h_not_flagged() {
        let clock = FixedClock::new(t("2026-11-17T23:00:00Z"));
        let mono = MonotonicClock::new(clock, t("2026-11-18T00:00:00Z"));
        let (eff, rollback) = mono.effective_now();
        assert_eq!(eff, t("2026-11-18T00:00:00Z"));
        assert!(!rollback);
    }
}
