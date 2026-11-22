<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { convertFileSrc } from '@tauri-apps/api/core'
import {
  api,
  formatMoney,
  today,
  type LicenseSnapshot,
  type ProductHit,
  type CartLine,
  type Category,
  type Technician,
  type OrderItemRow,
  type ConfigRow,
  type ReportMeta,
  type ReportResult,
  type OrderQuote,
  type StoreProfile,
  type IndustryOption,
  type ServiceTable,
} from './api'
import RestaurantScene from './components/RestaurantScene.vue'
import KitchenRoutePanel from './components/KitchenRoutePanel.vue'
import ChessScene from './components/ChessScene.vue'
import BabyScene from './components/BabyScene.vue'
import WellnessScene from './components/WellnessScene.vue'
import TechnicianPanel from './components/TechnicianPanel.vue'
import Icon from './components/Icon.vue'
import ProductAdmin from './components/ProductAdmin.vue'
import CategoryAdmin from './components/CategoryAdmin.vue'
import SupplyAdmin from './components/SupplyAdmin.vue'
import MemberAdmin from './components/MemberAdmin.vue'
import OrderDetailPanel from './components/OrderDetailPanel.vue'
import RefundPanel from './components/RefundPanel.vue'
import Switch from './components/Switch.vue'
import SetupSteps from './components/SetupSteps.vue'
import {
  ICON_SIZE,
  MS_PER_DAY,
  TOAST_DURATION_MS,
  ERROR_DURATION_MS,
  SCAN_GAP_MS_DEFAULT,
  SCAN_MIN_LENGTH,
  SEARCH_DEBOUNCE_MS,
  CENTS_PER_YUAN,
  NATIVE_POLL_INTERVAL_MS,
  PAY_CHANNEL_LABEL,
  PAY_CRED_FIELDS,
} from './constants'

type View =
  | 'boot'
  | 'industry'
  | 'activate'
  | 'login'
  | 'pos'
  | 'products'
  | 'categories'
  | 'supply'
  | 'member'
  | 'parked'
  | 'report'
  | 'settings'
  | 'refund'
  | 'order'
const view = ref<View>('boot')
/** 商品管理页内子标签：商品列表 / 分类管理（菜单已合并为一个「商品」入口） */
const goodsTab = ref<'list' | 'category'>('list')
const bootError = ref('')
const license = ref<LicenseSnapshot | null>(null)
const needAdmin = ref(false)
/** 首次初始化流程：设管理员 → 登录 → 选行业 → 激活 */
const onboarding = ref(false)
/** 激活待办标记：建管理员后落盘，完成激活或选择试用后清除 */
const activationPending = ref(false)
const ACTIVATION_PENDING_KEY = 'checkoutr.activation_pending'
function readActivationPending(): boolean {
  try {
    return localStorage.getItem(ACTIVATION_PENDING_KEY) === '1'
  } catch {
    return false
  }
}
function setActivationPending(pending: boolean) {
  activationPending.value = pending
  try {
    if (pending) localStorage.setItem(ACTIVATION_PENDING_KEY, '1')
    else localStorage.removeItem(ACTIVATION_PENDING_KEY)
  } catch {
    // 本地存储不可用时只影响跨重启提示，不阻断当前流程
  }
}
/** 当前是否还需要走激活：未激活，或首启激活待办尚未完成 */
const needsActivation = computed(() => {
  const status = license.value?.status
  if (!status || status === 'ACTIVE') return false
  return status === 'UNACTIVATED' || activationPending.value
})
/** 初始化步骤号：登录页在设置账号时为 1、正式登录为 2 */
const authStep = computed(() => {
  if (view.value === 'login') return needAdmin.value ? 1 : 2
  if (view.value === 'industry') return 3
  if (view.value === 'activate') return 4
  return 0
})
const adminUser = ref('admin')
const adminPass = ref('')
const loginUser = ref('')
const loginPass = ref('')
const loginError = ref('')
const activateToken = ref('')
const activateError = ref('')
const renewMode = ref(false)
/** 激活方式：在线激活为默认主路径；离线导入为网络不可用时的备用入口 */
const activateMode = ref<'online' | 'offline'>('online')
const activateCustomer = ref('')
/** 激活服务可达性：null=未探测，true=可达，false=不可达 */
const activationServiceReachable = ref<boolean | null>(null)
/** 探测激活服务中，避免重复点击 */
const probingService = ref(false)
const theme = ref<'light' | 'dark'>('light')
const currentUser = ref('')

// ===== 锁机码：离岗锁屏 / 隐藏到托盘自动锁屏（纯本地，不参与授权）=====
const LOCK_CODE_KEY = 'checkoutr.lock_code'
const lockCode = ref('')
const lockCodeDraft = ref('')
const lockSetupOpen = ref(false)
const pendingLockAfterSetup = ref(false)
const locked = ref(false)
const lockInput = ref('')
const lockError = ref('')
const LOCK_HASH_RE = /^[0-9a-f]{64}$/

// ===== 角色与菜单可见性 =====
// 与后端 M0002 角色种子一致：CASHIER < MANAGER < ADMIN。
// 这里只控制「显示什么」，真正的权限拦截仍由后端角色权限完成。
type Role = 'CASHIER' | 'MANAGER' | 'ADMIN'
const currentRole = ref<Role>('CASHIER')
const ROLE_RANK: Record<Role, number> = { CASHIER: 1, MANAGER: 2, ADMIN: 3 }
function normalizeRole(roleName: string): Role {
  return roleName === 'ADMIN' ? 'ADMIN' : roleName === 'MANAGER' ? 'MANAGER' : 'CASHIER'
}
/** 当前角色是否达到要求等级：店员仅收银，店长增加经营管理，管理员全量。 */
function atLeast(role: Role): boolean {
  return ROLE_RANK[currentRole.value] >= ROLE_RANK[role]
}

// 设置中心子页：左侧导航 + 右侧内容，避免九张卡片堆一屏
type SettingsTab = 'overview' | 'appearance' | 'staff' | 'system' | 'payment' | 'points' | 'recon' | 'print'
type SettingsIcon = 'store' | 'sun' | 'shield' | 'sliders' | 'wallet' | 'star' | 'report' | 'print'
const settingsTab = ref<SettingsTab>('overview')
const settingsTabs: Array<{ id: SettingsTab; label: string; icon: SettingsIcon; minRole: Role }> = [
  { id: 'overview', label: '门店概览', icon: 'store', minRole: 'MANAGER' },
  { id: 'appearance', label: '外观主题', icon: 'sun', minRole: 'MANAGER' },
  { id: 'staff', label: '员工管理', icon: 'shield', minRole: 'ADMIN' },
  { id: 'system', label: '系统配置', icon: 'sliders', minRole: 'ADMIN' },
  { id: 'payment', label: '支付设置', icon: 'wallet', minRole: 'MANAGER' },
  { id: 'points', label: '积分配置', icon: 'star', minRole: 'MANAGER' },
  { id: 'recon', label: '渠道对账', icon: 'report', minRole: 'MANAGER' },
  { id: 'print', label: '打印队列', icon: 'print', minRole: 'MANAGER' },
]
const visibleSettingsTabs = computed(() => settingsTabs.filter((t) => atLeast(t.minRole)))

/** 受角色保护的视图；店员只能进入 pos。 */
const ROLE_GUARDED_VIEWS: View[] = ['products', 'categories', 'supply', 'member', 'refund', 'order', 'report', 'settings', 'parked']
watch([view, currentRole], () => {
  if (ROLE_GUARDED_VIEWS.includes(view.value) && !atLeast('MANAGER')) {
    view.value = 'pos'
  }
  if (view.value === 'settings') {
    const tab = settingsTabs.find((t) => t.id === settingsTab.value)
    if (tab && !atLeast(tab.minRole)) settingsTab.value = 'overview'
  }
})

// ===== 门店档案与行业（多行业适配）=====
const storeProfile = ref<StoreProfile | null>(null)
const industries = ref<IndustryOption[]>([])
const pendingIndustry = ref('')
const storeNameDraft = ref('')
/** 当前行业场景：决定收银台显示哪套独立操作面板 */
const industry = computed(() => storeProfile.value?.industry ?? '')
/** 当前选中的桌台/包间；结算成功后清台 */
const activeTable = ref<ServiceTable | null>(null)
let tableSaveTimer: ReturnType<typeof setTimeout> | null = null
/** 棋牌计时刷新：每 30 秒按后端计算结果更新房费行，最低 60 分钟。 */
let chessFeeTimer: ReturnType<typeof setInterval> | null = null
/** 订单详情页当前单号 */
const orderNo = ref('')
/** 退款页预置单号（从订单详情跳转时带入） */
const refundOrderNo = ref('')

// 收银状态
const keyword = ref('')
const products = ref<ProductHit[]>([])
/** 收银点选区：分类页签与当前分类商品，替代只靠键盘搜索的单一入口 */
const posCategories = ref<Category[]>([])
const posProducts = ref<ProductHit[]>([])
const posCategoryId = ref<number | null>(null)
const posCatalogBusy = ref(false)
const posRootCategories = computed(() =>
  posCategories.value.filter((c) => c.parent_id == null)
)
/** 联想查询中：搜索框内显示轻量加载指示，不打断输入 */
const searchBusy = ref(false)
/** 收款提交中：按钮禁用防重复提交，保留宽度防抖 */
const paying = ref(false)
const cart = ref<(CartLine & { name: string })[]>([])
/** 养生技师：只允许把服务行指派给当日排班在岗的技师。 */
const technicians = ref<Technician[]>([])
const onDutyTechnicians = computed(() => technicians.value.filter((t) => t.on_duty))
/** 当前餐饮桌已验证下发的数量；退菜前不能把数量改到低于它。 */
const tableSentQty = computed<Record<number, number>>(() => {
  if (!activeTable.value || activeTable.value.scene !== 'DINING') return {}
  try {
    const lines = JSON.parse(activeTable.value.sent_json || '[]') as CartLine[]
    return lines.reduce<Record<number, number>>((m, l) => {
      m[l.sku_id] = (m[l.sku_id] || 0) + (l.qty || 0)
      return m
    }, {})
  } catch {
    return {}
  }
})
const cashTendered = ref('')
const lastOrder = ref('')
const toast = ref('')
/** toast 错误态独立于文案：后端异常原文未必带「错误」前缀，靠前缀判断会漏标红 */
const toastKind = ref<'info' | 'error'>('info')
const parked = ref<unknown[]>([])
/** 挂单弹窗：付款区按钮唤起，替代原独立挂单菜单 */
const parkDialogOpen = ref(false)
/** 会员识别弹窗：付款区按钮唤起 */
const memberDialogOpen = ref(false)

/** 付款后清空会员识别信息，避免下一单沿用上一顾客会员 */
function clearMember() {
  memberPhone.value = ''
  memberId.value = null
  memberBalances.value = null
  for (const l of cart.value) l.member_price_cents = null
}

/** 桌台购物车落库：点单改动防抖保存，切台/清台前立即冲刷。 */
function scheduleTableSave() {
  if (!activeTable.value) return
  if (tableSaveTimer) clearTimeout(tableSaveTimer)
  tableSaveTimer = setTimeout(() => {
    tableSaveTimer = null
    void flushTableCart()
  }, 400)
}

async function flushTableCart() {
  if (!activeTable.value) return
  try {
    await api.saveServiceTableCart(activeTable.value.table_id, JSON.stringify(cart.value))
  } catch (e) {
    showToast(`桌台购物车保存失败：${String(e)}`, true)
  }
}

async function selectSceneTable(table: ServiceTable, lines: CartLine[]) {
  if (activeTable.value && activeTable.value.table_id !== table.table_id) await flushTableCart()
  if (tableSaveTimer) {
    clearTimeout(tableSaveTimer)
    tableSaveTimer = null
  }
  activeTable.value = table
  couponCode.value = ''
  clearMember()
  cart.value = lines.map((l) => ({
    ...l,
    name: l.name || `商品 ${l.sku_id}`,
    member_price_cents: null,
  }))
  // 开台时绑定的会员随桌台恢复：重新识别后刷新会员价与积分展示。
  if (table.member_id && table.member_phone) {
    memberPhone.value = table.member_phone
    await findMember()
  }
  if (table.scene === 'CHESS') await refreshChessFee()
  showToast(`已选中 ${table.code}${lines.length ? '，继续点单' : '，可以点单'}`)
}

/** 棋牌包间按分钟计费：房费行由后端计时结果生成，随购物车进入统一报价与结算。 */
async function refreshChessFee() {
  const table = activeTable.value
  if (!table || table.scene !== 'CHESS' || !table.fee_sku_id) {
    stopChessFeeTimer()
    return
  }
  try {
    const fee = await api.chessTableFee(table.table_id)
    const others = cart.value.filter((l) => !l.is_chess_fee)
    cart.value = [
      ...others,
      {
        sku_id: table.fee_sku_id,
        qty: 1,
        unit_price_cents: fee.fee_cents,
        name: `包间费 ${fee.billable_minutes} 分钟（${fee.fee_unit === 'HOUR' ? '按小时价折算' : '按分钟价'}）`,
        is_chess_fee: true,
        chess_minutes: fee.billable_minutes,
      },
    ]
    if (!chessFeeTimer) {
      chessFeeTimer = setInterval(() => void refreshChessFee(), 30000)
    }
  } catch (e) {
    showToast(String(e), true)
  }
}

function stopChessFeeTimer() {
  if (chessFeeTimer) clearInterval(chessFeeTimer)
  chessFeeTimer = null
}

async function clearActiveTable() {
  if (!activeTable.value) return
  if (tableSaveTimer) clearTimeout(tableSaveTimer)
  tableSaveTimer = null
  stopChessFeeTimer()
  activeTable.value = null
  cart.value = []
  clearMember()
}

/** 结算成功释放桌台；清台失败不回滚已收款的订单，返回需提示的警告。 */
async function releaseActiveTable(): Promise<string | null> {
  const table = activeTable.value
  if (!table) return null
  activeTable.value = null
  stopChessFeeTimer()
  if (tableSaveTimer) clearTimeout(tableSaveTimer)
  tableSaveTimer = null
  try {
    await api.closeServiceTable(table.table_id, false)
    return null
  } catch (e) {
    return `订单已完成，但桌台 ${table.code} 清台失败：${String(e)}，请手动清台`
  }
}

watch(cart, scheduleTableSave, { deep: true })
watch(industry, () => {
  activeTable.value = null
  stopChessFeeTimer()
  void loadTechnicians()
  if (tableSaveTimer) {
    clearTimeout(tableSaveTimer)
    tableSaveTimer = null
  }
})

// 会员识别 / 优惠券 / 混合支付（§6.5 / §13）
const memberPhone = ref('')
const memberId = ref<number | null>(null)
const memberBalances = ref<{ principal_cents: number; bonus_cents: number; points: number } | null>(null)
const couponCode = ref('')
// 优惠券弹窗：券码从内联输入框改为图标按钮唤起弹窗输入
const couponDialogOpen = ref(false)
const couponDraft = ref('')
// 混合支付拆分：每项 { channel, amountYuan, authCode }
interface PaySplit { channel: string; amountYuan: string; authCode: string }
const paySplits = ref<PaySplit[]>([{ channel: 'CASH', amountYuan: '', authCode: '' }])
// 收银台支付状态（主流式交互）：先选支付方式，金额自动=应收，只补渠道必需信息
type PayChannel = 'CASH' | 'STORED_VALUE' | 'WECHAT' | 'ALIPAY'
const payChannel = ref<PayChannel>('CASH')
const payMode = ref<'simple' | 'mixed'>('simple')
/** 现金收银弹窗：点「现金」即弹出，弹窗内完成实收 / 找零 / 确认收款（对齐主流 POS tender 弹窗） */
const cashDialogOpen = ref(false)
const payAuthCode = ref('') // 微信/支付宝 B 扫 C：顾客付款码（扫码枪扫入或手输）
// Native 支付（§6.3）：顾客扫屏幕码；弹窗 + 轮询 + 本地确认兜底
const nativeDialog = ref<{ orderNo: string; codeUrl: string; qrSvg: string } | null>(null)
let nativeTimer: ReturnType<typeof setInterval> | null = null
// 渠道开关与网关模式（设置页配置；POS 启动时读取）
const channelEnabled = ref<Record<string, boolean>>({})
const gatewayModes = ref<{ wechat: string; alipay: string }>({ wechat: 'LOCAL', alipay: 'LOCAL' })
const nativeMode = ref(false) // 本单扫码拆分走 Native（顾客扫屏码）
// 二次授权弹窗（§12.4）：待批准的权限与业务回调
const authDialog = ref<{
  permission: string
  title: string
  resolve: ((nonce: string) => void) | null
  reject: ((reason: string) => void) | null
} | null>(null)
const authUser = ref('')
const authPass = ref('')
const authError = ref('')

// 扫码枪楔入捕获（§15.1）：记录按键时间戳，快速连续 + Enter 判为扫码
const scanBuffer = ref('')
const scanTimes = ref<number[]>([])
const scanGapMs = ref(SCAN_GAP_MS_DEFAULT)
const scanCapture = ref<HTMLInputElement | null>(null)

const banner = computed(() => {
  if (!license.value) return ''
  if (license.value.status === 'TRIAL_ACTIVE') {
    const ends = license.value.trial_ends_at
    if (!ends) return '试用期进行中'
    const days = Math.max(
      1,
      Math.ceil((new Date(ends).getTime() - Date.now()) / MS_PER_DAY)
    )
    return `试用期剩余 ${days} 天，到期后需激活`
  }
  if (license.value.status === 'GRACE_PERIOD') return '授权宽限期内，请尽快续期'
  if (license.value.status === 'OFFLINE_DEGRADED') return '授权已到期，前台收银仍可用，后台管理受限'
  return ''
})

/** 授权状态中文名（界面不直接暴露后端枚举）。 */
const LICENSE_STATUS_LABEL: Record<string, string> = {
  UNACTIVATED: '未激活',
  TRIAL_ACTIVE: '试用中',
  ACTIVE: '已激活',
  GRACE_PERIOD: '宽限期内',
  OFFLINE_DEGRADED: '已到期（降级）',
}

const licenseStatusLabel = computed(() => {
  const s = license.value?.status
  if (!s) return '未知'
  return LICENSE_STATUS_LABEL[s] ?? s
})

/** 试用期剩余天数（供激活页「试用15天」入口展示）。 */
const trialDaysLeft = computed(() => {
  if (license.value?.status !== 'TRIAL_ACTIVE') return 0
  const ends = license.value.trial_ends_at
  if (!ends) return 15
  return Math.max(1, Math.ceil((new Date(ends).getTime() - Date.now()) / MS_PER_DAY))
})

const subtotal = computed(() => cart.value.reduce((s, l) => s + l.qty * effectivePrice(l), 0))

/** 后端口径的应收金额（原价 − 促销 − 券）；未取到时退回前端估算。 */
const quote = ref<OrderQuote | null>(null)
let quoteSeq = 0
const payableAmount = computed(() => quote.value?.payable_cents ?? subtotal.value)

/** 行成交价：识别会员后按会员价显示（结算时与促销取优由后端裁决，前端只负责如实呈现）。 */
function effectivePrice(l: { unit_price_cents: number; member_price_cents?: number | null }): number {
  const mp = l.member_price_cents
  if (mp != null && mp > 0 && mp < l.unit_price_cents) return mp
  return l.unit_price_cents
}

/** 商品的会员价：未识别会员或未配置会员价时返回 null。 */
function memberPriceOf(p: ProductHit): number | null {
  if (memberId.value == null) return null
  const mp = p.member_price_cents
  if (mp == null || mp <= 0 || mp >= p.price_cents) return null
  return mp
}

function showToast(msg: string, isError = false) {
  toast.value = msg
  toastKind.value = isError ? 'error' : 'info'
  setTimeout(() => (toast.value = ''), isError ? ERROR_DURATION_MS : TOAST_DURATION_MS)
}

function showSceneError(message: string) {
  showToast(message, true)
}

/** 收银成功提示：有积分累计时附上本次积分 */
function pointsToast(base: string, points: number | undefined): string {
  return points && points > 0 ? `${base}，积分 +${points}` : base
}

// ===== 二次授权（§12.4）=====
function requireSensitiveAuth(permission: string, title: string): Promise<string> {
  return new Promise((resolve, reject) => {
    authDialog.value = { permission, title, resolve, reject }
    authUser.value = ''
    authPass.value = ''
    authError.value = ''
  })
}

async function confirmSensitiveAuth() {
  const d = authDialog.value
  if (!d) return
  try {
    const nonce = await api.requestSensitiveAuth(
      d.permission,
      authUser.value.trim(),
      authPass.value
    )
    d.resolve?.(nonce)
    authDialog.value = null
  } catch (e) {
    authError.value = String(e)
  }
}

function cancelSensitiveAuth() {
  const d = authDialog.value
  d?.reject?.('已取消二次授权')
  authDialog.value = null
}

// ===== 会员识别（§13）=====
/** 弹窗内联错误：识别失败时用户就盯着弹窗，全局 toast 在遮罩下不显眼 */
const memberFindError = ref('')
async function findMember(): Promise<boolean> {
  const phone = memberPhone.value.trim()
  if (!phone) {
    memberId.value = null
    memberBalances.value = null
    for (const l of cart.value) l.member_price_cents = null
    memberFindError.value = '请输入手机号或会员码'
    return false
  }
  try {
    const list = await api.listMembers(phone, 10)
    const m = list.find((x) => x.phone === phone || x.member_code === phone)
    if (!m) {
      memberId.value = null
      memberBalances.value = null
      for (const l of cart.value) l.member_price_cents = null
      memberFindError.value = `未找到会员「${phone}」，请核对后重试`
      return false
    }
    memberFindError.value = ''
    memberId.value = m.member_id
    memberBalances.value = {
      principal_cents: m.principal_cents,
      bonus_cents: m.bonus_cents,
      points: m.points,
    }
    await refreshCartMemberPrices()
    showToast(`会员已识别：${m.name ?? m.member_code}`)
    return true
  } catch (e) {
    memberFindError.value = `识别失败：${String(e)}`
    return false
  }
}

/** 识别会员后刷新购物车行的会员价：已有行也要按会员价成交。 */
async function refreshCartMemberPrices() {
  if (!cart.value.length) return
  try {
    const rows = await api.memberPrices(cart.value.map((l) => l.sku_id))
    const map = new Map(rows.map((r) => [r.sku_id, r.member_price_cents]))
    for (const l of cart.value) l.member_price_cents = map.get(l.sku_id) ?? null
  } catch {
    // 会员价刷新失败不阻断收银：后端结算仍会按会员价取优
  }
}

// ===== 扫码枪楔入（§15.1 / SCN-01）=====
function onScanKeydown(e: KeyboardEvent) {
  const now = Date.now()
  if (e.key === 'Enter') {
    const times = scanTimes.value
    const code = scanBuffer.value.trim()
    const fast =
      code.length >= SCAN_MIN_LENGTH &&
      times.length >= 2 &&
      now - times[0] < (times.length * scanGapMs.value)
    scanBuffer.value = ''
    scanTimes.value = []
    if (fast) {
      e.preventDefault()
      void handleScannedCode(code)
    } else if (code) {
      // 手动回车不能把联想第一条当扫码结果加入购物车：
      // 输入的是"可乐"却加进"可乐罐"是错单，必须让收银员从候选里明确选择。
      e.preventDefault()
      showToast('未匹配到精确条码，请从候选列表选择商品', true)
    }
    return
  }
  if (e.key.length === 1) {
    scanBuffer.value += e.key
    scanTimes.value.push(now)
  } else if (e.key === 'Backspace') {
    scanBuffer.value = scanBuffer.value.slice(0, -1)
  }
}

async function handleScannedCode(code: string) {
  // 顾客付款码优先（微信 18 位 10–15 前缀 / 支付宝 16–24 位 25–30 前缀纯数字，
  // 规则由后端 detect_payment_code 单点判定，前端不重复实现长度规则）：
  // 有待收金额时扫到付款码即走收款，不当商品条码查。
  if (cart.value.length && payShortfall.value > 0) {
    const channel = await api.detectPaymentCode(code)
    if (channel) {
      await applyPaymentCode(channel, code)
      return
    }
  }
  // 称重码（22/24 前缀 EAN-13）优先按 §15.2 解析
  if (/^2[24]\d{11}$/.test(code)) {
    try {
      const hit = await api.lookupWeigh(code)
      pushWeighToCart(hit)
      return
    } catch {
      // 解析失败则退回普通条码识别
    }
  }
  keyword.value = code
  // 扫码枪整码命中走精确查询，取消输入防抖避免双重请求
  if (searchTimer) {
    clearTimeout(searchTimer)
    searchTimer = null
  }
  await search()
}

/** 扫到顾客付款码：金额自动取剩余待收，落到当前渠道直接发起收款（金额不落文本框）。 */
async function applyPaymentCode(channel: string, code: string) {
  // 先取后端权威应收，避免按前端估算金额收款
  await refreshQuote()
  if (payShortfall.value <= 0) return
  payMode.value = 'simple'
  payChannel.value = channel as PayChannel
  payAuthCode.value = code
  nativeMode.value = false
  showToast(`已识别${PAY_CHANNEL_LABEL[channel] ?? channel}付款码，正在收款`)
  await submitCheckout()
}

function pushWeighToCart(hit: {
  sku_id: number
  product_name: string
  qty: number
  unit_price_cents: number
  line_total_cents: number
}) {
  cart.value.push({
    sku_id: hit.sku_id,
    qty: hit.qty,
    unit_price_cents: hit.unit_price_cents,
    name: hit.product_name,
  })
  showToast(`已加入称重商品：${hit.product_name}`)
}

function focusScan() {
  scanCapture.value?.focus()
}

onMounted(async () => {
  try {
    license.value = await api.bootstrap()
    await loadStoreProfile()
    await loadIndustries()
    // 首启固定四步：设置管理员 → 登录 → 选择行业 → 激活。
    // 激活页只在登录后出现，行业选择需要管理员会话才能落库。
    needAdmin.value = !(await api.hasUsers())
    activationPending.value = readActivationPending()
    onboarding.value =
      needAdmin.value ||
      !(storeProfile.value?.configured ?? false) ||
      activationPending.value
    view.value = 'login'
  } catch (e) {
    bootError.value = String(e)
  }
})

// ===== 门店档案与行业 =====
async function loadStoreProfile() {
  try {
    storeProfile.value = await api.getStoreProfile()
    storeNameDraft.value = storeProfile.value.store_name
    if (storeProfile.value.industry) pendingIndustry.value = storeProfile.value.industry
  } catch (e) {
    showToast(String(e), true)
  }
}

async function loadIndustries() {
  try {
    industries.value = await api.listIndustries()
  } catch (e) {
    showToast(String(e), true)
  }
}

/** 选定行业并写入分类模板 */
async function confirmIndustry() {
  if (!pendingIndustry.value) {
    showToast('请选择行业', true)
    return
  }
  try {
    storeProfile.value = await api.selectIndustry(pendingIndustry.value)
    showToast(`已切换为${storeProfile.value.industry_name}`)
  } catch (e) {
    showToast(String(e), true)
  }
}

/** 保存店名 */
async function saveStoreName() {
  try {
    storeProfile.value = await api.setStoreName(storeNameDraft.value)
    showToast('店名已保存')
  } catch (e) {
    showToast(String(e), true)
  }
}

/** 首次启动完成（行业 + 店名）后进入下一步 */
async function finishIndustrySetup() {
  if (!pendingIndustry.value) {
    showToast('请选择经营行业', true)
    return
  }
  if (!storeNameDraft.value.trim()) {
    showToast('请填写店名', true)
    return
  }
  try {
    // 行业若与当前档案不同，先落地该行业的分类模板，再保存店名
    if (pendingIndustry.value && pendingIndustry.value !== storeProfile.value?.industry) {
      storeProfile.value = await api.selectIndustry(pendingIndustry.value)
    }
    storeProfile.value = await api.setStoreName(storeNameDraft.value)
    await loadStoreProfile()
    if (needsActivation.value) {
      view.value = 'activate'
      void probeActivationService()
      showToast('行业已保存，下一步激活设备')
    } else {
      onboarding.value = false
      view.value = 'pos'
      showToast('初始化完成，可以开始收银')
    }
  } catch (e) {
    showToast(String(e), true)
  }
}

function gotoRefund(orderNo?: string) {
  refundOrderNo.value = orderNo ?? ''
  view.value = 'refund'
}

function gotoOrder(no: string) {
  orderNo.value = no
  view.value = 'order'
}

// ===== 锁机码逻辑 =====
/** 读取本机锁机码；本地存储不可用时保持为空，锁机按钮会引导重设 */
function loadLockCode() {
  try {
    const stored = localStorage.getItem(LOCK_CODE_KEY) || ''
    lockCode.value = stored
    // 历史版本存明文：读取时静默迁移为摘要，旧码仍可继续解锁。
    if (stored && !LOCK_HASH_RE.test(stored)) {
      void hashLockCode(stored)
        .then((digest) => {
          localStorage.setItem(LOCK_CODE_KEY, digest)
          lockCode.value = digest
        })
        .catch(() => {
          // 摘要不可用时保留明文比对，不阻断锁屏
        })
    }
  } catch {
    lockCode.value = ''
  }
}

/** 锁机码只在本机保存 SHA-256 摘要，避免 localStorage 明文可见。 */
async function hashLockCode(code: string): Promise<string> {
  const buf = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(code))
  return Array.from(new Uint8Array(buf))
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('')
}

/** 保存锁机码；若由「锁机」按钮触发的设置，保存后立即进入锁屏 */
async function saveLockCode() {
  const code = lockCodeDraft.value.trim()
  if (!code) {
    showToast('请输入锁机码', true)
    return
  }
  try {
    const digest = await hashLockCode(code)
    localStorage.setItem(LOCK_CODE_KEY, digest)
    lockCode.value = digest
    lockCodeDraft.value = ''
    lockSetupOpen.value = false
    const shouldLock = pendingLockAfterSetup.value
    pendingLockAfterSetup.value = false
    if (shouldLock) {
      lockInput.value = ''
      lockError.value = ''
      locked.value = true
    }
    showToast('锁机码已设置')
  } catch (e) {
    showToast(String(e), true)
  }
}

/** 清除锁机码 */
function clearLockCode() {
  try {
    localStorage.removeItem(LOCK_CODE_KEY)
  } catch {
    // 本地存储不可用时忽略，锁机码仍从内存清空
  }
  lockCode.value = ''
  lockCodeDraft.value = ''
  showToast('锁机码已清除')
}

/** 锁屏：未设置锁机码时先引导设置，设置完成立即锁屏 */
function lockScreen() {
  if (locked.value) return
  lockInput.value = ''
  lockError.value = ''
  if (!lockCode.value) {
    pendingLockAfterSetup.value = true
    lockSetupOpen.value = true
    return
  }
  locked.value = true
}

/** 解锁：摘要比对（兼容历史明文），错误时给出提示并保持锁屏 */
async function unlock() {
  let matched = false
  try {
    matched = LOCK_HASH_RE.test(lockCode.value)
      ? (await hashLockCode(lockInput.value)) === lockCode.value
      : lockInput.value === lockCode.value
  } catch {
    matched = false
  }
  if (matched) {
    locked.value = false
    lockInput.value = ''
    lockError.value = ''
    showToast('已解锁')
  } else {
    lockError.value = '锁机码不正确'
  }
}

/** 取消锁机码设置：同时清除「设置后立即锁屏」的待办标记 */
function cancelLockSetup() {
  lockSetupOpen.value = false
  pendingLockAfterSetup.value = false
}

onMounted(() => {
  loadLockCode()
  // 窗口隐藏到托盘时后端发出 lock-request：从托盘恢复后保持锁屏，需输码解锁
  void listen('lock-request', () => {
    lockScreen()
  }).catch(() => {
    // 浏览器预览环境无 Tauri 事件，忽略即可
  })
})

/** 选择试用15天：接受当前试用期，直接进入收银（试用期由后端在首次启动时开启） */
function enterTrial() {
  if (license.value?.status !== 'TRIAL_ACTIVE') {
    showToast('试用期不可用，请使用激活码激活', true)
    return
  }
  setActivationPending(false)
  onboarding.value = false
  view.value = 'pos'
  showToast(`已进入试用，剩余 ${trialDaysLeft.value} 天`)
}

/** 探测激活服务可达性：不可达时提示用户改用备用通道，而不是直接报错 */
async function probeActivationService() {
  if (probingService.value) return
  probingService.value = true
  try {
    activationServiceReachable.value = await api.probeActivationService()
  } catch {
    activationServiceReachable.value = false
  } finally {
    probingService.value = false
  }
}

/** 打开官网获取激活码：由后端解析官方地址并唤起系统浏览器，界面不展示地址 */
async function openOfficialSite() {
  activateError.value = ''
  try {
    await api.openActivationSite()
  } catch (e) {
    activateError.value = String(e)
  }
}

/** 在线激活：输入激活码，向激活服务换取授权 */
async function submitActivateOnline() {
  activateError.value = ''
  if (!activateToken.value.trim()) {
    activateError.value = '请输入激活码'
    return
  }
  try {
    license.value = await api.activateOnline(
      activateToken.value.trim(),
      activateCustomer.value.trim() || null
    )
    if (license.value.status !== 'UNACTIVATED') {
      setActivationPending(false)
      needAdmin.value = !(await api.hasUsers())
      if (needAdmin.value || !(await applyCurrentSession())) {
        view.value = 'login'
      } else {
        await routeAfterLogin()
      }
      showToast('激活成功')
    }
  } catch (e) {
    activationServiceReachable.value = false
    activateError.value = String(e)
  }
}

/** 联网刷新授权：读取服务端最新状态（撤销/过期/续期以服务端为准） */
async function refreshLicense() {
  activateError.value = ''
  try {
    license.value = await api.refreshLicenseOnline()
    showToast('授权状态已刷新')
  } catch (e) {
    activateError.value = String(e)
  }
}

async function submitActivate() {
  activateError.value = ''
  try {
    license.value = renewMode.value
      ? await api.renew(activateToken.value)
      : await api.activate(activateToken.value)
    if (license.value.status !== 'UNACTIVATED') {
      setActivationPending(false)
      needAdmin.value = !(await api.hasUsers())
      if (needAdmin.value || !(await applyCurrentSession())) {
        view.value = 'login'
      } else {
        await routeAfterLogin()
      }
      showToast('激活成功')
    }
  } catch (e) {
    activateError.value = String(e)
  }
}

/** 从后端会话恢复界面角色；返回是否存在有效登录。 */
async function applyCurrentSession(): Promise<boolean> {
  try {
    const session = await api.currentSession()
    if (session) {
      currentRole.value = normalizeRole(session.role_name)
      return true
    }
  } catch {
    // 未登录或会话过期：保持最低权限默认值
  }
  return false
}

/** 登录成功后的唯一分流：未配置行业 → 选行业；待激活 → 激活；否则进收银。 */
async function routeAfterLogin() {
  await loadStoreProfile()
  if (!(storeProfile.value?.configured ?? false)) {
    pendingIndustry.value =
      storeProfile.value?.industry || pendingIndustry.value || ''
    storeNameDraft.value = storeProfile.value?.store_name || storeNameDraft.value
    view.value = 'industry'
    return
  }
  if (needsActivation.value) {
    view.value = 'activate'
    void probeActivationService()
    return
  }
  onboarding.value = false
  view.value = 'pos'
}

async function submitAdmin() {
  loginError.value = ''
  try {
    await api.initAdmin(adminUser.value, adminPass.value)
    needAdmin.value = false
    setActivationPending(true)
    onboarding.value = true
    // 建号后退出建号态，回到登录页，让管理员用刚设置的账号登录
    loginUser.value = adminUser.value
    loginPass.value = ''
    adminPass.value = ''
    view.value = 'login'
    showToast('管理员已创建，请使用该账号登录')
  } catch (e) {
    loginError.value = String(e)
  }
}

async function submitLogin() {
  loginError.value = ''
  try {
    const session = await api.login(loginUser.value, loginPass.value)
    currentRole.value = normalizeRole(session.role_name)
    currentUser.value = loginUser.value
    await routeAfterLogin()
  } catch (e) {
    loginError.value = String(e)
  }
}

async function search() {
  if (!keyword.value.trim()) {
    products.value = []
    searchBusy.value = false
    return
  }
  try {
    products.value = await api.lookup(keyword.value.trim())
  } catch (e) {
    showToast(String(e))
  } finally {
    searchBusy.value = false
  }
}

// ===== 自动联想：输入停顿 SEARCH_DEBOUNCE_MS 后自动查询，无需点击识别按钮 =====
let searchTimer: ReturnType<typeof setTimeout> | null = null
watch(keyword, (val) => {
  if (searchTimer) clearTimeout(searchTimer)
  if (!val.trim()) {
    products.value = []
    searchBusy.value = false
    return
  }
  searchBusy.value = true
  searchTimer = setTimeout(() => void search(), SEARCH_DEBOUNCE_MS)
})

/** 商品图片展示地址：本地图片走 Tauri 资产协议，在线图片直接用；均无则返回 null。 */
function productImageSrc(p: { image_path?: string | null; image_url?: string | null }): string | null {
  if (p.image_path) return convertFileSrc(p.image_path)
  if (p.image_url) return p.image_url
  return null
}

async function loadPosCategory(categoryId: number | null) {
  posCategoryId.value = categoryId
  try {
    posProducts.value = await api.posProducts(categoryId)
  } catch (e) {
    posProducts.value = []
    showToast(String(e), true)
  }
}

/** 进入收银台即装载分类与首个分类商品；商品/分类变更后回到收银会重新拉取。 */
/** 本地营业日 YYYY-MM-DD；技师排班按自然日，不跟随后台跨零点营业日切换。 */
function localDateString(d = new Date()): string {
  const y = d.getFullYear()
  const m = String(d.getMonth() + 1).padStart(2, '0')
  const day = String(d.getDate()).padStart(2, '0')
  return `${y}-${m}-${day}`
}

async function loadTechnicians() {
  try {
    technicians.value = await api.listTechnicians(localDateString())
  } catch (e) {
    technicians.value = []
    showToast(String(e), true)
  }
}

async function loadPosCatalog() {
  if (posCatalogBusy.value) return
  posCatalogBusy.value = true
  try {
    posCategories.value = await api.posCategories()
    const roots = posCategories.value.filter((c) => c.parent_id == null)
    const current = roots.find((c) => c.category_id === posCategoryId.value)
    // 默认落在首个真实经营分类；「未分类」通常为空，不让它占据首屏。
    const fallback = roots.find((c) => c.category_id !== 1) ?? roots[0]
    await loadPosCategory(current?.category_id ?? fallback?.category_id ?? null)
  } catch (e) {
    showToast(String(e), true)
  } finally {
    posCatalogBusy.value = false
  }
}

function selectPosCategory(categoryId: number) {
  if (categoryId === posCategoryId.value) return
  void loadPosCategory(categoryId)
}

/** 点选商品：称重商品必须走称重码，点击普通商品直接入车。 */
function addFromCatalog(p: ProductHit) {
  if (p.is_weighed) {
    showToast('称重商品请用称重码扫码入车，避免按件误算', true)
    focusScan()
    return
  }
  addToCart(p)
}

watch(view, (v) => {
  if (v === 'pos') {
    void loadPosCatalog()
    void loadTechnicians()
  }
})

function addToCart(p: ProductHit) {
  const existing = cart.value.find((l) => l.sku_id === p.sku_id)
  if (existing) existing.qty += 1
  else
    cart.value.push({
      sku_id: p.sku_id,
      qty: 1,
      unit_price_cents: p.price_cents,
      member_price_cents: memberPriceOf(p),
      name: p.spu_name,
      image_path: p.image_path,
      image_url: p.image_url,
    })
  keyword.value = ''
  products.value = []
}

function changeQty(index: number, delta: number) {
  const line = cart.value[index]
  if (!line || line.is_chess_fee) return
  const sent = tableSentQty.value[line.sku_id] || 0
  if (delta < 0 && line.qty - 1 < sent) {
    showToast(`该菜已下厨 ${sent} 份，减菜请用「退菜」`, true)
    return
  }
  line.qty += delta
  if (line.qty <= 0) cart.value.splice(index, 1)
}

/** 直接改数量：键盘输入比连点加减快，0 视为移除该行。 */
function setLineQty(index: number, raw: string) {
  const line = cart.value[index]
  if (!line || line.is_chess_fee) return
  const qty = Math.floor(Number(raw))
  if (!Number.isFinite(qty) || qty <= 0) {
    cart.value.splice(index, 1)
    return
  }
  const sent = tableSentQty.value[line.sku_id] || 0
  if (qty < sent) {
    showToast(`该菜已下厨 ${sent} 份，不能改成 ${qty}，减菜请用「退菜」`, true)
    return
  }
  line.qty = qty
}

/** 占用中的桌台换绑会员：后端校验门店开关，成功写审计。 */
async function rebindTableMember() {
  const table = activeTable.value
  if (!table) return
  if (memberId.value == null) {
    showToast('请先识别要换绑的会员', true)
    return
  }
  if (!window.confirm(`把桌台 ${table.code} 换绑到当前会员？`)) return
  try {
    const updated = await api.rebindServiceTableMember(table.table_id, memberId.value)
    activeTable.value = updated
    showToast(`已换绑会员 ${updated.member_phone ?? ''}`)
  } catch (e) {
    showToast(`换绑失败：${String(e)}`, true)
  }
}

/** 餐饮下单：把新增菜品送后厨并打印厨打单；购物车与失败都不会被静默吞掉。 */
async function submitTableOrder() {
  const table = activeTable.value
  if (!table || table.scene !== 'DINING') return
  if (!cart.value.length) {
    showToast('购物车为空', true)
    return
  }
  await flushTableCart()
  try {
    const items = await api.submitServiceTableOrder(table.table_id, 'DINING')
    if (!items.length) {
      showToast('没有新增菜品需要下单')
      return
    }
    // 已下发快照与当前购物车一致，本地同步，避免退菜按钮状态滞后。
    activeTable.value = { ...table, sent_json: JSON.stringify(cart.value) }
    const printMsg = await api.printKitchenTicket(table.table_id)
    showToast(`已下单 ${items.length} 类菜品；${printMsg}`, printMsg.includes('失败') || printMsg.includes('未配置'))
  } catch (e) {
    showToast(`下单失败：${String(e)}`, true)
  }
}

/** 已下单退菜：需作废权限；数量不能超过已下厨数量，操作写审计。 */
async function returnTableLine(line: CartLine) {
  const table = activeTable.value
  if (!table || table.scene !== 'DINING') return
  const sent = tableSentQty.value[line.sku_id] || 0
  if (sent <= 0) {
    showToast('该菜尚未下单，直接减少数量即可', true)
    return
  }
  const raw = window.prompt(`退菜数量（已下单 ${sent} 份）`, '1')
  if (raw == null) return
  const qty = Math.floor(Number(raw))
  if (!Number.isFinite(qty) || qty <= 0) {
    showToast('退菜数量必须大于 0', true)
    return
  }
  try {
    const updated = await api.returnServiceTableLine(
      table.table_id,
      'DINING',
      line.sku_id,
      qty,
      '前台退菜'
    )
    activeTable.value = updated
    cart.value = (JSON.parse(updated.cart_json || '[]') as CartLine[]).map((l) => ({
      ...l,
      name: l.name || `商品 ${l.sku_id}`,
      member_price_cents: null,
    }))
    showToast(`已退 ${qty} 份，桌台与后厨已同步`)
  } catch (e) {
    showToast(`退菜失败：${String(e)}`, true)
  }
}

/** 清空当前购物车：主流 POS 的清空动作，避免逐行减到 0。 */
function clearCart() {
  if (!cart.value.length) return
  if (activeTable.value?.scene === 'CHESS') {
    showToast('棋牌房费按计时自动生成，不能清空购物车', true)
    return
  }
  if (Object.keys(tableSentQty.value).length) {
    showToast('本桌已有下厨菜品，请逐项退菜后再清空', true)
    return
  }
  if (!window.confirm('清空当前购物车的全部商品？')) return
  cart.value = []
  showToast('已清空购物车')
}

/** 付款成功后重置收银台：渠道回现金、清付款码/券/拆分行，回到默认单笔模式。 */
function resetPayState() {
  cashTendered.value = ''
  couponCode.value = ''
  couponDraft.value = ''
  payAuthCode.value = ''
  nativeMode.value = false
  payMode.value = 'simple'
  payChannel.value = 'CASH'
  paySplits.value = [{ channel: 'CASH', amountYuan: '', authCode: '' }]
}

async function submitCheckout() {
  if (!cart.value.length) {
    showToast('购物车为空', true)
    return
  }
  if (paying.value) return
  // 券未达门槛 / 专属券未识别会员时后端会拒绝收款；提前给可行动提示，避免白点
  if (couponRecoverable.value) {
    showToast(couponHint.value, true)
    return
  }
  // 现金必须经弹窗确认，杜绝「取消弹窗 → 主收款按钮空实收直接成交」的账实断点
  if (payMode.value === 'simple' && payChannel.value === 'CASH' && !cashDialogOpen.value) {
    cashTendered.value = ''
    cashDialogOpen.value = true
    return
  }
  paying.value = true
  try {
    let payments: Array<{
      channel: string
      amount_cents: number
      tendered_cents: number | null
      change_cents: number | null
      auth_code: string | null
      mode: string | null
    }>
    if (payMode.value === 'simple') {
      // 主流式单笔支付：金额自动=应收，只按渠道补必要信息
      const ch = payChannel.value
      // 单笔支付金额只认应收，不读组合拆分残留，避免把 3 元单算成 0 元流水
      const amount = payableAmount.value
      if (amount < 0) {
        showToast('支付金额异常（负数），请刷新报价', true)
        return
      }
      if (amount === 0) {
        // 零元单（全赠品/全额券）：不需要也不允许造一笔 0 元支付
        payments = []
      } else if (ch === 'WECHAT' || ch === 'ALIPAY') {
        if (!nativeMode.value && !payAuthCode.value.trim()) {
          showToast('请扫描或输入顾客付款码', true)
          return
        }
        payments = [
          {
            channel: ch,
            amount_cents: amount,
            tendered_cents: null,
            change_cents: null,
            auth_code: nativeMode.value ? null : payAuthCode.value.trim(),
            mode: nativeMode.value ? 'NATIVE' : null,
          },
        ]
      } else if (ch === 'CASH') {
        // 现金实收由弹窗收集；找零 = 实收 − 应收，不得为负
        const t = Math.round(Number(cashTendered.value || '0') * CENTS_PER_YUAN)
        // 现金单必须先录入实收，0 元实收不能当成「刚好付清」提交
        if (amount > 0 && t <= 0) {
          showToast('请输入实收现金金额', true)
          return
        }
        if (t > 0 && t < amount) {
          showToast('实收现金不足应收金额', true)
          return
        }
        payments = [
          {
            channel: 'CASH',
            amount_cents: amount,
            tendered_cents: t > 0 ? t : null,
            change_cents: t > 0 ? t - amount : null,
            auth_code: null,
            mode: null,
          },
        ]
      } else {
        if (memberId.value == null) {
          showToast('储值支付需先识别会员', true)
          return
        }
        payments = [
          {
            channel: 'STORED_VALUE',
            amount_cents: amount,
            tendered_cents: null,
            change_cents: null,
            auth_code: null,
            mode: null,
          },
        ]
      }
    } else {
      // 组合支付：按拆分行提交（现金行可叠加实收/找零）
      payments = paySplits.value
        .filter((s) => Number(s.amountYuan) > 0)
        .map((s) => {
          const amount = Math.round(Number(s.amountYuan) * CENTS_PER_YUAN)
          const isScan = s.channel === 'WECHAT' || s.channel === 'ALIPAY'
          // 现金：实收未填视为刚好付清；找零 = 实收 − 应收，不得为负
          let tendered = amount
          let change = 0
          if (s.channel === 'CASH') {
            const t = Math.round(Number(cashTendered.value || '0') * CENTS_PER_YUAN)
            if (t > 0) {
              if (t < amount) {
                throw new Error('实收现金不足以支付该笔金额')
              }
              tendered = t
              change = t - amount
            }
          }
          return {
            channel: s.channel,
            amount_cents: amount,
            tendered_cents: s.channel === 'CASH' ? tendered : null,
            change_cents: s.channel === 'CASH' ? change : null,
            auth_code: isScan && !nativeMode.value ? s.authCode || null : null,
            mode: isScan && nativeMode.value ? 'NATIVE' : null,
          }
        })
      const sum = payments.reduce((s, p) => s + p.amount_cents, 0)
      // 组合支付必须刚好等于应收：多收/少收都在收款前拦下，不能靠后端兜底
      if (sum !== (quote.value?.payable_cents ?? -1)) {
        showToast(
          `组合支付合计 ${(sum / CENTS_PER_YUAN).toFixed(2)} 元与应收 ${(
            (quote.value?.payable_cents ?? 0) / CENTS_PER_YUAN
          ).toFixed(2)} 元不一致`,
          true
        )
        return
      }
      if (
        payments.some((p) => p.channel === 'STORED_VALUE') &&
        memberId.value == null
      ) {
        showToast('储值支付需先识别会员', true)
        return
      }
      if (!payments.length) {
        showToast('请填写支付金额', true)
        return
      }
    }
    const r = await api.checkout(
      cart.value,
      payments,
      memberId.value,
      couponCode.value.trim() || null,
      activeTable.value?.code ?? null,
      activeTable.value?.table_id ?? null
    )
    lastOrder.value = r.order_no
    if (r.status === 'PENDING_PAYMENT' && r.code_url) {
      // Native：展示二维码并轮询（§6.3）
      const svg = await api.qrSvg(r.code_url)
      nativeDialog.value = { orderNo: r.order_no, codeUrl: r.code_url, qrSvg: svg }
      startNativePolling(r.order_no)
      return
    }
    const tableWarn = await releaseActiveTable()
    clearMember()
    cart.value = []
    resetPayState()
    cashDialogOpen.value = false
    showToast(
      pointsToast(`收银成功：${r.order_no}`, r.points_earned) + (tableWarn ? `；${tableWarn}` : ''),
      !!tableWarn
    )
  } catch (e) {
    showToast(String(e), true)
  } finally {
    paying.value = false
  }
}

// ===== Native 支付轮询（§6.3）=====
function startNativePolling(orderNo: string) {
  stopNativePolling()
  nativeTimer = setInterval(async () => {
    try {
      const r = await api.pollNativePay(orderNo)
      if (r.status === 'COMPLETED') {
        stopNativePolling()
        closeNativeDialog()
        const tableWarn = await releaseActiveTable()
        clearMember()
        cart.value = []
        resetPayState()
        showToast(
          pointsToast(`收银成功：${orderNo}`, r.points_earned) + (tableWarn ? `；${tableWarn}` : ''),
          !!tableWarn
        )
      } else if (r.status === 'CLOSED' || r.status.startsWith('FAILED')) {
        stopNativePolling()
        closeNativeDialog()
        showToast(`扫码支付未完成（${r.status === 'CLOSED' ? '订单已关闭' : '支付失败'}），订单可在「订单作废/退货」中处理`, true)
      }
    } catch (e) {
      // 查单异常保持轮询，网络恢复后收敛
      showToast(String(e), true)
    }
  }, NATIVE_POLL_INTERVAL_MS)
}

function stopNativePolling() {
  if (nativeTimer) {
    clearInterval(nativeTimer)
    nativeTimer = null
  }
}

function closeNativeDialog() {
  nativeDialog.value = null
  nativeMode.value = false
}

async function cancelNative() {
  const d = nativeDialog.value
  if (!d) return
  stopNativePolling()
  try {
    await api.cancelNativePay(d.orderNo)
    showToast('已取消 Native 支付，订单可在订单操作中作废', true)
  } catch (e) {
    showToast(String(e), true)
  }
  closeNativeDialog()
}

/** 本地确认模式（无真实渠道凭据）：操作员确认顾客已扫码完成 */
async function confirmNativeLocal() {
  const d = nativeDialog.value
  if (!d) return
  stopNativePolling()
  try {
    const points = await api.confirmNativePay(d.orderNo)
    closeNativeDialog()
    const tableWarn = await releaseActiveTable()
    clearMember()
    cart.value = []
    resetPayState()
    showToast(
      pointsToast(`收银成功：${d.orderNo}`, points) + (tableWarn ? `；${tableWarn}` : ''),
      !!tableWarn
    )
  } catch (e) {
    showToast(String(e), true)
  }
}

const isLocalGateway = computed(
  () => gatewayModes.value.wechat === 'LOCAL' && gatewayModes.value.alipay === 'LOCAL'
)

// 渠道开关 + 网关模式读取（POS 收银台过滤可选渠道）
async function loadPayChannels() {
  try {
    const c = await api.getPayChannels()
    channelEnabled.value = {
      CASH: c.cash,
      WECHAT: c.wechat,
      ALIPAY: c.alipay,
      STORED_VALUE: c.stored_value,
    }
  } catch {
    // 读取失败不阻断收银（§1.2），渠道可用性由后端结算时校验兜底
    channelEnabled.value = { CASH: true, WECHAT: true, ALIPAY: true, STORED_VALUE: true }
  }
  try {
    const g = await api.paymentGatewayStatus()
    gatewayModes.value = { wechat: g.wechat_mode, alipay: g.alipay_mode }
  } catch {
    gatewayModes.value = { wechat: 'LOCAL', alipay: 'LOCAL' }
  }
}

function addPaySplit() {
  paySplits.value.push({ channel: 'STORED_VALUE', amountYuan: '', authCode: '' })
}

function removePaySplit(index: number) {
  if (paySplits.value.length > 1) paySplits.value.splice(index, 1)
}

/** 支付拆分录入差额：应付 − 已录入 */
const paidPreview = computed(() =>
  paySplits.value.reduce((s, p) => s + (Number(p.amountYuan) || 0) * CENTS_PER_YUAN, 0)
)
const payShortfall = computed(() => payableAmount.value - paidPreview.value)

// ===== 收银台（主流式交互）辅助 =====
/** 单笔支付方式清单：受设置页渠道开关过滤 */
const payMethods = computed(() =>
  (
    [
      { id: 'CASH', label: '现金', icon: 'cash' },
      { id: 'WECHAT', label: '微信', icon: 'chat' },
      { id: 'ALIPAY', label: '支付宝', icon: 'phone' },
      { id: 'STORED_VALUE', label: '储值', icon: 'wallet' },
    ] as Array<{ id: PayChannel; label: string; icon: 'cash' | 'chat' | 'phone' | 'wallet' }>
  ).filter((m) => channelEnabled.value[m.id] !== false)
)

/** 切换支付模式时清空上一模式的组合明细，避免把上次的拆分金额带入本次收款。 */
function resetPaySplits() {
  paySplits.value = [{ channel: 'CASH', amountYuan: '', authCode: '' }]
}

function enterMixedMode() {
  payMode.value = 'mixed'
  resetPaySplits()
}

function selectPayChannel(id: PayChannel) {
  payMode.value = 'simple'
  resetPaySplits()
  payChannel.value = id
  // 现金收银进入弹窗流程：展示应收、快捷面额、实收与找零
  if (id === 'CASH' && cart.value.length) {
    cashTendered.value = ''
    cashDialogOpen.value = true
  }
}

/** 微信/支付宝面板出现时自动聚焦付款码输入，扫码枪无需点选即可扫入 */
const payCodeInput = ref<HTMLInputElement | null>(null)
watch(
  () => [payMode.value, payChannel.value, nativeMode.value] as const,
  () => {
    if (
      payMode.value === 'simple' &&
      !nativeMode.value &&
      (payChannel.value === 'WECHAT' || payChannel.value === 'ALIPAY')
    ) {
      void nextTick(() => payCodeInput.value?.focus())
    }
  }
)

/** 现金弹窗打开时自动聚焦实收输入，扫码/手输无需点选 */
const cashDialogInput = ref<HTMLInputElement | null>(null)
watch(cashDialogOpen, (open) => {
  if (open) void nextTick(() => cashDialogInput.value?.focus())
})

/** 现金找零：实收 − 应收；不足时显示待收差额 */
const cashTenderedCents = computed(() =>
  Math.round(Number(cashTendered.value || '0') * CENTS_PER_YUAN)
)
const changeDue = computed(() => Math.max(0, cashTenderedCents.value - payableAmount.value))
const cashShort = computed(() => Math.max(0, payableAmount.value - cashTenderedCents.value))
const cashOverpaid = computed(() => cashTenderedCents.value > 0 && cashTenderedCents.value >= payableAmount.value)

/** 快捷实收面额：常见整钞 + 「刚好」付清 */
const QUICK_CASH_YUAN = [10, 20, 50, 100]
function exactCash() {
  cashTendered.value = (payableAmount.value / CENTS_PER_YUAN).toFixed(2)
}

/** 现金键盘：3 列数字 + 小数点 + 退格，贴合收银员盲打顺序 */
const CASH_KEYPAD = ['7', '8', '9', '4', '5', '6', '1', '2', '3', '.', '0', '退格'] as const

/** 实收输入只保留两位小数，避免 0.999 这类金额被静默四舍五入。 */
function sanitizeCashInput(raw: string): string {
  let s = raw.replace(/[^\d.]/g, '')
  const dot = s.indexOf('.')
  if (dot >= 0) {
    s = `${s.slice(0, dot)}.${s.slice(dot + 1).replace(/\./g, '').slice(0, 2)}`
  }
  s = s.replace(/^0+(?=\d)/, '')
  return s.slice(0, 9)
}

function onCashInput(e: Event) {
  const el = e.target as HTMLInputElement
  const s = sanitizeCashInput(el.value)
  cashTendered.value = s
  if (el.value !== s) el.value = s
}

/** 小键盘录入：小数点只允许一个，整数首位不保留多余 0。 */
function pressCashKey(key: string) {
  if (key === '退格') {
    cashTendered.value = cashTendered.value.slice(0, -1)
    return
  }
  if (key === '.') {
    if (!cashTendered.value) cashTendered.value = '0.'
    else if (!cashTendered.value.includes('.')) cashTendered.value += '.'
    return
  }
  const cur = cashTendered.value
  if (cur.includes('.') && (cur.split('.')[1]?.length ?? 0) >= 2) return
  cashTendered.value = sanitizeCashInput(cur === '0' ? key : cur + key)
}

function openCouponDialog() {
  couponDraft.value = couponCode.value
  couponDialogOpen.value = true
}

/** 券有效但当前条件未满足：门槛不足 / 专属券未识别会员。二者均可恢复，保留券码待条件满足自动生效。 */
const couponRecoverable = computed(
  () => quote.value?.coupon_reason === 'BELOW_THRESHOLD' || quote.value?.coupon_reason === 'MEMBER_MISMATCH'
)

/** 券提示：未达门槛给可行动的凑单差额，券本身无效则显示后端原文案 */
const couponHint = computed(() => {
  const q = quote.value
  if (!q?.coupon_note) return ''
  if (q.coupon_reason === 'BELOW_THRESHOLD' && q.coupon_shortfall_cents != null) {
    return `还差 ¥${formatMoney(q.coupon_shortfall_cents)} 可用券，加购后自动生效 · 不用请清除券`
  }
  return q.coupon_note
})

async function applyCouponDialog() {
  const code = couponDraft.value.trim()
  if (!code) {
    showToast('请输入优惠券码', true)
    return
  }
  couponCode.value = code
  couponDialogOpen.value = false
  // 显式校验券码：报价失败时回滚，避免显示「券已用」却按原价收款
  const ok = await refreshQuote({ silent: true })
  if (!ok) {
    couponCode.value = ''
    couponDraft.value = ''
    showToast('优惠券校验失败，已取消使用', true)
    return
  }
  const reason = quote.value?.coupon_reason
  if (couponRecoverable.value) {
    // 可恢复状态：门槛不足 / 专属券未识别会员。券本身有效 → 保留券码，待条件满足后报价自动生效
    showToast(couponHint.value)
    return
  }
  if (reason) {
    // 券本身无效（不存在/已核销/过期/未生效）：清掉券码，避免一直挂在收银台误导
    const note = quote.value?.coupon_note ?? '优惠券不可用'
    couponCode.value = ''
    couponDraft.value = ''
    void refreshQuote({ silent: true })
    showToast(`${note}，已取消使用`, true)
    return
  }
  showToast('优惠券已应用')
}
function clearCoupon() {
  couponCode.value = ''
  couponDraft.value = ''
  void refreshQuote()
}

/** 报价刷新：购物车/会员变化后向后端取权威应收金额；券码变化由 applyCoupon/clearCoupon 显式触发，避免竞态。
 *  返回是否成功，silent 用于显式校验场景（由调用方给出更准确的提示）。 */
async function refreshQuote(opts: { silent?: boolean } = {}): Promise<boolean> {
  if (!cart.value.length) {
    quote.value = null
    return false
  }
  // 竞态守卫：只接受最后一次请求的结果，避免旧报价覆盖新购物车
  const seq = ++quoteSeq
  try {
    const q = await api.quoteOrder(
      cart.value,
      memberId.value,
      couponCode.value.trim() || null,
      activeTable.value?.table_id ?? null
    )
    if (seq !== quoteSeq) return false
    quote.value = q
    return true
  } catch (e) {
    if (seq !== quoteSeq) return false
    quote.value = null
    if (!opts.silent) showToast(`应收金额计算失败：${String(e)}`, true)
    return false
  }
}

watch(
  [() => cart.value, memberId],
  () => {
    void refreshQuote()
  },
  { deep: true }
)

async function park() {
  if (!cart.value.length) {
    showToast('购物车为空，无需挂单', true)
    return
  }
  try {
    await api.parkCart(JSON.stringify(cart.value), null)
    cart.value = []
    showToast('已挂单')
    if (parkDialogOpen.value) await loadParked()
  } catch (e) {
    showToast(String(e), true)
  }
}

async function loadParked() {
  try {
    parked.value = await api.listParked()
  } catch (e) {
    showToast(String(e), true)
  }
}

async function resume(parkId: number) {
  try {
    const json = await api.resumeCart(parkId)
    const parsed = JSON.parse(json) as Array<Record<string, unknown>>
    // 挂单快照里的会员价是当时价格：取单后必须丢弃，由后端按当前会员等级重算。
    for (const line of parsed) delete line.member_price_cents
    cart.value = parsed as unknown as typeof cart.value
    parkDialogOpen.value = false
    view.value = 'pos'
    showToast('已取单，促销将现场重算')
  } catch (e) {
    showToast(String(e), true)
  }
}

async function printReceipt() {
  if (!lastOrder.value) return
  try {
    const r = await api.printReceipt(lastOrder.value)
    showToast(r)
  } catch (e) {
    showToast(String(e))
  }
}

// ===== 交班汇总（§5.4）=====
interface ShiftRow { cashier_id: number; order_count: number; total_cents: number; pay_cents: number; cash_net_cents: number }
const shiftId = ref(1)
const shiftRows = ref<ShiftRow[]>([])
const openingFloat = ref('0')
const actualCash = ref('0')
const currentShiftId = ref(0)
async function loadShift() {
  try {
    shiftRows.value = (await api.shiftReport(shiftId.value)) as ShiftRow[]
  } catch (e) {
    showToast(String(e), true)
  }
}
async function doOpenShift() {
  try {
    currentShiftId.value = await api.openShift(Math.round(Number(openingFloat.value || '0') * CENTS_PER_YUAN))
    shiftId.value = currentShiftId.value
    showToast(`已开班，班次 ${currentShiftId.value}`)
    await loadShift()
  } catch (e) {
    showToast(String(e), true)
  }
}
async function doCloseShift() {
  try {
    const nonce = await requireSensitiveAuth('pos.shift.close', '关班授权')
    const r = await api.closeShift(
      currentShiftId.value,
      Math.round(Number(actualCash.value || '0') * CENTS_PER_YUAN),
      null,
      nonce
    )
    showToast(`已关班，现金差额 ${formatMoney(r.difference_cents)}`)
    currentShiftId.value = 0
  } catch (e) {
    if (String(e) !== '已取消二次授权') showToast(String(e), true)
  }
}


// ===== 统一报表中心（34 张报表）=====
const reportMetas = ref<ReportMeta[]>([])
const reportGroups = computed(() => {
  const groups: Array<{ name: string; items: ReportMeta[] }> = []
  for (const meta of reportMetas.value) {
    let g = groups.find((x) => x.name === meta.group)
    if (!g) {
      g = { name: meta.group, items: [] }
      groups.push(g)
    }
    g.items.push(meta)
  }
  return groups
})
const activeReportId = ref('sales_daily')
const activeReport = ref<ReportResult | null>(null)
const reportLoading = ref(false)
const centerFrom = ref(today())
const centerTo = ref(today())

async function loadReportCatalog() {
  try {
    reportMetas.value = await api.reportCatalog()
    if (reportMetas.value.length && !reportMetas.value.some((m) => m.id === activeReportId.value)) {
      activeReportId.value = reportMetas.value[0].id
    }
  } catch (e) {
    showToast(String(e), true)
  }
}

async function runActiveReport() {
  reportLoading.value = true
  try {
    activeReport.value = await api.genericReport(activeReportId.value, centerFrom.value, centerTo.value)
  } catch (e) {
    showToast(String(e), true)
  } finally {
    reportLoading.value = false
  }
}

function selectReport(id: string) {
  reportTool.value = null
  activeReportId.value = id
  void runActiveReport()
}

/** 左栏「工具」分组：班次管理 / 订单作废退货，与报表查询互斥显示。 */
type ReportTool = 'shift' | 'orderOp'
const reportTool = ref<ReportTool | null>(null)
function openTool(t: ReportTool) {
  reportTool.value = t
}

/** 导出当前报表为 CSV：列头 + 数据行，金额/占比已是格式化字符串，直接引用。 */
function exportActiveReport() {
  const r = activeReport.value
  if (!r) return
  const esc = (s: string) => `"${s.replace(/"/g, '""')}"`
  const lines = [r.columns.map((c) => esc(c.label)).join(',')]
  for (const row of r.rows) lines.push(row.map(esc).join(','))
  const blob = new Blob(['\ufeff' + lines.join('\r\n')], { type: 'text/csv;charset=utf-8' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = `${r.id}-${centerFrom.value}_${centerTo.value}.csv`
  a.click()
  URL.revokeObjectURL(url)
  showToast('报表已导出')
}

// ===== 系统配置（§15/§14）=====
const configs = ref<ConfigRow[]>([])
const configDraft = ref<Record<string, string>>({})
/** 积分配置页：只取积分相关的三个键，避免在「系统配置」里重复出现。 */
const POINTS_CONFIG_KEYS = ['points_enabled', 'points_per_yuan', 'points_redeem_rate']
const pointsConfigs = computed(() =>
  POINTS_CONFIG_KEYS.map((key) => configs.value.find((c) => c.config_key === key)).filter(
    (c): c is ConfigRow => !!c
  )
)
/** 支付渠道开关由 payment 页专属管理，系统配置页不重复展示 */
const PAY_CHANNEL_CONFIG_KEYS = ['pay_channel_cash_enabled', 'pay_channel_wechat_enabled', 'pay_channel_alipay_enabled', 'pay_channel_sv_enabled']
/** 系统配置按功能分区：同类的键归一组，避免一屏大杂烩。键顺序即展示顺序。 */
const SYSTEM_CONFIG_GROUPS: Array<{ title: string; hint: string; keys: string[] }> = [
  { title: '营业规则', hint: '影响营业日切与收银基础规则', keys: ['store_timezone', 'business_day_cutoff', 'max_discount_ratio', 'allow_negative_stock_default'] },
  { title: '支付时序', hint: '付款码 / 主扫的轮询与超时参数', keys: ['micropay_poll_ms', 'micropay_timeout_s', 'native_poll_ms', 'native_timeout_s'] },
  { title: '收银设备', hint: '扫码枪等外设参数', keys: ['scan_gap_ms'] },
  { title: '经营费用', hint: '利润报表按天均摊的月度固定费用', keys: ['expense_rent_yuan', 'expense_utility_yuan', 'expense_labor_yuan'] },
  { title: '打印设置', hint: '网络打印机地址与结算自动打印', keys: ['print_host', 'print_port', 'print_auto'] },
  { title: '行业规则', hint: '棋牌房费与桌台会员策略，默认值可被单包间覆盖', keys: ['chess_fee_unit', 'chess_min_minutes', 'chess_round_minutes', 'table_member_bind', 'table_allow_member_rebind'] },
  { title: '激活服务', hint: '在线激活服务地址，留空使用内置服务', keys: ['activation_endpoint'] },
]
const systemConfigGroups = computed(() => {
  const grouped = SYSTEM_CONFIG_GROUPS.map((g) => ({
    title: g.title,
    hint: g.hint,
    items: g.keys
      .map((k) => configs.value.find((c) => c.config_key === k))
      .filter((c): c is ConfigRow => !!c && c.editable),
  })).filter((g) => g.items.length)
  // 兜底：新增但未归组的配置项仍可见，避免遗漏；积分与支付渠道有专属页面，排除
  const assigned = SYSTEM_CONFIG_GROUPS.flatMap((g) => g.keys)
  const orphans = configs.value.filter(
    (c) =>
      c.editable &&
      !assigned.includes(c.config_key) &&
      !POINTS_CONFIG_KEYS.includes(c.config_key) &&
      !PAY_CHANNEL_CONFIG_KEYS.includes(c.config_key)
  )
  if (orphans.length) grouped.push({ title: '其他', hint: '尚未归类的配置项', items: orphans })
  return grouped
})
/** 配置键 → 中文名（复用后端元数据，UI 不硬编码英文键） */
function configLabel(key: string): string {
  return configs.value.find((c) => c.config_key === key)?.label ?? key
}
/** 固定选项配置：值仍以字符串落库，界面不要求用户记枚举。 */
function configOptions(key: string): Array<{ value: string; label: string }> {
  if (key === 'chess_fee_unit') {
    return [
      { value: 'MINUTE', label: '按分钟计价（单价=每分钟）' },
      { value: 'HOUR', label: '按小时计价（单价=每小时）' },
    ]
  }
  if (key === 'table_member_bind') {
    return [
      { value: 'OPTIONAL', label: '可不绑会员' },
      { value: 'REQUIRED', label: '必须绑会员才能开台' },
    ]
  }
  return []
}
async function loadConfigs() {
  try {
    configs.value = await api.getConfigs()
    const draft: Record<string, string> = {}
    for (const c of configs.value) draft[c.config_key] = c.config_value
    configDraft.value = draft
    const gap = Number(draft['scan_gap_ms'])
    if (Number.isFinite(gap) && gap > 0) scanGapMs.value = gap
  } catch (e) {
    showToast(String(e), true)
  }
}
async function saveConfig(key: string): Promise<boolean> {
  try {
    await api.setConfig(key, configDraft.value[key] ?? '')
    showToast('配置已保存')
    await loadConfigs()
    return true
  } catch (e) {
    showToast(String(e), true)
    return false
  }
}

/** 开关型配置：点击即翻转并保存（'1' 启用 / '0' 停用）；失败回滚到旧值，避免界面与后端不一致。 */
async function toggleConfigSwitch(key: string) {
  const prev = configDraft.value[key]
  const next = prev === '1' ? '0' : '1'
  configDraft.value[key] = next
  if (!(await saveConfig(key))) configDraft.value[key] = prev
}

// ===== 打印重试队列（§7.6）=====
const printQueue = ref<string[]>([])
async function loadPrintQueue() {
  try {
    printQueue.value = await api.printRetries()
  } catch (e) {
    showToast(String(e), true)
  }
}

// ===== 支付设置（§6.1/§6.8）：渠道开关 + 凭据管理 =====
const credChannel = ref<'WECHAT' | 'ALIPAY'>('WECHAT')
const credDraft = ref<Record<string, string>>({})
const credSaved = ref<Record<string, boolean>>({})
const channelDraft = ref<Record<string, string>>({})

async function loadPaymentSettings() {
  try {
    const g = await api.paymentGatewayStatus()
    gatewayModes.value = { wechat: g.wechat_mode, alipay: g.alipay_mode }
    if (g.wechat_error) showToast(`微信网关状态异常：${g.wechat_error}`, true)
    if (g.alipay_error) showToast(`支付宝网关状态异常：${g.alipay_error}`, true)
  } catch (e) {
    showToast(String(e), true)
  }
  try {
    const c = await api.getPayChannels()
    channelDraft.value = {
      pay_channel_cash_enabled: c.cash ? '1' : '0',
      pay_channel_wechat_enabled: c.wechat ? '1' : '0',
      pay_channel_alipay_enabled: c.alipay ? '1' : '0',
      pay_channel_sv_enabled: c.stored_value ? '1' : '0',
    }
  } catch (e) {
    showToast(String(e), true)
  }
}

async function toggleChannel(key: string, current: string) {
  const next = current === '1' ? '0' : '1'
  try {
    await api.setConfig(key, next)
    channelDraft.value[key] = next
    showToast('渠道开关已保存')
    await loadPayChannels()
  } catch (e) {
    showToast(String(e), true)
  }
}

async function saveCredential(field: string) {
  const value = credDraft.value[field]
  if (!value || !value.trim()) {
    showToast('凭据内容为空', true)
    return
  }
  try {
    await api.setPaymentCredential(credChannel.value, field, value)
    credDraft.value[field] = ''
    credSaved.value[field] = true
    showToast('凭据已写入系统凭据库')
    await loadPaymentSettings()
  } catch (e) {
    showToast(String(e), true)
  }
}

async function removeCredential(field: string) {
  try {
    await api.deletePaymentCredential(credChannel.value, field)
    credSaved.value[field] = false
    showToast('凭据已删除')
    await loadPaymentSettings()
  } catch (e) {
    showToast(String(e), true)
  }
}

const currentCredFields = computed(() => PAY_CRED_FIELDS[credChannel.value])

// ===== 渠道账单对账 =====
const reconDate = ref(today())
const reconText = ref('')
const reconSummary = ref<{ matched: number; missing_remote: number; missing_local: number; amount_mismatch: number; status_mismatch: number } | null>(null)
const reconDiffs = ref<
  { recon_id: number; channel: string; diff_type: string; order_no: string | null; local_amount_cents: number; channel_amount_cents: number; detail: string | null; created_at: string }[]
>([])

async function importBill(channel: 'WECHAT' | 'ALIPAY') {
  if (!reconText.value.trim()) {
    showToast('请先粘贴账单文本', true)
    return
  }
  try {
    reconSummary.value =
      channel === 'WECHAT'
        ? await api.importWechatBill(reconDate.value, reconText.value)
        : await api.importAlipayBill(reconDate.value, reconText.value)
    reconText.value = ''
    await loadReconDiffs()
    showToast('账单已导入并完成对账')
  } catch (e) {
    showToast(String(e), true)
  }
}

async function loadReconDiffs() {
  try {
    reconDiffs.value = await api.listReconcileDiffs(reconDate.value)
  } catch (e) {
    showToast(String(e), true)
  }
}
async function doRetryPrint(orderNo: string) {
  try {
    showToast(await api.retryPrint(orderNo))
    await loadPrintQueue()
  } catch (e) {
    showToast(String(e), true)
  }
}

// ===== 订单作废 / 退货 =====
const opOrderNo = ref('')
const opItems = ref<OrderItemRow[]>([])
const opQty = ref<Record<number, string>>({})
const opMode = ref<'void' | 'refund'>('refund')

async function loadOrderItems() {
  const no = opOrderNo.value.trim()
  if (!no) return
  try {
    opItems.value = await api.getOrderItems(no)
  } catch (e) {
    showToast(String(e))
  }
}

async function doVoid() {
  try {
    const nonce = await requireSensitiveAuth('pos.void', '作废授权')
    await api.voidOrder(opOrderNo.value.trim(), nonce)
    showToast('订单已作废')
    opItems.value = []
    opOrderNo.value = ''
  } catch (e) {
    if (String(e) !== '已取消二次授权') showToast(String(e), true)
  }
}

async function doRefund() {
  const items = opItems.value
    .filter((r) => {
      const q = parseInt(opQty.value[r.item_id] || '0', 10)
      return q > 0 && q <= r.qty - r.returned_qty
    })
    .map((r) => ({ order_item_id: r.item_id, qty: parseInt(opQty.value[r.item_id], 10) }))
  if (!items.length) {
    showToast('请填写有效的退货数量', true)
    return
  }
  try {
    const nonce = await requireSensitiveAuth('pos.refund', '退货授权')
    const total = await api.refundOrder(opOrderNo.value.trim(), items, nonce)
    showToast(`退货成功，退款 ${formatMoney(total)}`)
    opItems.value = []
    opOrderNo.value = ''
  } catch (e) {
    if (String(e) !== '已取消二次授权') showToast(String(e), true)
  }
}

// ===== 员工管理 =====
interface UserRowUI { user_id: number; username: string; role_name: string; status: number }
const users = ref<UserRowUI[]>([])
const uForm = ref({ username: '', password: '', roleName: 'CASHIER' })

async function loadUsers() {
  try {
    users.value = await api.listUsers()
  } catch (e) {
    showToast(String(e), true)
  }
}

async function addUser() {
  const username = uForm.value.username.trim()
  const password = uForm.value.password
  if (!username) {
    showToast('请输入员工用户名', true)
    return
  }
  if (password.length < 8 || !/[a-zA-Z]/.test(password) || !/\d/.test(password)) {
    showToast('密码至少 8 位，且同时包含字母与数字', true)
    return
  }
  try {
    await api.createUser(username, password, uForm.value.roleName)
    uForm.value = { username: '', password: '', roleName: 'CASHIER' }
    showToast('员工账号已创建')
    await loadUsers()
  } catch (e) {
    showToast(String(e), true)
  }
}

async function toggleUser(u: UserRowUI) {
  try {
    await api.setUserStatus(u.user_id, u.status === 1 ? 0 : 1)
    await loadUsers()
  } catch (e) {
    showToast(String(e))
  }
}

function applyTheme(t: 'light' | 'dark') {
  theme.value = t
  document.documentElement.dataset.theme = t
  localStorage.setItem('checkoutr-theme', t)
}
/** 收银台弹窗打开时，快捷键不得穿透到底层交易。 */
function posDialogOpen(): boolean {
  return (
    lockSetupOpen.value ||
    memberDialogOpen.value ||
    couponDialogOpen.value ||
    cashDialogOpen.value ||
    parkDialogOpen.value ||
    !!authDialog.value ||
    !!nativeDialog.value
  )
}

/** 收银快捷键：对齐主流 POS 的单手操作习惯（F2 扫码、F3 会员、Ctrl+G/Q 挂单取单）。 */
function handlePosShortcut(e: KeyboardEvent): boolean {
  if (view.value !== 'pos' || e.repeat) return false
  const key = e.key.toLowerCase()
  if (e.key === 'F2' && !posDialogOpen()) {
    e.preventDefault()
    focusScan()
    return true
  }
  if (e.key === 'F3' && !posDialogOpen()) {
    e.preventDefault()
    openMemberDialog()
    return true
  }
  if (e.ctrlKey && !e.altKey && key === 'g' && !posDialogOpen()) {
    e.preventDefault()
    void park()
    return true
  }
  if (e.ctrlKey && !e.altKey && key === 'q' && !posDialogOpen()) {
    e.preventDefault()
    void openParkDialog()
    return true
  }
  const tag = (e.target as HTMLElement | null)?.tagName
  const editing = tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT'
  if (e.ctrlKey && !e.altKey && key === 'z' && !editing && !posDialogOpen()) {
    e.preventDefault()
    clearCart()
    return true
  }
  return false
}

/** Esc 统一关闭最上层弹窗：收银场景键盘高频，必须能不摸鼠标退出。
 *  授权 / Native 收款弹窗走各自 cancel* 函数，确保挂起的 Promise 与轮询定时器一并清理。 */
function onGlobalKeydown(e: KeyboardEvent) {
  if (handlePosShortcut(e)) return
  if (e.key !== 'Escape') return
  if (lockSetupOpen.value) { cancelLockSetup(); return }
  if (memberDialogOpen.value) { memberDialogOpen.value = false; return }
  if (couponDialogOpen.value) { couponDialogOpen.value = false; return }
  if (cashDialogOpen.value) { cashDialogOpen.value = false; return }
  if (parkDialogOpen.value) { parkDialogOpen.value = false; return }
  if (authDialog.value) { cancelSensitiveAuth(); return }
  if (nativeDialog.value) { cancelNative(); return }
}
onMounted(() => {
  const saved = localStorage.getItem('checkoutr-theme') as 'light' | 'dark' | null
  if (saved) applyTheme(saved)
  window.addEventListener('keydown', onGlobalKeydown)
})
onUnmounted(() => window.removeEventListener('keydown', onGlobalKeydown))
function goPOS() { view.value = 'pos'; focusScan(); loadPayChannels() }
function goProducts() { goodsTab.value = 'list'; view.value = 'products' }
function goReport() {
  view.value = 'report'
  reportTool.value = null
  void loadReportCatalog().then(() => runActiveReport())
}

/** 付款区「挂单」按钮：打开弹窗并刷新待取挂单列表 */
async function openParkDialog() {
  parkDialogOpen.value = true
  await loadParked()
}
/** 付款区「会员」按钮：打开识别弹窗 */
function openMemberDialog() {
  memberFindError.value = ''
  memberDialogOpen.value = true
}
/** 弹窗内识别会员：成功则关闭弹窗，失败留在弹窗内展示内联错误 */
async function confirmMemberDialog() {
  if (await findMember()) memberDialogOpen.value = false
}
/** 会员/券弹窗打开即聚焦输入框，键盘与扫码枪操作员无需额外点选 */
const memberDialogInput = ref<HTMLInputElement | null>(null)
const couponDialogInput = ref<HTMLInputElement | null>(null)
watch(memberDialogOpen, (open) => { if (open) void nextTick(() => memberDialogInput.value?.focus()) })
watch(couponDialogOpen, (open) => { if (open) void nextTick(() => couponDialogInput.value?.focus()) })
function goSettings() { view.value = 'settings'; loadUsers(); loadConfigs(); loadPrintQueue(); loadPaymentSettings(); loadIndustries(); loadStoreProfile() }
async function doLogout() {
  // 先等后端会话注销：失败会留在当前页并提示，避免“界面已退、后端仍登录”。
  try {
    await api.logout()
  } catch (e) {
    showToast(`退出失败：${String(e)}`)
    return
  }
  view.value = 'login'
  currentUser.value = ''
  currentRole.value = 'CASHIER'
  settingsTab.value = 'overview'
}
</script>

<template>
  <div class="app-root">
    <div v-if="bootError" class="fatal">启动失败：{{ bootError }}</div>

    <!-- 启动中 -->
    <div v-else-if="view === 'boot'" class="center-screen">
      <div class="spinner"></div>
      <p>正在初始化系统…</p>
    </div>

    <!-- 行业选择与门店设置（首次启动第 3 步） -->
    <div v-else-if="view === 'industry'" class="center-screen">
      <div class="card auth-card industry-card">
        <SetupSteps v-if="onboarding" :step="authStep" />
        <div class="brand">
          <div class="brand-mark"><Icon name="scan" :size="ICON_SIZE.xxl" /></div>
          <div>
            <h1>Checkout-R</h1>
            <p class="muted">选择经营行业，收银台和商品分类会按行业切换</p>
          </div>
        </div>
        <h2>选择经营行业</h2>
        <div class="industry-list">
          <button
            v-for="ind in industries"
            :key="ind.id"
            :class="['industry-item', { active: pendingIndustry === ind.id }]"
            :aria-pressed="pendingIndustry === ind.id"
            @click="pendingIndustry = ind.id"
          >
            <strong>{{ ind.name }}</strong>
            <span class="muted small">{{ ind.description }}</span>
            <span class="muted small">默认单位：{{ ind.default_unit }}</span>
          </button>
        </div>
        <label class="field-label">
          店名
          <input v-model="storeNameDraft" class="input" placeholder="如 城南生活超市" maxlength="30" />
        </label>
        <button class="btn primary btn-lg" @click="finishIndustrySetup">
          {{ needsActivation ? '下一步：激活设备' : '进入收银' }}
        </button>
        <p class="muted small">行业分类可在设置页随时调整，切换只新增分类不删除已有商品。</p>
      </div>
    </div>

    <!-- 激活（首次启动第 4 步）：一个激活码输入框 + 试用入口 -->
    <div v-else-if="view === 'activate'" class="center-screen">
      <div class="card auth-card">
        <SetupSteps v-if="onboarding" :step="authStep" />
        <div class="brand">
          <div class="brand-mark"><Icon name="scan" :size="ICON_SIZE.xxl" /></div>
          <div>
            <h1>Checkout-R</h1>
            <p class="muted">收银系统授权</p>
          </div>
        </div>
        <h2>激活设备</h2>
        <p v-if="storeProfile?.industry_name" class="activate-context">
          当前行业：<strong>{{ storeProfile.industry_name }}</strong>
        </p>
        <label class="field-label">
          激活码
          <div class="activate-code-row">
            <input
              v-model="activateToken"
              class="input mono"
              placeholder="输入激活码后回车激活"
              autocapitalize="characters"
              @keyup.enter="submitActivateOnline"
            />
            <button class="btn primary" @click="submitActivateOnline">激活</button>
          </div>
        </label>
        <p v-if="activateError" class="error">{{ activateError }}</p>
        <div class="activate-actions">
          <button class="btn" @click="enterTrial">试用15天</button>
          <button class="btn" @click="openOfficialSite">免费获取激活码</button>
        </div>
        <button class="link setup-back" @click="view = 'industry'">返回上一步，重新选择行业</button>
      </div>
    </div>

    <!-- 登录：首次显示「设置管理员账号」，建号后回到登录页 -->
    <div v-else-if="view === 'login'" class="center-screen">
      <div class="card auth-card">
        <SetupSteps v-if="onboarding" :step="authStep" />
        <div class="auth-brand">
          <div class="brand-mark"><Icon name="scan" :size="ICON_SIZE.xxl" /></div>
          <h1>{{ needAdmin ? '设置管理员账号' : onboarding ? '登录' : '欢迎回来' }}</h1>
          <p class="muted">
            {{ needAdmin ? '首次使用，先创建管理员账号，再登录完成初始化' : onboarding ? '使用刚创建的管理员账号登录，继续选择行业' : '输入账号密码开始收银' }}
          </p>
        </div>
        <template v-if="needAdmin">
          <label class="field">
            <span class="field-label">用户名</span>
            <input v-model="adminUser" class="input input-lg" placeholder="默认 admin" @keyup.enter="submitAdmin" />
          </label>
          <label class="field">
            <span class="field-label">密码</span>
            <input v-model="adminPass" type="password" class="input input-lg" placeholder="至少 8 位，含字母和数字" @keyup.enter="submitAdmin" />
          </label>
          <p v-if="loginError" class="form-error"><Icon name="alert" :size="ICON_SIZE.sm" />{{ loginError }}</p>
          <button class="btn primary btn-lg" @click="submitAdmin">创建管理员账号</button>
        </template>
        <template v-else>
          <label class="field">
            <span class="field-label">用户名</span>
            <input v-model="loginUser" class="input input-lg" placeholder="用户名" @keyup.enter="submitLogin" />
          </label>
          <label class="field">
            <span class="field-label">密码</span>
            <input v-model="loginPass" type="password" class="input input-lg" placeholder="密码" @keyup.enter="submitLogin" />
          </label>
          <p v-if="loginError" class="form-error"><Icon name="alert" :size="ICON_SIZE.sm" />{{ loginError }}</p>
          <button class="btn primary btn-lg" @click="submitLogin">登录</button>
        </template>
      </div>
    </div>

    <!-- 主界面 -->
    <div v-else class="shell">
      <header class="topbar">
        <div class="logo">
          <Icon name="scan" :size="ICON_SIZE.xl" />
          <span>{{ storeProfile?.store_name || 'Checkout-R' }}</span>
          <span v-if="storeProfile?.industry_name" class="badge">{{ storeProfile.industry_name }}</span>
        </div>
        <nav class="nav">
          <button :class="['nav-btn', { active: view === 'pos' }]" @click="goPOS"><Icon name="cart" :size="ICON_SIZE.lg" />收银</button>
          <template v-if="atLeast('MANAGER')">
            <button :class="['nav-btn', { active: view === 'products' || view === 'categories' }]" @click="goProducts"><Icon name="package" :size="ICON_SIZE.lg" />商品</button>
            <button :class="['nav-btn', { active: view === 'supply' }]" @click="view = 'supply'"><Icon name="clipboard" :size="ICON_SIZE.lg" />采购盘点</button>
            <button :class="['nav-btn', { active: view === 'member' }]" @click="view = 'member'"><Icon name="heart" :size="ICON_SIZE.lg" />会员营销</button>
            <button :class="['nav-btn', { active: view === 'refund' }]" @click="gotoRefund()"><Icon name="undo" :size="ICON_SIZE.lg" />退款</button>
            <button :class="['nav-btn', { active: view === 'order' }]" @click="gotoOrder(orderNo)"><Icon name="receipt" :size="ICON_SIZE.lg" />订单</button>
            <button :class="['nav-btn', { active: view === 'report' }]" @click="goReport"><Icon name="report" :size="ICON_SIZE.lg" />报表</button>
            <button :class="['nav-btn', { active: view === 'settings' }]" @click="goSettings"><Icon name="settings" :size="ICON_SIZE.lg" />设置</button>
          </template>
        </nav>
        <div class="user">
          <Icon name="user" :size="ICON_SIZE.md" />
          <span>{{ currentUser }}</span>
          <span class="role-tag">{{ currentRole === 'ADMIN' ? '管理员' : currentRole === 'MANAGER' ? '店长' : '店员' }}</span>
          <button class="link" @click="lockScreen"><Icon name="lock" :size="ICON_SIZE.sm" />锁机</button>
          <button class="link" @click="doLogout"><Icon name="logout" :size="ICON_SIZE.sm" />退出</button>
        </div>
      </header>

      <div v-if="banner" class="banner">{{ banner }}</div>

      <!-- 收银 -->
      <main v-if="view === 'pos'" class="pos-layout">
        <section class="panel product-panel">
          <RestaurantScene
            v-if="industry === 'RESTAURANT'"
            :active-table-id="activeTable?.table_id ?? null"
            :cart="cart"
            :member-id="memberId"
            :can-manage="atLeast('MANAGER')"
            @select="selectSceneTable"
            @cleared="clearActiveTable"
            @error="showSceneError"
          />
          <KitchenRoutePanel
            v-if="industry === 'RESTAURANT'"
            :can-manage="atLeast('MANAGER')"
          />
          <ChessScene
            v-else-if="industry === 'BOARD_GAME'"
            :active-table-id="activeTable?.table_id ?? null"
            :cart="cart"
            :member-id="memberId"
            :can-manage="atLeast('MANAGER')"
            @select="selectSceneTable"
            @cleared="clearActiveTable"
            @error="showSceneError"
          />
          <BabyScene
            v-else-if="industry === 'BABY'"
            @add="addToCart"
            @error="showSceneError"
          />
          <WellnessScene
            v-else-if="industry === 'WELLNESS'"
            @add="addToCart"
            @error="showSceneError"
          />
          <TechnicianPanel
            v-if="industry === 'WELLNESS'"
            :can-manage="atLeast('MANAGER')"
            @changed="loadTechnicians"
          />
          <div class="search-row scan-search">
            <input
              ref="scanCapture"
              v-model="keyword"
              class="input"
              placeholder="扫码 / 条码 / 商品名，自动联想"
              @keydown="onScanKeydown"
              @focus="focusScan"
            />
            <span v-if="searchBusy" class="scan-spinner" aria-label="搜索中"></span>
            <span v-else-if="products.length" class="scan-hint">{{ products.length }} 个候选 · 回车收第一条</span>
          </div>
          <div v-if="keyword.trim()" class="product-list">
            <div v-for="p in products" :key="p.sku_id" class="product-item" @click="addToCart(p)">
              <img v-if="productImageSrc(p)" :src="productImageSrc(p)!" class="product-thumb" alt="" />
              <span v-else class="product-thumb product-thumb-empty"></span>
              <div class="product-info">
                <strong>{{ p.spu_name }}</strong>
                <span class="muted small"> {{ p.barcode }}</span>
              </div>
              <div class="price-stack">
                <span class="price">¥{{ formatMoney(p.price_cents) }}</span>
                <span v-if="memberPriceOf(p)" class="price member">会员 ¥{{ formatMoney(memberPriceOf(p)!) }}</span>
              </div>
            </div>
            <div v-if="!products.length" class="empty">
              {{ searchBusy ? '正在查询…' : '没有匹配的商品' }}
            </div>
          </div>
          <!-- 能扫能点：分类页签 + 商品卡片，触摸屏不用键盘也能点单 -->
          <div v-else class="pos-catalog">
            <div v-if="posRootCategories.length" class="pos-cat-row">
              <button
                v-for="c in posRootCategories"
                :key="c.category_id"
                :class="['pos-cat', { active: posCategoryId === c.category_id }]"
                @click="selectPosCategory(c.category_id)"
              >{{ c.name }}</button>
            </div>
            <div class="pos-grid">
              <button
                v-for="p in posProducts"
                :key="p.sku_id"
                class="pos-card"
                :class="{ weighed: p.is_weighed }"
                @click="addFromCatalog(p)"
              >
                <img v-if="productImageSrc(p)" :src="productImageSrc(p)!" class="pos-card-img" alt="" />
                <span v-else class="pos-card-img pos-card-empty"><Icon name="package" :size="ICON_SIZE.md" /></span>
                <span class="pos-card-name">{{ p.spu_name }}</span>
                <span class="pos-card-meta">{{ p.category_name }}<template v-if="p.is_weighed"> · 称重</template></span>
                <strong class="price">¥{{ formatMoney(p.price_cents) }}<span class="muted small">/{{ p.unit }}</span></strong>
              </button>
              <div v-if="!posProducts.length" class="empty pos-empty">
                {{ posCatalogBusy ? '正在装载商品…' : '该分类暂无在售商品，可切分类或直接扫码' }}
              </div>
            </div>
            <div class="pos-shortcuts">
              <span>F2 扫码</span><span>F3 会员</span><span>Ctrl+G 挂单</span><span>Ctrl+Q 取单</span><span>Ctrl+Z 清空</span>
            </div>
          </div>
        </section>

        <section class="panel cart-panel">
          <h3>
            购物车 <span class="badge">{{ cart.length }}</span>
            <button
              v-if="activeTable"
              class="btn btn-sm ghost cart-order"
              :disabled="memberId == null"
              @click="rebindTableMember"
            >换绑会员</button>
            <button
              v-if="industry === 'RESTAURANT' && activeTable"
              class="btn btn-sm cart-order"
              @click="submitTableOrder"
            >下单厨打</button>
            <button v-if="cart.length" class="btn ghost btn-sm cart-clear" @click="clearCart">清空</button>
          </h3>
          <div class="cart-list">
            <div v-for="(l, i) in cart" :key="`${l.sku_id}-${i}`" class="cart-line">
              <img v-if="productImageSrc(l)" :src="productImageSrc(l)!" class="cart-thumb" alt="" />
              <span v-else class="cart-thumb cart-thumb-empty"></span>
              <div class="line-name">
                {{ l.name || `商品 ${l.sku_id}` }}
                <span v-if="l.member_price_cents" class="tag">会员价</span>
                <span v-if="tableSentQty[l.sku_id]" class="tag sent-tag">已下 {{ tableSentQty[l.sku_id] }}</span>
                <button
                  v-if="industry === 'RESTAURANT' && activeTable && (tableSentQty[l.sku_id] || 0) > 0"
                  class="link danger return-link"
                  @click="returnTableLine(l)"
                >退菜</button>
                <select
                  v-if="industry === 'WELLNESS'"
                  v-model.number="l.technician_id"
                  class="input input-sm tech-select"
                  aria-label="指定技师"
                >
                  <option :value="null">不指定技师</option>
                  <option v-for="t in onDutyTechnicians" :key="t.technician_id" :value="t.technician_id">
                    {{ t.name }} · {{ (t.commission_bps / 100).toFixed(0) }}%
                  </option>
                </select>
              </div>
              <div v-if="l.is_chess_fee" class="qty-ctrl chess-fee-qty">
                <span>{{ l.chess_minutes }} 分钟</span>
              </div>
              <div v-else class="qty-ctrl">
                <button @click="changeQty(i, -1)" aria-label="减少数量"><Icon name="minus" :size="ICON_SIZE.sm" /></button>
                <input
                  class="qty-input"
                  type="number"
                  min="1"
                  :value="l.qty"
                  aria-label="商品数量"
                  @change="setLineQty(i, ($event.target as HTMLInputElement).value)"
                />
                <button @click="changeQty(i, 1)" aria-label="增加数量"><Icon name="plus" :size="ICON_SIZE.sm" /></button>
              </div>
              <span class="price">¥{{ formatMoney(l.qty * effectivePrice(l)) }}</span>
            </div>
            <div v-if="!cart.length" class="empty">购物车为空</div>
          </div>
          <div class="cart-foot">
            <div class="pay-quick-row">
              <button class="btn pay-quick" :class="{ active: memberId != null }" @click="openMemberDialog">
                <Icon name="user" :size="ICON_SIZE.md" />{{ memberId != null ? '会员（已识别）' : '会员' }}
              </button>
              <button
                class="btn pay-quick"
                :class="{
                  active: !!couponCode && !couponRecoverable,
                  pending: couponRecoverable,
                }"
                @click="openCouponDialog"
              >
                <Icon name="ticket" :size="ICON_SIZE.md" />{{ couponCode ? `券 ${couponCode}` : '优惠券' }}
              </button>
              <button v-if="couponCode" class="btn ghost pay-quick" @click="clearCoupon"><Icon name="close" :size="ICON_SIZE.sm" />清除券</button>
            </div>
            <span v-if="couponHint" class="small" :class="couponRecoverable ? 'warning-text' : 'muted'">{{ couponHint }}</span>
            <span v-if="memberBalances" class="member-balance-inline">
              本金 ¥{{ formatMoney(memberBalances.principal_cents) }} · 赠金 ¥{{ formatMoney(memberBalances.bonus_cents) }} · 积分 {{ memberBalances.points }}
            </span>
            <div v-if="memberId != null" class="points-earn-inline">
              <Icon name="star" :size="ICON_SIZE.sm" />
              <span v-if="(quote?.points_earned ?? 0) > 0">本单预计累计 <strong>+{{ quote?.points_earned }}</strong> 积分</span>
              <span v-else>本单无积分累计（未达整元或积分功能已关闭）</span>
            </div>
            <!-- 空车 = 空闲态：不显示任何金额与支付方式（对齐 Square / Shopify POS 空闲屏模式） -->
            <template v-if="cart.length">
              <div class="cart-total">
                <div class="cart-total-copy">
                  <span>应收</span>
                  <span v-if="quote && quote.discount_cents > 0" class="cart-total-meta">
                    原价 ¥{{ formatMoney(quote.total_cents) }} · 优惠 ¥{{ formatMoney(quote.discount_cents) }}
                  </span>
                </div>
                <strong>¥{{ formatMoney(payableAmount) }}</strong>
              </div>
              <!-- 支付方式紧凑单行：金额自动=应收，选中后只补渠道必需信息 -->
              <div class="pay-methods">
              <button
                v-for="m in payMethods"
                :key="m.id"
                :class="['pay-method-btn', { active: payMode === 'simple' && payChannel === m.id }]"
                :aria-pressed="payMode === 'simple' && payChannel === m.id"
                @click="selectPayChannel(m.id)"
              >
                <Icon :name="m.icon" :size="ICON_SIZE.md" />{{ m.label }}
              </button>
              <button
                :class="['pay-method-btn', { active: payMode === 'mixed' }]"
                :aria-pressed="payMode === 'mixed'"
                @click="enterMixedMode"
              >
                <Icon name="plus" :size="ICON_SIZE.md" />组合
              </button>
            </div>
            <!-- 微信 / 支付宝：只露付款码输入，金额已自动带出 -->
            <div v-if="payMode === 'simple' && (payChannel === 'WECHAT' || payChannel === 'ALIPAY')" class="pay-panel">
              <template v-if="!nativeMode">
                <input
                  ref="payCodeInput"
                  v-model="payAuthCode"
                  class="input input-lg"
                  placeholder="扫描顾客付款码，回车即收款"
                  @keyup.enter="submitCheckout"
                />
                <p class="muted small">扫码枪扫顾客付款码即完成收款，金额已按应收自动带出。</p>
                <button class="btn ghost" @click="nativeMode = true"><Icon name="qr" :size="ICON_SIZE.sm" />改为顾客扫屏幕二维码</button>
              </template>
              <template v-else>
                <p class="pay-hint">
                  <Icon name="qr" :size="ICON_SIZE.md" />
                  点击「收款」后将展示二维码，顾客用{{ payChannel === 'WECHAT' ? '微信' : '支付宝' }}扫码支付 ¥{{ formatMoney(payableAmount) }}。
                </p>
                <button class="btn ghost" @click="nativeMode = false"><Icon name="undo" :size="ICON_SIZE.sm" />改回扫描顾客付款码</button>
              </template>
            </div>
            <!-- 现金：点击「现金」按钮弹出收银弹窗（实收 / 找零在弹窗内完成） -->
            <!-- 储值：需先识别会员，金额自动带出 -->
            <div v-else-if="payMode === 'simple' && payChannel === 'STORED_VALUE'" class="pay-panel">
              <p v-if="memberId == null" class="pay-hint warn"><Icon name="alert" :size="ICON_SIZE.md" />储值支付需先点击「会员」识别会员。</p>
              <p v-else class="pay-hint ok"><Icon name="wallet" :size="ICON_SIZE.md" />将从会员储值余额扣除 ¥{{ formatMoney(payShortfall > 0 ? payShortfall : 0) }}。</p>
            </div>
            <!-- 组合支付：渠道与金额同排，付款码/实收等补充项按需展开 -->
            <div v-else-if="payMode === 'mixed'" class="pay-splits">
              <div v-for="(s, i) in paySplits" :key="i" class="pay-split">
                <select v-model="s.channel" class="input split-channel" aria-label="支付渠道">
                  <option v-if="channelEnabled.CASH !== false" value="CASH">现金</option>
                  <option v-if="channelEnabled.STORED_VALUE !== false" value="STORED_VALUE">储值</option>
                  <option v-if="channelEnabled.WECHAT !== false" value="WECHAT">微信</option>
                  <option v-if="channelEnabled.ALIPAY !== false" value="ALIPAY">支付宝</option>
                </select>
                <input
                  v-model="s.amountYuan"
                  class="input split-amount"
                  placeholder="金额(元)"
                  aria-label="该支付方式金额"
                  type="number"
                  step="0.01"
                />
                <button
                  v-if="paySplits.length > 1"
                  class="btn ghost split-remove"
                  aria-label="移除该支付方式"
                  @click="removePaySplit(i)"
                ><Icon name="close" :size="ICON_SIZE.sm" /></button>
                <input
                  v-if="(s.channel === 'WECHAT' || s.channel === 'ALIPAY') && !nativeMode"
                  v-model="s.authCode"
                  class="input split-auth"
                  aria-label="顾客付款码"
                  placeholder="扫描或输入顾客付款码"
                />
              </div>
              <div class="pay-split-tools">
                <button class="link add-split" @click="addPaySplit">+ 添加支付方式</button>
                <label class="checkbox-label">
                  <input v-model="nativeMode" type="checkbox" />
                  顾客扫屏幕二维码支付
                </label>
              </div>
              <input
                v-if="paySplits.some((s) => s.channel === 'CASH')"
                v-model="cashTendered"
                class="input"
                placeholder="现金实收(元)"
                aria-label="现金实收"
                type="number"
              />
              <div class="pay-mixed-status">
                <span class="pay-shortfall" :class="{ ok: payShortfall <= 0 }">
                  待收 ¥{{ formatMoney(payShortfall > 0 ? payShortfall : 0) }}
                </span>
                <button class="link" @click="selectPayChannel('CASH')"><Icon name="undo" :size="ICON_SIZE.sm" />返回单笔支付</button>
              </div>
            </div>
            <div class="actions checkout-actions">
              <button class="btn" @click="openParkDialog"><Icon name="pause" :size="ICON_SIZE.md" />挂单</button>
              <button class="btn primary" :disabled="paying" @click="submitCheckout">
                <span v-if="paying" class="btn-spinner" aria-label="处理中"></span>
                <Icon v-else name="check" :size="ICON_SIZE.md" />{{ paying ? '处理中…' : '收款' }}
              </button>
            </div>
            </template>
            <!-- 空车引导：金额与支付选择只在有商品后出现 -->
            <div v-else class="cart-idle">
              <Icon name="scan" :size="ICON_SIZE.lg" />
              <span>扫码或选择商品后开始收款</span>
            </div>
            <button v-if="lastOrder" class="btn ghost full" @click="printReceipt"><Icon name="print" :size="ICON_SIZE.md" />重打小票 {{ lastOrder }}</button>
          </div>
        </section>
      </main>

      <!-- 商品与分类：页签与内容同处统一管理骨架，左对齐 -->
      <main v-else-if="view === 'products' || view === 'categories'" class="main-simple">
        <div class="page-head">
          <h2>{{ goodsTab === 'category' ? '分类管理' : '商品管理' }}</h2>
          <div class="seg" role="tablist">
            <button :class="['seg-btn', { active: goodsTab === 'list' }]" role="tab" :aria-selected="goodsTab === 'list'" @click="goodsTab = 'list'; view = 'products'">
              <Icon name="package" :size="ICON_SIZE.md" />商品列表
            </button>
            <button :class="['seg-btn', { active: goodsTab === 'category' }]" role="tab" :aria-selected="goodsTab === 'category'" @click="goodsTab = 'category'; view = 'categories'">
              <Icon name="box" :size="ICON_SIZE.md" />分类管理
            </button>
          </div>
        </div>
        <ProductAdmin v-if="goodsTab === 'list'" />
        <CategoryAdmin v-else />
      </main>

      <!-- 采购与盘点 -->
      <SupplyAdmin v-else-if="view === 'supply'" :request-auth="requireSensitiveAuth" />

      <!-- 会员营销 -->
      <MemberAdmin v-else-if="view === 'member'" />

      <!-- 报表 -->
      <main v-else-if="view === 'report'" class="admin-shell">
        <!-- 报表中心：后台管理式布局，左栏贴左全高（34 报表 + 工具），右侧内容独立滚动 -->
        <aside class="admin-side">
          <div class="admin-side-title">报表中心</div>
          <div v-for="g in reportGroups" :key="g.name" class="report-nav-group">
            <div class="report-nav-title">{{ g.name }}</div>
            <button
              v-for="m in g.items"
              :key="m.id"
              :class="['report-nav-item', { active: !reportTool && activeReportId === m.id }]"
              @click="selectReport(m.id)"
            >{{ m.title }}</button>
          </div>
          <div class="report-nav-group">
            <div class="report-nav-title">工具</div>
            <button :class="['report-nav-item', { active: reportTool === 'shift' }]" @click="openTool('shift')">班次管理</button>
            <button :class="['report-nav-item', { active: reportTool === 'orderOp' }]" @click="openTool('orderOp')">订单作废 / 退货</button>
          </div>
        </aside>
        <section class="admin-main">
          <!-- 报表查询 -->
          <template v-if="!reportTool">
            <div class="toolbar report-toolbar">
              <input v-model="centerFrom" class="input" type="date" />
              <span class="muted small">至</span>
              <input v-model="centerTo" class="input" type="date" />
              <button class="btn primary" :disabled="reportLoading" @click="runActiveReport">
                {{ reportLoading ? '查询中…' : '查询' }}
              </button>
              <button class="btn" :disabled="!activeReport" @click="exportActiveReport">导出CSV</button>
            </div>
            <template v-if="activeReport">
              <div class="report-head">
                <h3>{{ activeReport.title }}</h3>
                <p class="muted small">{{ activeReport.description }}</p>
              </div>
              <div v-if="activeReport.metrics.length" class="stats">
                <div v-for="(mt, i) in activeReport.metrics" :key="i" class="stat">
                  <span>{{ mt.label }}</span><strong>{{ mt.value }}</strong>
                </div>
              </div>
              <div class="report-table-wrap">
                <table v-if="activeReport.rows.length" class="tbl">
                  <thead>
                    <tr><th v-for="(c, i) in activeReport.columns" :key="i" :class="{ 'num-cell': c.kind === 'money' || c.kind === 'qty' || c.kind === 'percent' }">{{ c.label }}</th></tr>
                  </thead>
                  <tbody>
                    <tr v-for="(row, ri) in activeReport.rows" :key="ri">
                      <td v-for="(cell, ci) in row" :key="ci" :class="{ 'num-cell': activeReport.columns[ci]?.kind === 'money' || activeReport.columns[ci]?.kind === 'qty' || activeReport.columns[ci]?.kind === 'percent' }">{{ cell }}</td>
                    </tr>
                  </tbody>
                </table>
                <div v-else class="empty">该区间暂无数据</div>
              </div>
            </template>
            <div v-else class="empty">请选择左侧报表</div>
          </template>
          <!-- 工具：班次管理 -->
          <template v-else-if="reportTool === 'shift'">
            <div class="report-head">
              <h3>班次管理</h3>
              <p class="muted small">开班设置备用金，关班需授权并核对现金差额。</p>
            </div>
            <div class="toolbar">
              <input v-model="openingFloat" class="input" type="number" placeholder="开班备用金(元)" />
              <button class="btn" @click="doOpenShift">开班</button>
              <span v-if="currentShiftId" class="badge">当前班次 {{ currentShiftId }}</span>
            </div>
            <div class="toolbar">
              <input v-model="actualCash" class="input" type="number" placeholder="关班实收现金(元)" />
              <button class="btn" @click="doCloseShift">关班（需授权）</button>
            </div>
            <div class="toolbar">
              <input v-model.number="shiftId" class="input" type="number" placeholder="班次 ID" />
              <button class="btn" @click="loadShift">查询汇总</button>
            </div>
            <table v-if="shiftRows.length" class="tbl">
              <thead><tr><th>收银员</th><th class="num-cell">订单数</th><th class="num-cell">应收</th><th class="num-cell">实收</th><th class="num-cell">现金净额</th></tr></thead>
              <tbody>
                <tr v-for="r in shiftRows" :key="r.cashier_id">
                  <td>{{ r.cashier_id }}</td>
                  <td class="num-cell">{{ r.order_count }}</td>
                  <td class="num-cell">{{ formatMoney(r.total_cents) }}</td>
                  <td class="num-cell">{{ formatMoney(r.pay_cents) }}</td>
                  <td class="num-cell">{{ formatMoney(r.cash_net_cents) }}</td>
                </tr>
              </tbody>
            </table>
            <div v-else class="empty">输入班次 ID 查询交班汇总</div>
          </template>
          <!-- 工具：订单作废 / 退货 -->
          <template v-else>
            <div class="report-head">
              <h3>订单作废 / 退货</h3>
              <p class="muted small">作废：仅待支付单或当日现金单；扫码已支付订单只能退货退款。退货受角色限额约束。</p>
            </div>
            <div class="toolbar">
              <input v-model="opOrderNo" class="input" placeholder="输入订单号" />
              <button class="btn" @click="loadOrderItems">查询明细</button>
            </div>
            <table v-if="opItems.length" class="tbl">
              <thead><tr><th>商品</th><th class="num-cell">数量</th><th class="num-cell">已退</th><th class="num-cell">单价</th><th class="num-cell">本次退货数</th></tr></thead>
              <tbody>
                <tr v-for="r in opItems" :key="r.item_id">
                  <td>{{ r.product_name }}</td>
                  <td class="num-cell">{{ r.qty }}</td>
                  <td class="num-cell">{{ r.returned_qty }}</td>
                  <td class="num-cell">{{ formatMoney(r.unit_price_cents) }}</td>
                  <td class="num-cell"><input v-model="opQty[r.item_id]" class="input input-sm" type="number" min="0" :max="r.qty - r.returned_qty" /></td>
                </tr>
              </tbody>
            </table>
            <div v-if="opItems.length" class="op-form">
              <button class="btn" @click="doVoid">作废订单</button>
              <button class="btn primary" @click="doRefund">退货退款</button>
            </div>
          </template>
        </section>
      </main>

      <!-- 退款 -->
      <RefundPanel
        v-else-if="view === 'refund'"
        :initial-order-no="refundOrderNo"
        :request-auth="requireSensitiveAuth"
        @detail="gotoOrder"
      />

      <!-- 订单详情 -->
      <OrderDetailPanel
        v-else-if="view === 'order'"
        :order-no="orderNo"
        @refund="gotoRefund"
        @search="gotoOrder"
      />

      <!-- 设置中心：后台管理式布局，左栏贴左全高，右侧内容独立滚动 -->
      <main v-else class="admin-shell">
        <nav class="admin-side">
          <div class="admin-side-title">设置中心</div>
          <button
            v-for="t in visibleSettingsTabs"
            :key="t.id"
            :class="['settings-nav-btn', { active: settingsTab === t.id }]"
            @click="settingsTab = t.id"
          >
            <Icon :name="t.icon" :size="ICON_SIZE.md" />{{ t.label }}
          </button>
        </nav>

        <div class="admin-main settings-content">
          <!-- 概览：门店信息 + 授权状态 -->
          <template v-if="settingsTab === 'overview'">
            <div class="card wide">
              <h3>门店信息</h3>
              <div class="form-grid">
                <label>店名
                  <input v-model="storeNameDraft" class="input" maxlength="30" placeholder="如 城南生活超市" />
                </label>
                <label>所属行业
                  <select v-model="pendingIndustry" class="input">
                    <option v-for="ind in industries" :key="ind.id" :value="ind.id">{{ ind.name }}</option>
                  </select>
                </label>
              </div>
              <div class="op-form">
                <button class="btn primary" @click="saveStoreName">保存店名</button>
                <button class="btn" @click="confirmIndustry">应用行业分类</button>
              </div>
              <p class="muted small">当前：{{ storeProfile?.industry_name || '未设置' }} · {{ storeProfile?.store_name || '未命名门店' }}。切换行业会新增模板分类并隐藏旧行业里没有商品的空分类；有商品的分类会保留。</p>
            </div>
            <div class="card wide">
              <h3>授权状态</h3>
              <div class="license-info">
                <p>授权状态：<strong>{{ licenseStatusLabel }}</strong></p>
                <p class="muted small">{{ license?.valid_until ? `有效期至 ${license.valid_until}` : '买断许可' }}</p>
                <p v-if="license?.trial_ends_at" class="muted small">试用期至 {{ license.trial_ends_at }}</p>
                <p class="muted small">授权状态以激活服务为准，可联网刷新。</p>
                <button class="btn" @click="refreshLicense"><Icon name="refresh" :size="ICON_SIZE.md" />刷新授权状态</button>
              </div>
            </div>
          </template>

          <!-- 外观 -->
          <div v-else-if="settingsTab === 'appearance'" class="card wide">
            <h3>外观主题</h3>
            <p class="muted small">主题切换入口仅在设置页，不占主界面。</p>
            <div class="theme-row">
              <button :class="['btn', { primary: theme === 'light' }]" @click="applyTheme('light')">浅色</button>
              <button :class="['btn', { primary: theme === 'dark' }]" @click="applyTheme('dark')">深色</button>
            </div>
          </div>

          <!-- 员工管理 -->
          <div v-else-if="settingsTab === 'staff'" class="card wide">
            <h3>员工管理</h3>
            <p class="muted small">仅管理员可见可用（user.manage 权限）。</p>
            <table class="tbl">
              <thead><tr><th>用户名</th><th>角色</th><th>状态</th><th>操作</th></tr></thead>
              <tbody>
                <tr v-for="u in users" :key="u.user_id">
                  <td>{{ u.username }}</td>
                  <td>{{ u.role_name }}</td>
                  <td>{{ u.status === 1 ? '启用' : '停用' }}</td>
                  <td><button class="btn btn-sm ghost" @click="toggleUser(u)">{{ u.status === 1 ? '停用' : '启用' }}</button></td>
                </tr>
              </tbody>
            </table>
            <div class="op-form">
              <input v-model="uForm.username" class="input" placeholder="用户名" />
              <input v-model="uForm.password" class="input" type="password" placeholder="密码（≥8位含字母数字）" />
              <select v-model="uForm.roleName" class="input">
                <option value="CASHIER">收银员</option>
                <option value="MANAGER">店长</option>
                <option value="ADMIN">管理员</option>
              </select>
              <button class="btn" @click="addUser">创建账号</button>
            </div>
          </div>

          <!-- 系统配置：按功能分区，同组同卡片；支付渠道归 payment 页、积分归 points 页，不重复 -->
          <template v-else-if="settingsTab === 'system'">
            <div v-for="g in systemConfigGroups" :key="g.title" class="card wide">
              <h3>{{ g.title }}</h3>
              <p class="muted small">{{ g.hint }}</p>
              <div v-for="c in g.items" :key="c.config_key" class="config-row config-row-stacked">
                <div class="config-label">
                  <strong>{{ c.label }}</strong>
                  <span class="muted small">{{ c.description }}</span>
                </div>
                <div class="config-control">
                  <Switch
                    v-if="c.kind === 'switch'"
                    :model-value="configDraft[c.config_key] === '1'"
                    :label="c.label"
                    @update:model-value="toggleConfigSwitch(c.config_key)"
                  />
                  <select
                    v-else-if="c.kind === 'select'"
                    v-model="configDraft[c.config_key]"
                    class="input"
                  >
                    <option v-for="o in configOptions(c.config_key)" :key="o.value" :value="o.value">
                      {{ o.label }}
                    </option>
                  </select>
                  <input
                    v-else
                    v-model="configDraft[c.config_key]"
                    class="input"
                    :type="c.kind === 'text' ? 'text' : 'number'"
                    :step="c.kind === 'decimal' ? '0.01' : '1'"
                  />
                  <button v-if="c.kind !== 'switch'" class="btn btn-sm" @click="saveConfig(c.config_key)">保存</button>
                </div>
              </div>
            </div>
            <div class="card wide">
              <h3>安全</h3>
              <p class="muted small">离岗锁屏、关闭窗口隐藏到托盘后需输入此码解锁（仅存本机）。</p>
              <div class="config-row config-row-stacked">
                <div class="config-label">
                  <strong>锁机码</strong>
                  <span class="muted small">设置后本机生效，不上传服务器；仅保存摘要，但本机文件仍可被读取，不能替代系统账户权限</span>
                </div>
                <div class="config-control">
                  <input
                    v-model="lockCodeDraft"
                    class="input"
                    type="password"
                    :placeholder="lockCode ? '已设置，输入新值覆盖' : '输入锁机码'"
                  />
                  <button class="btn btn-sm primary" @click="saveLockCode">保存</button>
                  <button v-if="lockCode" class="btn btn-sm ghost" @click="clearLockCode">清除</button>
                </div>
              </div>
            </div>
          </template>

          <!-- 支付设置 -->
          <div v-else-if="settingsTab === 'payment'" class="card wide">
            <h3>支付设置</h3>
            <div class="search-row">
              <span class="config-key">网关模式</span>
              <span class="muted small">微信：{{ gatewayModes.wechat === 'REAL' ? '真实渠道' : '本地确认（未配置凭据）' }}；支付宝：{{ gatewayModes.alipay === 'REAL' ? '真实渠道' : '本地确认（未配置凭据）' }}。保存凭据后需重启应用生效。</span>
            </div>
            <h4>支付渠道开关</h4>
            <p class="muted small">收银台可用的支付方式；关闭后该渠道在收款区隐藏。</p>
            <div class="config-row" v-for="(v, k) in channelDraft" :key="k">
              <span class="config-key">{{ configLabel(k) }}</span>
              <Switch :model-value="v === '1'" :label="configLabel(k)" @update:model-value="toggleChannel(k, v)" />
            </div>
            <h4>商户凭据（存 Windows 凭据库，不落库不回显）</h4>
            <div class="search-row">
              <button :class="['btn', { primary: credChannel === 'WECHAT' }]" @click="credChannel = 'WECHAT'">微信支付</button>
              <button :class="['btn', { primary: credChannel === 'ALIPAY' }]" @click="credChannel = 'ALIPAY'">支付宝</button>
            </div>
            <div class="form-grid">
              <label v-for="f in currentCredFields" :key="f.key">
                {{ f.label }}
                <textarea v-if="f.multiline" v-model="credDraft[f.key]" class="input" rows="3" :placeholder="credSaved[f.key] ? '已配置（写入新值将覆盖）' : '粘贴 PEM 或密钥'"></textarea>
                <input v-else v-model="credDraft[f.key]" class="input" type="password" :placeholder="credSaved[f.key] ? '已配置（写入新值将覆盖）' : '输入后保存'" />
                <span class="cred-actions">
                  <button class="btn btn-sm primary" @click="saveCredential(f.key)">保存</button>
                  <button class="btn btn-sm ghost" @click="removeCredential(f.key)">删除</button>
                </span>
              </label>
            </div>
          </div>

          <!-- 积分配置 -->
          <div v-else-if="settingsTab === 'points'" class="card wide">
            <h3>积分配置</h3>
            <p class="muted small">识别会员收款后自动累计积分：按实付整元数 × 每元赠送积分计算，赠送金抵扣部分不计分；退款时按原单积分扣回。修改后对新订单生效。</p>
            <div v-for="c in pointsConfigs" :key="c.config_key" class="config-row config-row-stacked">
              <div class="config-label">
                <strong>{{ c.label }}</strong>
                <span class="muted small">{{ c.description }}</span>
              </div>
              <div class="config-control">
                <Switch
                  v-if="c.kind === 'switch'"
                  :model-value="configDraft[c.config_key] === '1'"
                  :label="c.label"
                  @update:model-value="toggleConfigSwitch(c.config_key)"
                />
                <input
                  v-else
                  v-model="configDraft[c.config_key]"
                  class="input"
                  type="number"
                  step="1"
                  min="0"
                />
                <button v-if="c.kind !== 'switch'" class="btn btn-sm" @click="saveConfig(c.config_key)">保存</button>
              </div>
            </div>
            <div class="points-summary">
              <Icon name="star" :size="ICON_SIZE.md" />
              <span>当前规则：每消费 1 元累计 <strong>{{ configDraft['points_per_yuan'] ?? '1' }}</strong> 积分；{{ configDraft['points_enabled'] === '0' ? '积分累计已关闭' : '积分累计已开启' }}。</span>
            </div>
          </div>

          <!-- 渠道对账 -->
          <div v-else-if="settingsTab === 'recon'" class="card wide">
            <h3>渠道账单对账</h3>
            <div class="search-row">
              <input v-model="reconDate" class="input" type="date" />
              <button class="btn" @click="loadReconDiffs">查询差异</button>
            </div>
            <p class="muted small">微信粘贴交易账单 CSV；支付宝粘贴对账单 CSV。差异只告警冻结，绝不自动抹平。</p>
            <textarea v-model="reconText" class="input mono" rows="5" placeholder="粘贴渠道账单文本"></textarea>
            <div class="op-form">
              <button class="btn" @click="importBill('WECHAT')">导入微信账单</button>
              <button class="btn" @click="importBill('ALIPAY')">导入支付宝账单</button>
            </div>
            <div v-if="reconSummary" class="stats">
              <div class="stat"><span>匹配</span><strong>{{ reconSummary.matched }}</strong></div>
              <div class="stat"><span>渠道缺失</span><strong>{{ reconSummary.missing_remote }}</strong></div>
              <div class="stat"><span>本地缺失</span><strong>{{ reconSummary.missing_local }}</strong></div>
              <div class="stat"><span>金额不符</span><strong>{{ reconSummary.amount_mismatch }}</strong></div>
              <div class="stat"><span>状态不符</span><strong>{{ reconSummary.status_mismatch }}</strong></div>
            </div>
            <table v-if="reconDiffs.length" class="tbl">
              <thead><tr><th>渠道</th><th>差异类型</th><th>订单号</th><th class="num-cell">本地</th><th class="num-cell">渠道</th><th>说明</th></tr></thead>
              <tbody>
                <tr v-for="d in reconDiffs" :key="d.recon_id">
                  <td>{{ d.channel }}</td>
                  <td>{{ d.diff_type }}</td>
                  <td class="mono">{{ d.order_no || '—' }}</td>
                  <td class="num-cell">{{ formatMoney(d.local_amount_cents) }}</td>
                  <td class="num-cell">{{ formatMoney(d.channel_amount_cents) }}</td>
                  <td class="muted small">{{ d.detail }}</td>
                </tr>
              </tbody>
            </table>
          </div>

          <!-- 打印队列 -->
          <div v-else-if="settingsTab === 'print'" class="card wide">
            <h3>打印重试队列</h3>
            <div v-if="printQueue.length" class="print-queue">
              <div v-for="no in printQueue" :key="no" class="print-queue-row">
                <span>{{ no }}</span>
                <button class="btn btn-sm" @click="doRetryPrint(no)">重打</button>
              </div>
            </div>
            <p v-else class="muted small">无待重打订单</p>
          </div>
        </div>
      </main>

      <!-- 二次授权弹窗（§12.4） -->
      <div v-if="authDialog" class="modal-mask">
        <div class="modal" role="dialog" aria-modal="true" aria-label="二次授权">
          <h3>{{ authDialog?.title }}</h3>
          <p class="muted small">此项操作需店长或管理员二次授权</p>
          <input v-model="authUser" class="input" placeholder="授权人用户名" />
          <input v-model="authPass" class="input" type="password" placeholder="授权人密码" @keyup.enter="confirmSensitiveAuth" />
          <p v-if="authError" class="err-text">{{ authError }}</p>
          <div class="actions">
            <button class="btn" @click="cancelSensitiveAuth">取消</button>
            <button class="btn primary" @click="confirmSensitiveAuth">确认授权</button>
          </div>
        </div>
      </div>

      <!-- Native 支付弹窗（§6.3）：二维码 + 轮询 + 本地确认兜底 -->
      <div v-if="nativeDialog" class="modal-mask">
        <div class="modal native-modal" role="dialog" aria-modal="true" aria-label="顾客扫码支付">
          <h3>顾客扫码支付</h3>
          <p class="muted small">请顾客使用微信或支付宝扫一扫完成付款，收款后自动关闭</p>
          <div class="qr-box" v-html="nativeDialog.qrSvg"></div>
          <p class="mono small muted">单号 {{ nativeDialog.orderNo }}</p>
          <div class="actions">
            <button class="btn" @click="cancelNative">取消支付</button>
            <button v-if="isLocalGateway" class="btn primary" @click="confirmNativeLocal">本地确认收款</button>
          </div>
        </div>
      </div>

      <!-- 挂单弹窗：付款区「挂单」按钮唤起 -->
      <div v-if="parkDialogOpen" class="modal-mask" @click.self="parkDialogOpen = false">
        <div class="modal" role="dialog" aria-modal="true" aria-label="待取挂单">
          <h3>待取挂单</h3>
          <p class="muted small">点击「取单」将挂单商品放回购物车，促销与会员价现场重算。</p>
          <div class="park-dialog-list">
            <div v-for="p in parked" :key="String((p as any).park_id)" class="row-item">
              <span>挂单 {{ (p as any).park_id }} · {{ (p as any).created_at }}</span>
              <button class="btn" @click="resume((p as any).park_id)">取单</button>
            </div>
            <div v-if="!parked.length" class="empty">暂无挂单</div>
          </div>
          <div class="actions">
            <button class="btn" @click="park">挂起当前购物车</button>
            <button class="btn primary" @click="parkDialogOpen = false">关闭</button>
          </div>
        </div>
      </div>

      <!-- 会员识别弹窗：付款区「会员」按钮唤起 -->
      <!-- 优惠券弹窗：点击付款区「优惠券」图标按钮唤起 -->
      <div v-if="couponDialogOpen" class="modal-mask" @click.self="couponDialogOpen = false">
        <div class="modal" role="dialog" aria-modal="true" aria-label="使用优惠券">
          <h3>使用优惠券</h3>
          <p class="muted small">输入优惠券码，应用后按券规则计算优惠；不使用可清除。</p>
          <input
            ref="couponDialogInput"
            v-model="couponDraft"
            class="input"
            placeholder="优惠券码"
            @keyup.enter="applyCouponDialog"
          />
          <div class="actions">
            <button class="btn" @click="couponDialogOpen = false">取消</button>
            <button v-if="couponCode" class="btn" @click="clearCoupon(); couponDialogOpen = false">清除已用券</button>
            <button class="btn primary" @click="applyCouponDialog">应用</button>
          </div>
        </div>
      </div>

      <!-- 现金收银弹窗：应收 / 实收 / 找零 三栏 + 快捷面额 + 数字小键盘 -->
      <div v-if="cashDialogOpen" class="modal-mask" @click.self="cashDialogOpen = false">
        <div class="modal cash-modal" role="dialog" aria-modal="true" aria-label="现金收款">
          <h3>现金收款</h3>
          <div class="cash-summary">
            <div class="cash-cell">
              <span>应收</span>
              <strong>¥{{ formatMoney(payableAmount) }}</strong>
            </div>
            <div class="cash-cell tendered">
              <span>实收</span>
              <div class="cash-amount">
                <span>¥</span>
                <input
                  ref="cashDialogInput"
                  :value="cashTendered"
                  inputmode="decimal"
                  autocomplete="off"
                  aria-label="实收现金"
                  placeholder="0.00"
                  @input="onCashInput"
                  @keyup.enter="submitCheckout"
                />
              </div>
            </div>
            <div class="cash-cell change" :class="{ ok: cashOverpaid, short: cashShort > 0 }" aria-live="polite">
              <span>找零</span>
              <strong>¥{{ formatMoney(changeDue) }}</strong>
              <small v-if="cashShort > 0" class="cash-short">还差 ¥{{ formatMoney(cashShort) }}</small>
            </div>
          </div>
          <div class="quick-cash">
            <button
              v-for="v in QUICK_CASH_YUAN"
              :key="v"
              class="btn ghost quick-cash-btn"
              :disabled="v * CENTS_PER_YUAN < payableAmount"
              :title="v * CENTS_PER_YUAN < payableAmount ? `小于应收 ¥${formatMoney(payableAmount)}` : `实收 ¥${v}`"
              @click="cashTendered = String(v)"
            >¥{{ v }}</button>
            <button class="btn ghost quick-cash-btn exact" @click="exactCash">刚好</button>
          </div>
          <div class="cash-keypad" role="group" aria-label="现金数字键盘">
            <button
              v-for="k in CASH_KEYPAD"
              :key="k"
              class="cash-key"
              :class="{ muted: k === '退格' }"
              @click="pressCashKey(k)"
            >{{ k }}</button>
          </div>
          <div class="actions">
            <button class="btn" @click="cashDialogOpen = false">取消</button>
            <button class="btn primary" :disabled="paying || cashShort > 0" @click="submitCheckout">
              <span v-if="paying" class="btn-spinner" aria-label="处理中"></span>
              <Icon v-else name="check" :size="ICON_SIZE.md" />{{ paying ? '处理中…' : '确认收款' }}
            </button>
          </div>
        </div>
      </div>

      <div v-if="memberDialogOpen" class="modal-mask" @click.self="memberDialogOpen = false">
        <div class="modal" role="dialog" aria-modal="true" aria-label="识别会员">
          <h3>识别会员</h3>
          <p class="muted small">输入会员手机号或会员码，识别后按会员价结算；本单收款完成后自动清除。</p>
          <input
            ref="memberDialogInput"
            v-model="memberPhone"
            class="input"
            placeholder="会员手机号/会员码"
            @keyup.enter="confirmMemberDialog"
          />
          <p v-if="memberFindError" class="form-error" role="alert"><Icon name="alert" :size="ICON_SIZE.sm" />{{ memberFindError }}</p>
          <div v-if="memberBalances" class="member-balance">
            本金 ¥{{ formatMoney(memberBalances.principal_cents) }} · 赠金 ¥{{ formatMoney(memberBalances.bonus_cents) }} · 积分 {{ memberBalances.points }}
          </div>
          <p class="muted small">识别后本单按会员价结算，并在收款成功后自动累计积分（赠送金抵扣部分不计分）。</p>
          <div class="actions">
            <button class="btn" @click="memberDialogOpen = false">关闭</button>
            <button class="btn primary" @click="confirmMemberDialog">识别会员</button>
          </div>
        </div>
      </div>

      <!-- 锁机码锁屏：离岗锁屏或隐藏到托盘时覆盖整个界面 -->
      <div v-if="locked" class="lock-overlay">
        <div class="lock-card">
          <div class="brand-mark"><Icon name="lock" :size="ICON_SIZE.xxl" /></div>
          <h2>收银台已锁定</h2>
          <p class="muted small">收银员临时离岗，请输入锁机码解锁</p>
          <input
            v-model="lockInput"
            class="input mono lock-input"
            type="password"
            placeholder="输入锁机码"
            @keyup.enter="unlock"
          />
          <p v-if="lockError" class="error">{{ lockError }}</p>
          <button class="btn primary" @click="unlock">解锁</button>
        </div>
      </div>

      <!-- 锁机码设置：首次锁机尚未设码时由「锁机」按钮唤起 -->
      <div v-if="lockSetupOpen" class="modal-mask" @click.self="cancelLockSetup">
        <div class="modal" role="dialog" aria-modal="true" aria-label="设置锁机码">
          <h3>设置锁机码</h3>
          <p class="muted small">离岗锁屏、或关闭窗口隐藏到托盘后，输入此码解锁。摘要仅保存在本机，清空浏览器数据会一并清除。</p>
          <input
            v-model="lockCodeDraft"
            class="input mono"
            type="password"
            placeholder="输入锁机码"
            @keyup.enter="saveLockCode"
          />
          <div class="actions">
            <button class="btn" @click="cancelLockSetup">取消</button>
            <button class="btn primary" @click="saveLockCode">保存并锁屏</button>
          </div>
        </div>
      </div>

      <div v-if="toast" class="toast" :class="{ err: toastKind === 'error' }" role="status" :aria-live="toastKind === 'error' ? 'assertive' : 'polite'">{{ toast }}</div>
    </div>
  </div>
</template>
