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

/** 行业界面口径：页面入口、台位语义、看板文案与默认编号前缀的唯一来源 */
export const INDUSTRY_UI: Record<string, {
  scene: 'DINING' | 'CHESS' | 'WELLNESS' | null
  navTables: string
  manageTitle: string
  itemTitle: string
  capacityLabel: string
  capacityUnit: string
  capacityDefault: number
  boardTitle: string
  boardHint: string
  defaultPrefix: string
}> = {
  SUPERMARKET: {
    scene: null, navTables: '', manageTitle: '', itemTitle: '',
    capacityLabel: '', capacityUnit: '', capacityDefault: 1,
    boardTitle: '超市选购', boardHint: '扫码或点选商品，称重品按重量计价', defaultPrefix: '',
  },
  CLOTHING: {
    scene: null, navTables: '', manageTitle: '', itemTitle: '',
    capacityLabel: '', capacityUnit: '', capacityDefault: 1,
    boardTitle: '服装选购', boardHint: '按分类点选或扫码/款号检索；维护款色码后可点规格标签筛选对应 SKU', defaultPrefix: '',
  },
  RESTAURANT: {
    scene: 'DINING', navTables: '桌台管理', manageTitle: '餐饮桌台管理', itemTitle: '餐饮桌台',
    capacityLabel: '座位', capacityUnit: '座', capacityDefault: 4,
    boardTitle: '餐饮桌台', boardHint: '点击空闲桌台进入点菜，加入菜品后自动开台；结算成功自动清台', defaultPrefix: 'A',
  },
  BABY: {
    scene: null, navTables: '', manageTitle: '', itemTitle: '',
    capacityLabel: '', capacityUnit: '', capacityDefault: 1,
    boardTitle: '母婴选购', boardHint: '奶粉、纸尿裤、辅食按品类点选；可按月龄段筛选并查看批次效期', defaultPrefix: '',
  },
  BOARD_GAME: {
    scene: 'CHESS', navTables: '包间管理', manageTitle: '棋牌包间管理', itemTitle: '棋牌包间',
    capacityLabel: '人数', capacityUnit: '人', capacityDefault: 4,
    boardTitle: '棋牌包间', boardHint: '按分钟计时，最低 60 分钟；开台不再预收整小时房费', defaultPrefix: 'B',
  },
  WELLNESS: {
    scene: 'WELLNESS', navTables: '服务间管理', manageTitle: '养生服务间管理', itemTitle: '养生服务间',
    capacityLabel: '床位', capacityUnit: '床', capacityDefault: 1,
    boardTitle: '养生服务间', boardHint: '选择床位进入服务间，点选项目并指定技师；房费按服务间时长自动生成', defaultPrefix: 'W',
  },
}

/** 促销默认优先级：新建规则排在已有规则之后，不擅自改变现有顺序 */
export const PROMO_DEFAULT_PRIORITY = 100

/** 折扣率转中文「折」的倍率：0.85 → 8.5 折 */
export const DISCOUNT_ZHE_FACTOR = 10

/** 扫码枪楔入判定：连续按键间隔小于此值即判为扫码（§15.1，与后端 scan_gap_ms 默认一致） */
export const SCAN_GAP_MS_DEFAULT = 50
/** 扫码输入最小长度：短于此值视为人工输入，避免单字符误判为扫码 */
export const SCAN_MIN_LENGTH = 4
/** 商品搜索联想防抖（ms）：输入停止 250ms 再请求，兼顾扫码枪连续楔入与手输体感 */
export const SEARCH_DEBOUNCE_MS = 250
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

/** 订单状态中文名：后端状态串为唯一来源，界面只做显示映射 */
export const ORDER_STATUS_LABEL: Record<string, string> = {
  PENDING_PAYMENT: '待支付',
  COMPLETED: '已完成',
  PARTIAL_REFUND: '部分退款',
  REFUNDED: '已退款',
  CANCELED: '已取消',
  VOIDED: '已作废',
}

/** 支付渠道中文名 */
export const PAY_CHANNEL_LABEL: Record<string, string> = {
  CASH: '现金',
  WECHAT: '微信',
  ALIPAY: '支付宝',
  STORED_VALUE: '储值',
}

/** 支付流水状态中文名 */
export const PAY_STATUS_LABEL: Record<string, string> = {
  PENDING: '待确认',
  SUCCESS: '成功',
  CLOSED: '已关闭',
  FAILED: '失败',
  PROCESSING: '处理中',
}

/** 订单列表默认拉取条数：一天一个门店的订单量级 */
export const ORDER_LIST_LIMIT = 100
