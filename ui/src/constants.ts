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

/** 图标尺寸刻度：导航 18、常规按钮 16、紧凑按钮 14、小标记 12、LOGO 20/24 */
export const ICON_SIZE = {
  xs: 12,
  sm: 14,
  md: 16,
  lg: 18,
  xl: 20,
  xxl: 24,
} as const
