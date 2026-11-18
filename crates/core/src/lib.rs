//! Checkout-R 核心领域库。
//! 架构硬约束：前台开单/支付/打印/挂单/退货/交班在任何授权状态下不被阻断。

pub mod bill;
pub mod constants;
pub mod db;
pub mod error;
pub mod escpos;
pub mod gateway_alipay;
pub mod gateway_wechat;
pub mod inventory;
pub mod license;
pub mod member;
pub mod money;
pub mod order;
pub mod paycreds;
pub mod payment;
pub mod product;
pub mod promo;
pub mod purchase;
pub mod rbac;
pub mod report;
pub mod scanner;
pub mod time;

pub use error::{CoreError, Result};

/// 内嵌验证公钥。正式构建由 build.rs 从 CHECKOUT_PUBLIC_KEY 环境变量注入；
/// 未提供时使用 RFC 8032 测试向量公钥（仅开发/CI 编译用），私钥绝不进入客户端。
pub mod embedded_key {
    include!(concat!(env!("OUT_DIR"), "/public_key.rs"));
}
pub use embedded_key::EMBEDDED_PUBLIC_KEY;
