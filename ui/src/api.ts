// Tauri invoke 封装。所有命令名与后端 invoke_handler 注册清单一一对应。
import { invoke } from '@tauri-apps/api/core'

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
  is_weighed: boolean
}

export interface CartLine {
  sku_id: number
  qty: number
  unit_price_cents: number
  name?: string
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
  is_weighed: boolean
  status: number
  qty: number
  alert_qty: number
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
  lookup: (keyword: string) => invoke<ProductHit[]>('lookup_product', { keyword }),
  parseWeigh: (code: string) =>
    invoke<{ instore_code: string; mode: string; value: number }>('parse_weigh_barcode', { code }),
  checkout: (lines: CartLine[], payments: unknown[]) =>
    invoke<{ order_no: string; status: string }>('checkout', { lines, payments }),
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
    categoryId: number
    unit: string
    salePriceCents: number
    costPriceCents: number
    isWeighed: boolean
    initialQty: number
    alertQty: number
  }) => invoke<number>('create_product', p),
  listProducts: (keyword: string, limit: number) =>
    invoke<ProductRow[]>('list_products', { keyword, limit }),
  updateProduct: (skuId: number, salePriceCents: number, alertQty: number) =>
    invoke<void>('update_product', { skuId, salePriceCents, alertQty }),
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
  shiftReport: (shiftId: number) => invoke<unknown[]>('shift_report', { shiftId }),
}

export function formatMoney(cents: number): string {
  const sign = cents < 0 ? '-' : ''
  const abs = Math.abs(cents)
  return `${sign}${Math.floor(abs / 100)}.${String(abs % 100).padStart(2, '0')}`
}

export function today(): string {
  const d = new Date()
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
}
