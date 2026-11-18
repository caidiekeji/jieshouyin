//! 业务常量唯一来源（跨 crate 共享）。
//!
//! 纪律：领域层、命令层、签发工具中任何带业务语义的字符串与数值
//! 都必须在此定义并引用，不得就地写字面量。SQL 迁移脚本中的字面量
//! 属于 schema 数据定义，不在本模块管辖范围。
//!
//! 每个常量均标注其对应文档章节或推导依据，避免「说不出为什么」的刻度。

// ── 角色（§12.1，与 M0002 种子一致）────────────────────────────
pub const ROLE_CASHIER: &str = "CASHIER";
pub const ROLE_MANAGER: &str = "MANAGER";
pub const ROLE_ADMIN: &str = "ADMIN";
pub const ROLES: [&str; 3] = [ROLE_CASHIER, ROLE_MANAGER, ROLE_ADMIN];

// ── 权限键（§12.2，与 M0002 permission 种子一一对应）────────────
pub const PERM_POS_CHECKOUT: &str = "pos.checkout";
pub const PERM_POS_VOID: &str = "pos.void";
pub const PERM_POS_REFUND: &str = "pos.refund";
pub const PERM_POS_DISCOUNT_MANUAL: &str = "pos.discount.manual";
pub const PERM_POS_PRICE_OVERRIDE: &str = "pos.price_override";
pub const PERM_POS_SHIFT_CLOSE: &str = "pos.shift.close";
pub const PERM_PRODUCT_EDIT: &str = "product.edit";
pub const PERM_INVENTORY_ADJUST: &str = "inventory.adjust";
pub const PERM_PURCHASE_CREATE: &str = "purchase.create";
pub const PERM_STOCKTAKE_APPROVE: &str = "stocktake.approve";
pub const PERM_REPORT_VIEW: &str = "report.view";
pub const PERM_REPORT_EXPORT: &str = "report.export";
pub const PERM_SYSTEM_CONFIG: &str = "system.config";
pub const PERM_USER_MANAGE: &str = "user.manage";

// ── 授权 entitlement（§4.3）──────────────────────────────────
/// 前台六项生存性操作，任何非 UNACTIVATED 状态都不得阻断。
pub const ENT_POS_CORE: &str = "pos.core";
/// 后台管理整体 entitlement，降级态受限。
pub const ENT_REPORT_STANDARD: &str = "report.standard";
/// 会员基础能力，随试用期一并授予。
pub const ENT_MEMBER_BASIC: &str = "member.basic";

// ── 授权类型（§4.2）──────────────────────────────────────────
pub const LICENSE_PERPETUAL: &str = "PERPETUAL";
pub const LICENSE_ANNUAL: &str = "ANNUAL";
pub const LICENSE_NONE: &str = "NONE";
/// 试用记录在本地状态表中的类型标识（不进入签发载体）。
pub const LICENSE_TRIAL: &str = "TRIAL";
/// 试用记录在 license_state 表中的主键。
pub const STATE_KEY_TRIAL: &str = "trial";

// ── 授权状态（§4.5，与 LicenseStatus::as_str 一致）─────────────
pub const STATUS_UNACTIVATED: &str = "UNACTIVATED";
pub const STATUS_TRIAL_ACTIVE: &str = "TRIAL_ACTIVE";
pub const STATUS_ACTIVE: &str = "ACTIVE";
pub const STATUS_GRACE_PERIOD: &str = "GRACE_PERIOD";
pub const STATUS_OFFLINE_DEGRADED: &str = "OFFLINE_DEGRADED";

// ── 库存事件类型（§10 / F-13，与 TransType::as_str 一致）───────
pub const TRANS_INITIAL: &str = "INITIAL";
pub const TRANS_PURCHASE_IN: &str = "PURCHASE_IN";
pub const TRANS_SALE: &str = "SALE";
pub const TRANS_RETURN: &str = "RETURN";
pub const TRANS_VOID: &str = "VOID";
pub const TRANS_PURCHASE_RETURN: &str = "PURCHASE_RETURN";
pub const TRANS_ADJUST: &str = "ADJUST";
pub const TRANS_LOSS: &str = "LOSS";
pub const TRANS_TRANSFER_IN: &str = "TRANSFER_IN";
pub const TRANS_TRANSFER_OUT: &str = "TRANSFER_OUT";

// ── 单据与支付状态 ────────────────────────────────────────────
pub const WAREHOUSE_STORE: &str = "STORE";
pub const PAY_CASH: &str = "CASH";
pub const PAY_WECHAT: &str = "WECHAT";
pub const PAY_ALIPAY: &str = "ALIPAY";
pub const PAY_STORED_VALUE: &str = "STORED_VALUE";
pub const PAY_SUCCESS: &str = "SUCCESS";
pub const PAY_PENDING: &str = "PENDING";
pub const PAY_MODE_MICROPAY: &str = "MICROPAY";
pub const PAY_MODE_NATIVE: &str = "NATIVE";

// ── 限额键（§12.2，与 M0002 role_limit 种子一致）──────────────
pub const LIMIT_REFUND_MAX_CENTS: &str = "refund.max_cents";
pub const LIMIT_PRICE_OVERRIDE_MAX_CENTS: &str = "price_override.max_cents";

// ── 业务阈值与默认值 ──────────────────────────────────────────
/// 试用期天数：用户确认的商务政策，自首次启动起全功能可用。
pub const TRIAL_DAYS: i64 = 15;
/// 试用期授予的 entitlement 集合：前台 + 后台 + 会员基础。
pub const TRIAL_ENTITLEMENTS: [&str; 3] = [ENT_POS_CORE, ENT_REPORT_STANDARD, ENT_MEMBER_BASIC];
/// 单调水位容差：系统时钟回拨超过 24 小时才判定为异常，容忍 NTP 微调。
pub const CLOCK_SKEW_TOLERANCE_HOURS: i64 = 24;
/// 未分类类目 ID：M0007 种子行，所有新建商品默认归入此类。
pub const DEFAULT_CATEGORY_ID: i64 = 1;
/// 人民币最小单位换算：所有金额字段以分为单位。
pub const CENTS_PER_YUAN: i64 = 100;
/// 默认小票纸宽（毫米）：M0005 print_template 种子，58mm 是主流便携机型。
pub const PAPER_WIDTH_DEFAULT: u8 = 58;
/// 密码策略下限（§12.3）：8 位且字母数字混合。
pub const PASSWORD_MIN_LEN: usize = 8;
