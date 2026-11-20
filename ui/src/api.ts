// Tauri invoke 封装。所有命令名与后端 invoke_handler 注册清单一一对应。
import { invoke } from '@tauri-apps/api/core'
import { CENTS_PER_YUAN, CURRENCY_DECIMALS, DATE_PART_DIGITS } from './constants'

export interface LicenseSnapshot {
  status:
    | 'UNACTIVATED'
    | 'TRIAL_ACTIVE'
    | 'ACTIVE'
    | 'GRACE_PERIOD'
    | 'OFFLINE_DEGRADED'
  entitlements: string[]
  valid_until: string | null
  maintenance_until: string | null
  trial_ends_at: string | null
}

export interface ProductHit {
  sku_id: number
  spu_name: string
  barcode: string
  price_cents: number
  member_price_cents: number | null
  is_weighed: boolean
  image_path: string | null
  image_url: string | null
}

export interface CartLine {
  sku_id: number
  qty: number
  unit_price_cents: number
  /** 会员价：识别会员后由前端刷新并展示，后端结算仍按会员价取优 */
  member_price_cents?: number | null
  name?: string
  image_path?: string | null
  image_url?: string | null
}

/** 收银台报价：后端口径的应收金额（原价 − 促销 − 券） */
export interface OrderQuote {
  total_cents: number
  discount_cents: number
  payable_cents: number
  /** 券不可用等提示（不阻断报价） */
  coupon_note: string | null
  /** 识别会员后本单预计累计积分 */
  points_earned: number
}

/** 收银结算结果 */
export interface CheckoutResult {
  order_no: string
  status: string
  code_url: string | null
  /** 本单会员实际累计积分 */
  points_earned: number
}

export interface ProductRow {
  sku_id: number
  spu_id: number
  name: string
  barcode: string | null
  category_id: number
  category_name: string
  unit: string
  sale_price_cents: number
  cost_price_cents: number
  member_price_cents: number | null
  instore_code: string | null
  is_weighed: boolean
  status: number
  qty: number
  alert_qty: number
  /** 本地图片绝对路径（需 convertFileSrc 转可展示地址） */
  image_path: string | null
  /** 在线图片地址（http/https） */
  image_url: string | null
}

/** 系统配置项：后端已给出中文名称、说明与控件类型 */
export interface ConfigRow {
  config_key: string
  config_value: string
  label: string
  description: string
  kind: 'text' | 'number' | 'decimal' | 'switch'
  /** 后端标注是否可在本页直接编辑：false 表示由专属命令管理（如门店名称、所属行业） */
  editable: boolean
}

/** 门店档案 */
export interface StoreProfile {
  industry: string
  industry_name: string
  store_name: string
  configured: boolean
}

/** 行业选项 */
export interface IndustryOption {
  id: string
  name: string
  description: string
  default_unit: string
  supports_weighed: boolean
}

/** 订单摘要 */
export interface OrderSummary {
  order_no: string
  business_date: string
  created_at: string
  cashier_id: number
  member_id: number | null
  member_name: string | null
  total_cents: number
  discount_cents: number
  pay_cents: number
  status: string
  item_count: number
  refunded_cents: number
}

/** 订单明细行 */
export interface OrderItemDetail {
  item_id: number
  sku_id: number
  product_name: string
  qty: number
  returned_qty: number
  unit_price_cents: number
  line_total_cents: number
  discount_cents: number
  is_gift: boolean
}

/** 支付流水明细 */
export interface PaymentDetail {
  pay_id: number
  pay_channel: string
  pay_mode: string | null
  amount_cents: number
  cash_tendered_cents: number | null
  change_cents: number | null
  out_trade_no: string | null
  trade_no: string | null
  status: string
  paid_at: string | null
  created_at: string
}

/** 优惠明细 */
export interface PromoDetail {
  promotion_id: number
  discount_cents: number
  detail: string | null
}

/** 退货记录 */
export interface RefundDetail {
  refund_id: number
  return_no: string | null
  refund_no: string
  amount_cents: number
  status: string
  created_at: string
}

/** 订单详情 */
export interface OrderDetail {
  header: OrderSummary
  items: OrderItemDetail[]
  payments: PaymentDetail[]
  promos: PromoDetail[]
  refunds: RefundDetail[]
}

export interface Category {
  category_id: number
  parent_id: number | null
  name: string
}

export interface SupplierRow {
  supplier_id: number
  name: string
  contact: string | null
  phone: string | null
}

export interface PurchaseOrderRow {
  po_no: string
  supplier_name: string
  status: string
  total_cents: number
  created_at: string
  item_count: number
}

export interface PurchaseItemRow {
  item_id: number
  sku_id: number
  product_name: string
  ordered_qty: number
  received_qty: number | null
  unit_cost_cents: number
}

export interface StocktakeRow {
  task_no: string
  status: string
  created_at: string
  item_count: number
}

export interface StocktakeItemRow {
  item_id: number
  sku_id: number
  product_name: string
  book_qty: number
  counted_qty: number | null
  diff_qty: number | null
}

export interface MemberRow {
  member_id: number
  phone: string | null
  member_code: string
  name: string | null
  status: number
  principal_cents: number
  bonus_cents: number
  points: number
  growth: number
}

export interface OrderItemRow {
  item_id: number
  sku_id: number
  product_name: string
  qty: number
  returned_qty: number
  unit_price_cents: number
  line_total_cents: number
  discount_cents: number
}

export interface WeighHit {
  sku_id: number
  product_name: string
  qty: number
  unit_price_cents: number
  line_total_cents: number
  mode: string
}

export interface ChannelTotal { channel: string; amount_cents: number; count: number }
export interface ChannelReport { channels: ChannelTotal[]; balanced: boolean }
export interface ProductSales { sku_id: number; qty: number; sales_cents: number; discount_cents: number }
export interface CategorySales { category_id: number; category_name: string; qty: number; sales_cents: number }
export interface CashierSales { cashier_id: number; order_count: number; pay_cents: number }

/** 统一报表中心：元数据 + 通用结果结构（33 张报表共用） */
export interface ReportMeta { id: string; title: string; group: string }
export interface ReportColumn { key: string; label: string; kind: 'money' | 'qty' | 'text' | 'percent' | 'date' }
export interface ReportMetric { label: string; value: string }
export interface ReportResult {
  id: string
  title: string
  description: string
  columns: ReportColumn[]
  rows: string[][]
  metrics: ReportMetric[]
}
export interface CouponRow {
  coupon_code: string
  member_id: number | null
  discount_cents: number
  min_order_cents: number
  status: string
  valid_until: string | null
}
export interface ShiftCloseResult {
  shift_id: number
  expected_cash_cents: number
  actual_cash_cents: number
  difference_cents: number
}

export interface PromotionRow {
  promotion_id: number
  name: string
  scope: string
  priority: number
  exclusive: boolean
  stackable: boolean
  min_qty: number
  min_amount_cents: number
  discount_rate: number | null
  discount_cents: number | null
  target_skus: number[]
  valid_from: string | null
  valid_until: string | null
  status: number
}

export const api = {
  bootstrap: () => invoke<LicenseSnapshot>('bootstrap'),
  initAdmin: (username: string, password: string) =>
    invoke<void>('init_admin', { username, password }),
  hasUsers: () => invoke<boolean>('has_users'),
  getLicense: () => invoke<LicenseSnapshot>('get_license'),
  activate: (token: string) => invoke<LicenseSnapshot>('activate', { token }),
  renew: (token: string) => invoke<LicenseSnapshot>('renew_license', { token }),
  login: (username: string, password: string) =>
    invoke<{ user_id: number; role_name: string }>('login', { username, password }),
  logout: () => invoke<void>('logout'),
  /** 恢复界面角色：未登录或会话过期返回 null。 */
  currentSession: () => invoke<{ user_id: number; role_name: string } | null>('current_session'),
  lookup: (keyword: string) => invoke<ProductHit[]>('lookup_product', { keyword }),
  /** 批量取 SKU 会员价（识别会员后刷新购物车行） */
  memberPrices: (skuIds: number[]) =>
    invoke<{ sku_id: number; member_price_cents: number | null }[]>('member_prices', { skuIds }),
  parseWeigh: (code: string) =>
    invoke<{ instore_code: string; mode: string; value: number }>('parse_weigh_barcode', { code }),
  quoteOrder: (lines: CartLine[], memberId: number | null, couponCode: string | null) =>
    invoke<OrderQuote>('quote_order', { lines, memberId, couponCode }),
  checkout: (
    lines: CartLine[],
    payments: unknown[],
    memberId: number | null,
    couponCode: string | null
  ) =>
    invoke<CheckoutResult>('checkout', {
      lines,
      payments,
      memberId,
      couponCode,
    }),
  parkCart: (cartJson: string, remark: string | null) =>
    invoke<number>('park_cart', { cartJson, remark }),
  listParked: () => invoke<unknown[]>('list_parked'),
  resumeCart: (parkId: number) => invoke<string>('resume_cart', { parkId }),
  dailyReport: (date: string) =>
    invoke<{ order_count: number; total_sales_cents: number; refund_cents: number; discount_cents: number }>('daily_report', { date }),
  printReceipt: (orderNo: string) => invoke<string>('print_receipt', { orderNo }),
  createProduct: (p: {
    name: string
    barcode: string | null
    instoreCode: string | null
    categoryId: number
    unit: string
    salePriceCents: number
    costPriceCents: number
    memberPriceCents: number | null
    isWeighed: boolean
    initialQty: number
    alertQty: number
  }) => invoke<number>('create_product', p),
  listProducts: (keyword: string, limit: number) =>
    invoke<ProductRow[]>('list_products', { keyword, limit }),
  updateProduct: (
    skuId: number,
    salePriceCents: number,
    alertQty: number,
    memberPriceCents: number | null,
    instoreCode: string | null
  ) =>
    invoke<void>('update_product', { skuId, salePriceCents, alertQty, memberPriceCents, instoreCode }),
  setProductStatus: (spuId: number, status: number) =>
    invoke<void>('set_product_status', { spuId, status }),
  listCategories: () => invoke<Category[]>('list_categories'),
  createCategory: (parentId: number | null, name: string) =>
    invoke<number>('create_category', { parentId, name }),
  adjustStock: (skuId: number, changeQty: number, reason: string) =>
    invoke<number>('adjust_stock', { skuId, changeQty, reason }),
  createSupplier: (name: string, contact: string | null, phone: string | null) =>
    invoke<number>('create_supplier', { name, contact, phone }),
  listSuppliers: () => invoke<SupplierRow[]>('list_suppliers'),
  createPurchaseOrder: (
    supplierId: number,
    items: { sku_id: number; ordered_qty: number; unit_cost_cents: number }[]
  ) => invoke<string>('create_purchase_order', { supplierId, items }),
  listPurchaseOrders: () => invoke<PurchaseOrderRow[]>('list_purchase_orders'),
  getPurchaseItems: (poNo: string) => invoke<PurchaseItemRow[]>('get_purchase_items', { poNo }),
  receivePurchase: (poNo: string, received: { item_id: number; received_qty: number }[]) =>
    invoke<void>('receive_purchase', { poNo, received }),
  listStocktakes: () => invoke<StocktakeRow[]>('list_stocktakes'),
  getStocktakeItems: (taskNo: string) => invoke<StocktakeItemRow[]>('get_stocktake_items', { taskNo }),
  createStocktake: (skuIds: number[]) => invoke<string>('create_stocktake', { skuIds }),
  recordCount: (taskNo: string, counts: { item_id: number; received_qty: number }[]) =>
    invoke<void>('record_count', { taskNo, counts }),
  approveStocktake: (taskNo: string) => invoke<void>('approve_stocktake', { taskNo }),
  createMember: (phone: string | null, name: string | null) =>
    invoke<number>('create_member', { phone, name }),
  listMembers: (keyword: string, limit: number) =>
    invoke<MemberRow[]>('list_members', { keyword, limit }),
  rechargeMember: (memberId: number, principalCents: number, bonusCents: number) =>
    invoke<void>('recharge_member', { memberId, principalCents, bonusCents }),
  listPromotions: () => invoke<PromotionRow[]>('list_promotions'),
  createPromotion: (p: {
    name: string
    scope: string
    priority: number
    exclusive: boolean
    stackable: boolean
    minQty: number
    minAmountCents: number
    discountRate: number | null
    discountCents: number | null
    targetSkus: number[]
    validFrom: string | null
    validUntil: string | null
  }) => invoke<number>('create_promotion', p),
  setPromotionStatus: (promotionId: number, status: number) =>
    invoke<void>('set_promotion_status', { promotionId, status }),
  voidOrder: (orderNo: string, sensitiveNonce: string) =>
    invoke<void>('void_order_cmd', { orderNo, sensitiveNonce }),
  getOrderItems: (orderNo: string) => invoke<OrderItemRow[]>('get_order_items', { orderNo }),
  refundOrder: (
    orderNo: string,
    items: { order_item_id: number; qty: number }[],
    sensitiveNonce: string
  ) => invoke<number>('refund_order', { orderNo, items, sensitiveNonce }),
  requestSensitiveAuth: (permission: string, approverUsername: string, approverPassword: string) =>
    invoke<string>('request_sensitive_auth', { permission, approverUsername, approverPassword }),
  createUser: (username: string, password: string, roleName: string) =>
    invoke<number>('create_user', { username, password, roleName }),
  listUsers: () =>
    invoke<{ user_id: number; username: string; role_name: string; status: number }[]>('list_users'),
  setUserStatus: (userId: number, status: number) =>
    invoke<void>('set_user_status', { userId, status }),
  shiftReport: (shiftId: number) => invoke<unknown[]>('shift_report', { shiftId }),

  // 称重码识别（SCN-02）
  lookupWeigh: (code: string) => invoke<WeighHit>('lookup_weigh', { code }),
  // 顾客付款码识别：返回 'WECHAT' / 'ALIPAY'，非付款码返回 null
  detectPaymentCode: (code: string) => invoke<string | null>('detect_payment_code', { code }),
  // 报表维度（§14）
  channelReport: (date: string) => invoke<ChannelReport>('channel_report', { date }),
  productReport: (from: string, to: string) =>
    invoke<ProductSales[]>('product_report', { from, to }),
  categoryReport: (from: string, to: string) =>
    invoke<CategorySales[]>('category_report', { from, to }),
  cashierReport: (from: string, to: string) =>
    invoke<CashierSales[]>('cashier_report', { from, to }),
  // 统一报表中心（33 张报表）
  reportCatalog: () => invoke<ReportMeta[]>('report_catalog'),
  genericReport: (id: string, from: string, to: string) =>
    invoke<ReportResult>('generic_report', { reportId: id, from, to }),
  reconcileCsv: (date: string) => invoke<string>('reconcile_csv', { date }),
  // 交班闭环（§5.4）
  openShift: (openingFloatCents: number) =>
    invoke<number>('open_shift', { openingFloatCents }),
  closeShift: (
    shiftId: number,
    actualCashCents: number,
    differenceReason: string | null,
    sensitiveNonce: string
  ) =>
    invoke<ShiftCloseResult>('close_shift', {
      shiftId,
      actualCashCents,
      differenceReason,
      sensitiveNonce,
    }),
  // 打印队列（§7.6）
  printRetries: () => invoke<string[]>('print_retries'),
  retryPrint: (orderNo: string) => invoke<string>('retry_print', { orderNo }),
  // 系统配置（§15/§14）
  getConfigs: () => invoke<ConfigRow[]>('get_configs'),
  setConfig: (key: string, value: string) => invoke<void>('set_config', { key, value }),
  // 券管理（§13.7）
  listCoupons: () => invoke<CouponRow[]>('list_coupons'),
  issueCoupon: (
    memberId: number | null,
    discountCents: number,
    minOrderCents: number,
    validUntil: string | null
  ) =>
    invoke<string>('issue_coupon', { memberId, discountCents, minOrderCents, validUntil }),

  // Native 支付收口（§6.3）
  pollNativePay: (orderNo: string) => invoke<CheckoutResult>('poll_native_pay', { orderNo }),
  cancelNativePay: (orderNo: string) => invoke<void>('cancel_native_pay', { orderNo }),
  confirmNativePay: (orderNo: string) => invoke<number>('confirm_native_pay', { orderNo }),
  // 支付凭据与网关状态（§6.8，只报状态不回显明文）
  setPaymentCredential: (channel: string, field: string, value: string) =>
    invoke<void>('set_payment_credential', { channel, field, value }),
  deletePaymentCredential: (channel: string, field: string) =>
    invoke<void>('delete_payment_credential', { channel, field }),
  paymentGatewayStatus: () =>
    invoke<{
      wechat_mode: string
      alipay_mode: string
      wechat_error: string | null
      alipay_error: string | null
    }>('payment_gateway_status'),
  getPayChannels: () =>
    invoke<{ cash: boolean; wechat: boolean; alipay: boolean; stored_value: boolean }>(
      'get_pay_channels'
    ),
  // 渠道账单对账（PAY-05/06）
  importWechatBill: (reconDate: string, csvText: string) =>
    invoke<{ matched: number; missing_remote: number; missing_local: number; amount_mismatch: number; status_mismatch: number }>(
      'import_wechat_bill',
      { reconDate, csvText }
    ),
  importAlipayBill: (reconDate: string, csvText: string) =>
    invoke<{ matched: number; missing_remote: number; missing_local: number; amount_mismatch: number; status_mismatch: number }>(
      'import_alipay_bill',
      { reconDate, csvText }
    ),
  listReconcileDiffs: (reconDate: string) =>
    invoke<
      {
        recon_id: number
        channel: string
        diff_type: string
        order_no: string | null
        local_amount_cents: number
        channel_amount_cents: number
        detail: string | null
        created_at: string
      }[]
    >('list_reconcile_diffs', { reconDate }),
  // 二维码 SVG（Native 支付/离线生成）
  qrSvg: (text: string) => invoke<string>('qr_svg', { text }),

  // ===== 门店档案与行业（多行业适配）=====
  getStoreProfile: () => invoke<StoreProfile>('get_store_profile'),
  listIndustries: () => invoke<IndustryOption[]>('list_industries'),
  selectIndustry: (industry: string) => invoke<StoreProfile>('select_industry', { industry }),
  setStoreName: (name: string) => invoke<StoreProfile>('set_store_name', { name }),

  // ===== 商品图片 =====
  /** 上传本地图片（base64），返回图片绝对路径 */
  uploadProductImage: (skuId: number, dataBase64: string) =>
    invoke<string>('upload_product_image', { skuId, dataBase64 }),
  /** 登记在线图片地址 */
  setProductImageUrl: (skuId: number, url: string) =>
    invoke<void>('set_product_image_url', { skuId, url }),

  // ===== 订单查询（订单详情 / 退款）=====
  listOrders: (date: string | null, keyword: string, limit: number) =>
    invoke<OrderSummary[]>('list_orders', { date, keyword, limit }),
  getOrderDetail: (orderNo: string) => invoke<OrderDetail>('get_order_detail', { orderNo }),
  /** 退款金额试算（与实际退款同口径） */
  previewRefund: (orderNo: string, items: { order_item_id: number; qty: number }[]) =>
    invoke<number>('preview_refund', { orderNo, items }),

  // ===== 在线激活 =====
  activateOnline: (activationKey: string, customerName: string | null) =>
    invoke<LicenseSnapshot>('activate_online', { activationKey, customerName }),
  /** 联网刷新授权：用本地保存的激活码向激活服务读取最新状态 */
  refreshLicenseOnline: () => invoke<LicenseSnapshot>('refresh_license_online'),
  /** 探测激活服务可达性 */
  probeActivationService: () => invoke<boolean>('probe_activation_service'),
  /** 用系统默认浏览器打开官网获取激活码（地址由后端解析，前端不接触） */
  openActivationSite: () => invoke<void>('open_activation_site'),
}

export function formatMoney(cents: number): string {
  const sign = cents < 0 ? '-' : ''
  const abs = Math.abs(cents)
  return `${sign}${Math.floor(abs / CENTS_PER_YUAN)}.${String(abs % CENTS_PER_YUAN).padStart(CURRENCY_DECIMALS, '0')}`
}

export function today(): string {
  const d = new Date()
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(DATE_PART_DIGITS, '0')}-${String(d.getDate()).padStart(DATE_PART_DIGITS, '0')}`
}
