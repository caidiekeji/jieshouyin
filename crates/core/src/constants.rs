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
pub const PERM_MEMBER_MANAGE: &str = "member.manage";
pub const PERM_MEMBER_RECHARGE: &str = "member.recharge";

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
/// 商品名最大字符数（与前端 PRODUCT_NAME_MAX 对齐）。
pub const PRODUCT_NAME_MAX_CHARS: usize = 40;

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
/// 利润报表月度费用（元，非负整数）：房租 / 水电 / 人工。
pub const CFG_EXPENSE_RENT_YUAN: &str = "expense_rent_yuan";
pub const CFG_EXPENSE_UTILITY_YUAN: &str = "expense_utility_yuan";
pub const CFG_EXPENSE_LABOR_YUAN: &str = "expense_labor_yuan";
/// 会员积分累计总开关（'1' 启用 / '0' 停用）。关闭后结算不累计积分。
pub const CFG_POINTS_ENABLED: &str = "points_enabled";
pub const CFG_MAX_DISCOUNT_RATIO: &str = "max_discount_ratio";
/// 打印服务端地址键（§7.5 网络打印机，v1 可选配置）。
pub const CFG_PRINT_HOST: &str = "print_host";
pub const CFG_PRINT_PORT: &str = "print_port";
/// 结算成功后是否自动打印小票（0 关 / 1 开）。
pub const CFG_PRINT_AUTO: &str = "print_auto";
/// 棋牌房费计价单位：MINUTE（单价按分钟）/ HOUR（单价按小时）。默认 MINUTE。
pub const CFG_CHESS_FEE_UNIT: &str = "chess_fee_unit";
/// 棋牌最低计费分钟数，默认 60；0 表示不设最低。
pub const CFG_CHESS_MIN_MINUTES: &str = "chess_min_minutes";
/// 棋牌超出后取整分钟数，默认 0 表示按实际分钟不取整；10 表示不足 10 分钟按 10 分钟。
pub const CFG_CHESS_ROUND_MINUTES: &str = "chess_round_minutes";
/// 开台会员策略：OPTIONAL（可不绑）/ REQUIRED（必须绑定会员才能开台）。
pub const CFG_TABLE_MEMBER_BIND: &str = "table_member_bind";
/// 占用中的桌台是否允许换绑会员（1 允许 / 0 禁止）。
pub const CFG_TABLE_ALLOW_REBIND: &str = "table_allow_member_rebind";
/// 厨打档口路由：JSON 数组 [{category_id,label,host,port}]；为空时回退主打印机。
pub const CFG_KITCHEN_ROUTES: &str = "kitchen_print_routes";

// ── 支付渠道开关（§6.1 设置页；'1' 启用 / '0' 停用）────────────
pub const CFG_PAY_CHANNEL_CASH: &str = "pay_channel_cash_enabled";
pub const CFG_PAY_CHANNEL_WECHAT: &str = "pay_channel_wechat_enabled";
pub const CFG_PAY_CHANNEL_ALIPAY: &str = "pay_channel_alipay_enabled";
pub const CFG_PAY_CHANNEL_SV: &str = "pay_channel_sv_enabled";

// ── 门店与行业（多行业适配；行业决定分类树与功能可见性）────────
/// 门店名称：顶栏与小票抬头显示。
pub const CFG_STORE_NAME: &str = "store_name";
pub const CFG_STORE_PHONE: &str = "store_phone";
/// 门店编号 / 终端编号：拼进订单号前缀，多门店多机台部署时必须不同，否则会撞单号。
pub const CFG_STORE_NO: &str = "store_no";
pub const CFG_DEVICE_NO: &str = "device_no";
/// 小票二维码链接前缀（真实可访问的订单查询页）；未配置时二维码只承载单号。
pub const CFG_RECEIPT_QR_BASE: &str = "receipt_qr_base";
/// 所属行业标识：SUPERMARKET / CLOTHING / RESTAURANT / BABY / BOARD_GAME / WELLNESS。
pub const CFG_INDUSTRY: &str = "industry";
/// 在线激活服务地址。为空时回退到构建时内置的地址。
pub const CFG_ACTIVATION_ENDPOINT: &str = "activation_endpoint";

/// 行业标识：超市便利店。
pub const INDUSTRY_SUPERMARKET: &str = "SUPERMARKET";
/// 行业标识：服装鞋帽。
pub const INDUSTRY_CLOTHING: &str = "CLOTHING";
/// 行业标识：餐饮门店（桌台开台点单）。
pub const INDUSTRY_RESTAURANT: &str = "RESTAURANT";
/// 行业标识：母婴用品。
pub const INDUSTRY_BABY: &str = "BABY";
/// 行业标识：棋牌包间。
pub const INDUSTRY_BOARD_GAME: &str = "BOARD_GAME";
/// 行业标识：养生保健服务。
pub const INDUSTRY_WELLNESS: &str = "WELLNESS";
/// 全部行业标识（顺序即选择页展示顺序）。
pub const INDUSTRIES: [&str; 6] = [
    INDUSTRY_SUPERMARKET,
    INDUSTRY_CLOTHING,
    INDUSTRY_RESTAURANT,
    INDUSTRY_BABY,
    INDUSTRY_BOARD_GAME,
    INDUSTRY_WELLNESS,
];

// ── 配置项控件类型（前端据此选择控件，不在前端另立映射）─────────
/// 文本输入。
pub const CONFIG_KIND_TEXT: &str = "text";
/// 整数输入（带单位后缀由说明文字承载）。
pub const CONFIG_KIND_NUMBER: &str = "number";
/// 小数输入（折扣率）。
pub const CONFIG_KIND_DECIMAL: &str = "decimal";
/// 开关（'1' 启用 / '0' 停用）。
pub const CONFIG_KIND_SWITCH: &str = "switch";
/// 固定选项下拉（选项由前端按配置键给出）。
pub const CONFIG_KIND_SELECT: &str = "select";

/// 配置项元数据：键、中文名、说明、控件类型。系统配置页全中文显示的唯一来源。
/// 顺序即界面展示顺序（按「门店 → 收银 → 支付 → 打印」分组语义排列）。
pub const CONFIG_META: [(&str, &str, &str, &str); 34] = [
    (
        CFG_STORE_NAME,
        "门店名称",
        "显示在界面顶栏与小票抬头",
        CONFIG_KIND_TEXT,
    ),
    (
        CFG_INDUSTRY,
        "所属行业",
        "超市 / 服装 / 餐饮，决定商品分类树",
        CONFIG_KIND_TEXT,
    ),
    (
        CFG_STORE_TIMEZONE,
        "门店时区",
        "用于业务日切点与报表日期归属",
        CONFIG_KIND_TEXT,
    ),
    (
        CFG_BUSINESS_DAY_CUTOFF,
        "业务日切点",
        "格式 HH:MM，跨过该时刻归入次日",
        CONFIG_KIND_TEXT,
    ),
    (
        CFG_SCAN_GAP_MS,
        "扫码判定间隔",
        "单位毫秒，扫码枪连续按键间隔小于该值判为一次扫码",
        CONFIG_KIND_NUMBER,
    ),
    (
        CFG_MICROPAY_POLL_MS,
        "付款码轮询间隔",
        "单位毫秒，付款码支付时向渠道查单的间隔",
        CONFIG_KIND_NUMBER,
    ),
    (
        CFG_MICROPAY_TIMEOUT_S,
        "付款码支付超时",
        "单位秒，超时判定支付未完成",
        CONFIG_KIND_NUMBER,
    ),
    (
        CFG_NATIVE_POLL_MS,
        "主扫轮询间隔",
        "单位毫秒，顾客扫码后轮询订单状态的间隔",
        CONFIG_KIND_NUMBER,
    ),
    (
        CFG_NATIVE_TIMEOUT_S,
        "主扫支付超时",
        "单位秒，超时后允许作废订单",
        CONFIG_KIND_NUMBER,
    ),
    (
        CFG_POINTS_ENABLED,
        "启用会员积分",
        "开启后会员付款自动累计积分；关闭则不累计",
        CONFIG_KIND_SWITCH,
    ),
    (
        CFG_POINTS_PER_YUAN,
        "每元赠送积分",
        "消费满一元赠送的积分数量",
        CONFIG_KIND_NUMBER,
    ),
    (
        CFG_EXPENSE_RENT_YUAN,
        "月房租（元）",
        "利润报表按天均摊的月度房租费用",
        CONFIG_KIND_NUMBER,
    ),
    (
        CFG_EXPENSE_UTILITY_YUAN,
        "月水电（元）",
        "利润报表按天均摊的月度水电费用",
        CONFIG_KIND_NUMBER,
    ),
    (
        CFG_EXPENSE_LABOR_YUAN,
        "月人工（元）",
        "利润报表按天均摊的月度人工费用",
        CONFIG_KIND_NUMBER,
    ),
    (
        CFG_MAX_DISCOUNT_RATIO,
        "最大折扣率",
        "0–1 之间，限制单笔优惠幅度",
        CONFIG_KIND_DECIMAL,
    ),
    (
        CFG_ALLOW_NEGATIVE_STOCK,
        "允许负库存",
        "开启后售出库存不足的商品不阻断收银",
        CONFIG_KIND_SWITCH,
    ),
    (
        CFG_PAY_CHANNEL_CASH,
        "现金支付",
        "收银台可选支付方式",
        CONFIG_KIND_SWITCH,
    ),
    (
        CFG_PAY_CHANNEL_WECHAT,
        "微信支付",
        "收银台可选支付方式",
        CONFIG_KIND_SWITCH,
    ),
    (
        CFG_PAY_CHANNEL_ALIPAY,
        "支付宝支付",
        "收银台可选支付方式",
        CONFIG_KIND_SWITCH,
    ),
    (
        CFG_PAY_CHANNEL_SV,
        "储值支付",
        "收银台可选支付方式",
        CONFIG_KIND_SWITCH,
    ),
    (
        CFG_PRINT_HOST,
        "打印机地址",
        "网络打印机 IP，留空表示不打印",
        CONFIG_KIND_TEXT,
    ),
    (
        CFG_PRINT_PORT,
        "打印机端口",
        "网络打印机端口，默认 9100",
        CONFIG_KIND_NUMBER,
    ),
    (
        CFG_PRINT_AUTO,
        "结算后自动打印小票",
        "开启后收款成功立即打印小票；关闭则需在小票处手动重打",
        CONFIG_KIND_SWITCH,
    ),
    (
        CFG_ACTIVATION_ENDPOINT,
        "在线激活服务",
        "在线激活服务地址，留空使用内置服务",
        CONFIG_KIND_TEXT,
    ),
    (
        CFG_STORE_PHONE,
        "门店电话",
        "打印在小票抬头的联系电话",
        CONFIG_KIND_TEXT,
    ),
    (
        CFG_STORE_NO,
        "门店编号",
        "拼进订单号前缀；多门店部署时每店必须不同",
        CONFIG_KIND_TEXT,
    ),
    (
        CFG_DEVICE_NO,
        "终端编号",
        "拼进订单号前缀；同店多收银机必须不同",
        CONFIG_KIND_TEXT,
    ),
    (
        CFG_RECEIPT_QR_BASE,
        "小票二维码链接前缀",
        "小票二维码指向的订单查询地址前缀，留空则二维码只承载订单号",
        CONFIG_KIND_TEXT,
    ),
    (
        CFG_CHESS_FEE_UNIT,
        "棋牌房费计价单位",
        "MINUTE=单价按每分钟；HOUR=单价按每小时。可在包间建档时单独覆盖",
        CONFIG_KIND_SELECT,
    ),
    (
        CFG_CHESS_MIN_MINUTES,
        "棋牌最低计费分钟",
        "不足该分钟数按该分钟数收费；0 表示不设最低",
        CONFIG_KIND_NUMBER,
    ),
    (
        CFG_CHESS_ROUND_MINUTES,
        "棋牌超时取整分钟",
        "0=按实际分钟；10=不足 10 分钟按 10 分钟计",
        CONFIG_KIND_NUMBER,
    ),
    (
        CFG_TABLE_MEMBER_BIND,
        "开台会员绑定策略",
        "OPTIONAL=可不绑会员；REQUIRED=必须识别会员才能开台",
        CONFIG_KIND_SELECT,
    ),
    (
        CFG_TABLE_ALLOW_REBIND,
        "允许桌台换绑会员",
        "开启后店长可在占用中的桌台上换绑会员，操作写审计",
        CONFIG_KIND_SWITCH,
    ),
    (
        CFG_KITCHEN_ROUTES,
        "厨打档口路由",
        "JSON 配置，分类绑定独立打印机；留空回退打印设置里的主打印机",
        CONFIG_KIND_TEXT,
    ),
];

/// 可在「系统配置」页直接编辑的配置键白名单。
/// 未列入的键（门店名称、所属行业）由专属命令管理，界面只读展示，避免点了保存却被拒。
pub const CONFIG_EDITABLE: [&str; 31] = [
    CFG_STORE_TIMEZONE,
    CFG_BUSINESS_DAY_CUTOFF,
    CFG_SCAN_GAP_MS,
    CFG_MICROPAY_POLL_MS,
    CFG_MICROPAY_TIMEOUT_S,
    CFG_NATIVE_POLL_MS,
    CFG_NATIVE_TIMEOUT_S,
    CFG_POINTS_ENABLED,
    CFG_POINTS_PER_YUAN,
    CFG_EXPENSE_RENT_YUAN,
    CFG_EXPENSE_UTILITY_YUAN,
    CFG_EXPENSE_LABOR_YUAN,
    CFG_MAX_DISCOUNT_RATIO,
    CFG_ALLOW_NEGATIVE_STOCK,
    CFG_PAY_CHANNEL_CASH,
    CFG_PAY_CHANNEL_WECHAT,
    CFG_PAY_CHANNEL_ALIPAY,
    CFG_PAY_CHANNEL_SV,
    CFG_PRINT_HOST,
    CFG_PRINT_PORT,
    CFG_PRINT_AUTO,
    CFG_ACTIVATION_ENDPOINT,
    CFG_STORE_PHONE,
    CFG_STORE_NO,
    CFG_DEVICE_NO,
    CFG_RECEIPT_QR_BASE,
    CFG_CHESS_FEE_UNIT,
    CFG_CHESS_MIN_MINUTES,
    CFG_CHESS_ROUND_MINUTES,
    CFG_TABLE_MEMBER_BIND,
    CFG_TABLE_ALLOW_REBIND,
];

// ── 配置默认值（与 M0001 种子一致，读取失败时兜底）──────────────
pub const DEFAULT_SCAN_GAP_MS: u64 = 50;
pub const DEFAULT_MICROPAY_POLL_MS: u64 = 2000;
pub const DEFAULT_MICROPAY_TIMEOUT_S: u64 = 60;
pub const DEFAULT_NATIVE_POLL_MS: u64 = 3000;
pub const DEFAULT_NATIVE_TIMEOUT_S: u64 = 300;
pub const DEFAULT_POINTS_PER_YUAN: i64 = 1;
pub const DEFAULT_POINTS_REDEEM_RATE: i64 = 100;
/// 会员积分累计默认开启（老库无该键时按开启处理，避免升级后突然停止累计）。
pub const DEFAULT_POINTS_ENABLED: i64 = 1;
/// 利润报表月度费用默认 0（未配置则不摊费用，利润=毛利）。
pub const DEFAULT_EXPENSE_YUAN: i64 = 0;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// 配置清单是手写数组，最容易出现“新增键忘加编辑白名单/重复项”；
    /// 用测试把两个清单的包含关系与唯一性钉死。
    #[test]
    fn config_meta_and_editable_lists_stay_consistent() {
        let meta_keys: Vec<&str> = CONFIG_META.iter().map(|m| m.0).collect();
        let unique: HashSet<&str> = meta_keys.iter().copied().collect();
        assert_eq!(unique.len(), meta_keys.len(), "CONFIG_META 存在重复键");

        let editable: HashSet<&str> = CONFIG_EDITABLE.iter().copied().collect();
        assert_eq!(
            editable.len(),
            CONFIG_EDITABLE.len(),
            "CONFIG_EDITABLE 存在重复键"
        );
        for key in CONFIG_EDITABLE {
            assert!(
                unique.contains(key),
                "可编辑配置 {key} 未出现在 CONFIG_META"
            );
        }
    }
}
