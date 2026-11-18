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

// ── 系统配置键（§16 / M0001 system_config 种子）────────────────
pub const CFG_STORE_TIMEZONE: &str = "store_timezone";
pub const CFG_BUSINESS_DAY_CUTOFF: &str = "business_day_cutoff";
pub const CFG_SCAN_GAP_MS: &str = "scan_gap_ms";
pub const CFG_MICROPAY_POLL_MS: &str = "micropay_poll_ms";
pub const CFG_MICROPAY_TIMEOUT_S: &str = "micropay_timeout_s";
pub const CFG_NATIVE_POLL_MS: &str = "native_poll_ms";
pub const CFG_NATIVE_TIMEOUT_S: &str = "native_timeout_s";
pub const CFG_ALLOW_NEGATIVE_STOCK: &str = "allow_negative_stock_default";
pub const CFG_POINTS_PER_YUAN: &str = "points_per_yuan";
pub const CFG_POINTS_REDEEM_RATE: &str = "points_redeem_rate";
pub const CFG_MAX_DISCOUNT_RATIO: &str = "max_discount_ratio";
/// 打印服务端地址键（§7.5 网络打印机，v1 可选配置）。
pub const CFG_PRINT_HOST: &str = "print_host";
pub const CFG_PRINT_PORT: &str = "print_port";

// ── 支付渠道开关（§6.1 设置页；'1' 启用 / '0' 停用）────────────
pub const CFG_PAY_CHANNEL_CASH: &str = "pay_channel_cash_enabled";
pub const CFG_PAY_CHANNEL_WECHAT: &str = "pay_channel_wechat_enabled";
pub const CFG_PAY_CHANNEL_ALIPAY: &str = "pay_channel_alipay_enabled";
pub const CFG_PAY_CHANNEL_SV: &str = "pay_channel_sv_enabled";

// ── 配置默认值（与 M0001 种子一致，读取失败时兜底）──────────────
pub const DEFAULT_SCAN_GAP_MS: u64 = 50;
pub const DEFAULT_MICROPAY_POLL_MS: u64 = 2000;
pub const DEFAULT_MICROPAY_TIMEOUT_S: u64 = 60;
pub const DEFAULT_NATIVE_POLL_MS: u64 = 3000;
pub const DEFAULT_NATIVE_TIMEOUT_S: u64 = 300;
pub const DEFAULT_POINTS_PER_YUAN: i64 = 1;
pub const DEFAULT_POINTS_REDEEM_RATE: i64 = 100;
pub const DEFAULT_MAX_DISCOUNT_RATIO: f64 = 0.8;
/// 网络打印机默认端口（§7.5 IP:9100）。
pub const DEFAULT_PRINT_PORT: u16 = 9100;

// ── 券状态（§13.7，与 member.rs 状态机一致）───────────────────
pub const COUPON_UNUSED: &str = "UNUSED";
pub const COUPON_LOCKED: &str = "LOCKED";
pub const COUPON_REDEEMED: &str = "REDEEMED";
pub const COUPON_REFUNDED: &str = "REFUNDED";
pub const COUPON_EXPIRED: &str = "EXPIRED";

// ── 班次状态（§5.4 / §16 shift）───────────────────────────────
pub const SHIFT_OPEN: &str = "OPEN";
pub const SHIFT_CLOSED: &str = "CLOSED";

// ── 审计命令标识（§12.4 operation_audit.command）──────────────
pub const AUDIT_VOID: &str = "void_order";
pub const AUDIT_REFUND: &str = "refund_order";
pub const AUDIT_PRICE_OVERRIDE: &str = "price_override";
pub const AUDIT_STOCKTAKE_APPROVE: &str = "approve_stocktake";
pub const AUDIT_SHIFT_CLOSE: &str = "close_shift";

// ── 二次授权一次性 nonce TTL 秒数（§12.4，与 rbac 60s 一致）────
pub const SENSITIVE_AUTH_TTL_SECONDS: i64 = 60;
