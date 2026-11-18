/**
 * 前端业务常量 · 唯一来源
 *
 * 规则：组件与 API 层不得出现带业务语义的裸数字。
 * 推导：每个值均注明「为什么是它」，与 styles/tokens.css 的设计值分工——
 * 这里管业务尺度，tokens 管视觉尺度。
 */

/** 人民币最小单位：1 元 = 100 分（后端金额字段统一为分） */
export const CENTS_PER_YUAN = 100

/** 金额显示小数位：分精度，两位足够表达到 0.01 元 */
export const CURRENCY_DECIMALS = 2

/** 日期段显示宽度：年月日统一两位，保证 ISO 格式对齐 */
export const DATE_PART_DIGITS = 2

/** 一天的毫秒数：试用期剩余天数由 RFC3339 时间差换算 */
export const MS_PER_DAY = 86_400_000

/** 成功提示停留 4 秒：一句短提示的阅读时间，再长会挡住操作 */
export const TOAST_DURATION_MS = 4_000

/** 错误提示停留 6 秒：需要读完原因并决定怎么改 */
export const ERROR_DURATION_MS = 6_000

/** 列表拉取上限：后台全量列表与收银联想搜索分档 */
export const PAGE_SIZE = {
  MEMBERS: 100,
  PRODUCTS: 200,
  LOOKUP: 10,
} as const

/** 促销默认优先级：新建规则排在已有规则之后，不擅自改变现有顺序 */
export const PROMO_DEFAULT_PRIORITY = 100

/** 折扣率转中文「折」的倍率：0.85 → 8.5 折 */
export const DISCOUNT_ZHE_FACTOR = 10

/** 扫码枪楔入判定：连续按键间隔小于此值即判为扫码（§15.1，与后端 scan_gap_ms 默认一致） */
export const SCAN_GAP_MS_DEFAULT = 50
/** 扫码输入最小长度：短于此值视为人工输入，避免单字符误判为扫码 */
export const SCAN_MIN_LENGTH = 4
/** 收银员手动输入与扫码的区分等待窗口（ms）：输入停止后再联想 */
export const SEARCH_DEBOUNCE_MS = 120
/** 组织商品名用于小票/列表的截断长度 */
export const PRODUCT_NAME_MAX = 40

/** Native 支付前端轮询间隔（ms）：§6.3 每 3s 查单 */
export const NATIVE_POLL_INTERVAL_MS = 3000
/** 凭据字段标签（§6.8 设置页，与后端字段一一对应，值绝不回显） */
export const PAY_CRED_FIELDS: Record<
  'WECHAT' | 'ALIPAY',
  { key: string; label: string; multiline?: boolean }[]
> = {
  WECHAT: [
    { key: 'wechat.mchid', label: '商户号' },
    { key: 'wechat.appid', label: 'AppID' },
    { key: 'wechat.serial_no', label: '证书序列号' },
    { key: 'wechat.apiv3_key', label: 'APIv3 密钥' },
    { key: 'wechat.private_key_pem', label: '商户私钥(PEM)', multiline: true },
    { key: 'wechat.platform_key_id', label: '平台公钥ID(可选)' },
    { key: 'wechat.platform_public_key_pem', label: '平台公钥(PEM,可选)', multiline: true },
  ],
  ALIPAY: [
    { key: 'alipay.app_id', label: '应用 AppID' },
    { key: 'alipay.private_key_pem', label: '应用私钥(PEM)', multiline: true },
    { key: 'alipay.public_key_pem', label: '支付宝公钥(PEM)', multiline: true },
  ],
}

/** 图标尺寸刻度：导航 18、常规按钮 16、紧凑按钮 14、小标记 12、LOGO 20/24 */
export const ICON_SIZE = {
  xs: 12,
  sm: 14,
  md: 16,
  lg: 18,
  xl: 20,
  xxl: 24,
} as const
