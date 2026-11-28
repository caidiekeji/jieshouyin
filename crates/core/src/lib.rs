//! 贝来 核心领域库。
//! 架构硬约束：前台开单/支付/打印/挂单/退货/交班在任何授权状态下不被阻断。

pub mod bill;
pub mod catalog_ext;
pub mod constants;
pub mod db;
pub mod error;
pub mod escpos;
pub mod gateway_alipay;
pub mod gateway_wechat;
pub mod industry;
pub mod inventory;
pub mod license;
pub mod loss;
pub mod member;
pub mod money;
pub mod order;
pub mod orderview;
pub mod paycreds;
pub mod payment;
pub mod printer;
pub mod product;
pub mod promo;
pub mod purchase;
pub mod rbac;
pub mod report;
pub mod report_catalog;
pub mod scanner;
pub mod service;
pub mod technician;
pub mod time;

pub use error::{CoreError, Result};

/// 内嵌验证公钥。build.rs 已把正式公钥固化为源码常量，构建零外部配置即可通过；
/// 仅密钥轮换时可用 CHECKOUT_PUBLIC_KEY 环境变量覆盖（release 强制校验指纹）。
/// 任何构建都拒绝 RFC 8032 测试向量公钥，私钥绝不进入客户端。
/// EMBEDDED_KEYS 是按 key_id 编号的公钥环，轮换期同时含新旧公钥。
pub mod embedded_key {
    include!(concat!(env!("OUT_DIR"), "/public_key.rs"));
}
pub use embedded_key::EMBEDDED_ACTIVATION_ENDPOINT;
pub use embedded_key::EMBEDDED_KEYS;
pub use embedded_key::EMBEDDED_KEY_IS_DEV;
pub use embedded_key::EMBEDDED_PUBLIC_KEY;

/// 内置的在线激活服务地址（构建时注入）。为空表示未随包分发地址，
/// 用户可在「系统配置 → 在线激活服务」中填写；两者取其一即可生效。
pub fn default_activation_endpoint() -> &'static str {
    EMBEDDED_ACTIVATION_ENDPOINT
}
