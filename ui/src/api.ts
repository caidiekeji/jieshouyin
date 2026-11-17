// Tauri invoke 封装。所有命令名与后端 invoke_handler 注册清单一一对应。
import { invoke } from '@tauri-apps/api/core'

export interface LicenseSnapshot {
  status: 'UNACTIVATED' | 'ACTIVE' | 'GRACE_PERIOD' | 'OFFLINE_DEGRADED'
  entitlements: string[]
  valid_until: string | null
  maintenance_until: string | null
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

export const api = {
  bootstrap: () => invoke<LicenseSnapshot>('bootstrap'),
  initAdmin: (username: string, password: string) =>
    invoke<void>('init_admin', { username, password }),
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
  shiftReport: (shiftId: number) => invoke<unknown[]>('shift_report', { shiftId }),
  createProduct: (name: string, barcode: string, priceCents: number) =>
    invoke<number>('create_product', { name, barcode, priceCents }),
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
