<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import {
  api,
  formatMoney,
  today,
  type LicenseSnapshot,
  type ProductHit,
  type CartLine,
  type OrderItemRow,
  type ChannelReport,
  type ProductSales,
  type CategorySales,
  type CashierSales,
  type ConfigRow,
  type OrderQuote,
  type StoreProfile,
  type IndustryOption,
} from './api'
import Icon from './components/Icon.vue'
import ProductAdmin from './components/ProductAdmin.vue'
import CategoryAdmin from './components/CategoryAdmin.vue'
import SupplyAdmin from './components/SupplyAdmin.vue'
import MemberAdmin from './components/MemberAdmin.vue'
import OrderDetailPanel from './components/OrderDetailPanel.vue'
import RefundPanel from './components/RefundPanel.vue'
import Switch from './components/Switch.vue'
import {
  ICON_SIZE,
  MS_PER_DAY,
  TOAST_DURATION_MS,
  ERROR_DURATION_MS,
  SCAN_GAP_MS_DEFAULT,
  SCAN_MIN_LENGTH,
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
const bootError = ref('')
const license = ref<LicenseSnapshot | null>(null)
const needAdmin = ref(false)
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

// 设置中心子页：左侧导航 + 右侧内容，避免九张卡片堆一屏
type SettingsTab = 'overview' | 'appearance' | 'staff' | 'system' | 'payment' | 'recon' | 'coupon' | 'print'
type SettingsIcon = 'box' | 'settings' | 'users' | 'cash' | 'report' | 'print'
const settingsTab = ref<SettingsTab>('overview')
const settingsTabs: Array<{ id: SettingsTab; label: string; icon: SettingsIcon }> = [
  { id: 'overview', label: '门店概览', icon: 'box' },
  { id: 'appearance', label: '外观主题', icon: 'settings' },
  { id: 'staff', label: '员工管理', icon: 'users' },
  { id: 'system', label: '系统配置', icon: 'settings' },
  { id: 'payment', label: '支付设置', icon: 'cash' },
  { id: 'recon', label: '渠道对账', icon: 'report' },
  { id: 'coupon', label: '优惠券', icon: 'cash' },
  { id: 'print', label: '打印队列', icon: 'print' },
]

// ===== 门店档案与行业（多行业适配）=====
const storeProfile = ref<StoreProfile | null>(null)
const industries = ref<IndustryOption[]>([])
const pendingIndustry = ref('')
const storeNameDraft = ref('')
/** 订单详情页当前单号 */
const orderNo = ref('')
/** 退款页预置单号（从订单详情跳转时带入） */
const refundOrderNo = ref('')

// 收银状态
const keyword = ref('')
const products = ref<ProductHit[]>([])
const cart = ref<(CartLine & { name: string })[]>([])
const cashTendered = ref('')
const lastOrder = ref('')
const toast = ref('')
const parked = ref<unknown[]>([])

// 会员识别 / 优惠券 / 混合支付（§6.5 / §13）
const memberPhone = ref('')
const memberId = ref<number | null>(null)
const memberBalances = ref<{ principal_cents: number; bonus_cents: number; points: number } | null>(null)
const couponCode = ref('')
// 混合支付拆分：每项 { channel, amountYuan, authCode }
interface PaySplit { channel: string; amountYuan: string; authCode: string }
const paySplits = ref<PaySplit[]>([{ channel: 'CASH', amountYuan: '', authCode: '' }])
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
  setTimeout(() => (toast.value = ''), isError ? ERROR_DURATION_MS : TOAST_DURATION_MS)
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
async function findMember() {
  const phone = memberPhone.value.trim()
  if (!phone) {
    memberId.value = null
    memberBalances.value = null
    for (const l of cart.value) l.member_price_cents = null
    return
  }
  try {
    const list = await api.listMembers(phone, 10)
    const m = list.find((x) => x.phone === phone || x.member_code === phone)
    if (!m) {
      memberId.value = null
      memberBalances.value = null
      for (const l of cart.value) l.member_price_cents = null
      showToast('未找到会员', true)
      return
    }
    memberId.value = m.member_id
    memberBalances.value = {
      principal_cents: m.principal_cents,
      bonus_cents: m.bonus_cents,
      points: m.points,
    }
    await refreshCartMemberPrices()
    showToast(`会员已识别：${m.name ?? m.member_code}`)
  } catch (e) {
    showToast(String(e), true)
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
  // 顾客付款码优先（微信 18–19 位 / 支付宝 26–30 位纯数字）：
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
  await search()
}

/** 扫到顾客付款码：落到对应渠道的支付行，金额取剩余待收，直接发起收款。 */
async function applyPaymentCode(channel: string, code: string) {
  // 先取后端权威应收，避免按前端估算金额收款
  await refreshQuote()
  const amount = payShortfall.value
  if (amount <= 0) return
  let idx = paySplits.value.findIndex((s) => s.channel === channel)
  if (idx < 0) {
    const only = paySplits.value.length === 1 && !paySplits.value[0].amountYuan
    if (only) {
      idx = 0
    } else {
      paySplits.value.push({ channel, amountYuan: '', authCode: '' })
      idx = paySplits.value.length - 1
    }
  }
  const split = paySplits.value[idx]
  split.channel = channel
  split.amountYuan = (amount / CENTS_PER_YUAN).toFixed(2)
  split.authCode = code
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
    // 行业列表供「设置 → 门店概览」使用，不参与首启流程
    await loadIndustries()
    // 首启流程固定为：登录（首次即设置账号和密码）→ 激活 → 收银。
    // 行业与店名设置已归入「设置 → 门店概览」，不再作为首启拦截。
    needAdmin.value = !(await api.hasUsers())
    if (needAdmin.value) {
      view.value = 'login'
    } else if (license.value.status === 'UNACTIVATED') {
      view.value = 'activate'
      // 进入激活页即探测服务可达性，让用户第一时间知道能不能联网激活
      void probeActivationService()
    } else {
      view.value = 'login'
    }
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
    pendingIndustry.value = pendingIndustry.value || industries.value[0]?.id || ''
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
    if (storeProfile.value?.configured) {
      // 与启动流程一致：先确保有管理员账号，再判断激活状态
      needAdmin.value = !(await api.hasUsers())
      if (needAdmin.value) {
        view.value = 'login'
      } else if (license.value?.status === 'UNACTIVATED') {
        view.value = 'activate'
        void probeActivationService()
      } else {
        view.value = 'login'
      }
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

/** 选择试用15天：接受当前试用期，直接进入收银（试用期由后端在首次启动时开启） */
function enterTrial() {
  if (license.value?.status !== 'TRIAL_ACTIVE') {
    showToast('试用期不可用，请使用激活码激活', true)
    return
  }
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
      // 账号在激活前已创建，激活成功直接进入收银
      needAdmin.value = !(await api.hasUsers())
      view.value = needAdmin.value ? 'login' : 'pos'
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
      // 账号在激活前已创建，激活成功直接进入收银
      needAdmin.value = !(await api.hasUsers())
      view.value = needAdmin.value ? 'login' : 'pos'
      showToast('激活成功')
    }
  } catch (e) {
    activateError.value = String(e)
  }
}

async function submitAdmin() {
  loginError.value = ''
  try {
    await api.initAdmin(adminUser.value, adminPass.value)
    await api.login(adminUser.value, adminPass.value)
    currentUser.value = adminUser.value
    needAdmin.value = false
    // 首启固定三步：登录（建号）→ 激活 → 收银。
    // 不按授权状态跳过：首次启动处于 TRIAL_ACTIVE 试用态，若据此判断会漏掉激活页。
    view.value = 'activate'
    void probeActivationService()
    showToast('管理员已创建，请先激活设备')
  } catch (e) {
    loginError.value = String(e)
  }
}

async function submitLogin() {
  loginError.value = ''
  try {
    await api.login(loginUser.value, loginPass.value)
    currentUser.value = loginUser.value
    view.value = 'pos'
  } catch (e) {
    loginError.value = String(e)
  }
}

async function search() {
  if (!keyword.value.trim()) {
    products.value = []
    return
  }
  try {
    products.value = await api.lookup(keyword.value.trim())
  } catch (e) {
    showToast(String(e))
  }
}

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
    })
  keyword.value = ''
  products.value = []
}

function changeQty(index: number, delta: number) {
  const line = cart.value[index]
  line.qty += delta
  if (line.qty <= 0) cart.value.splice(index, 1)
}

async function submitCheckout() {
  if (!cart.value.length) {
    showToast('购物车为空', true)
    return
  }
  try {
    const payments = paySplits.value
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
    if (!payments.length) {
      showToast('请填写支付金额', true)
      return
    }
    const r = await api.checkout(
      cart.value,
      payments,
      memberId.value,
      couponCode.value.trim() || null
    )
    lastOrder.value = r.order_no
    if (r.status === 'PENDING_PAYMENT' && r.code_url) {
      // Native：展示二维码并轮询（§6.3）
      const svg = await api.qrSvg(r.code_url)
      nativeDialog.value = { orderNo: r.order_no, codeUrl: r.code_url, qrSvg: svg }
      startNativePolling(r.order_no)
      return
    }
    cart.value = []
    cashTendered.value = ''
    couponCode.value = ''
    paySplits.value = [{ channel: 'CASH', amountYuan: '', authCode: '' }]
    showToast(`收银成功：${r.order_no}`)
  } catch (e) {
    showToast(String(e), true)
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
        cart.value = []
        cashTendered.value = ''
        couponCode.value = ''
        paySplits.value = [{ channel: 'CASH', amountYuan: '', authCode: '' }]
        showToast(`收银成功：${orderNo}`)
      } else if (r.status === 'CLOSED' || r.status.startsWith('FAILED')) {
        stopNativePolling()
        closeNativeDialog()
        showToast(`Native 支付未完成：${r.status}`, true)
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
    await api.confirmNativePay(d.orderNo)
    closeNativeDialog()
    cart.value = []
    cashTendered.value = ''
    couponCode.value = ''
    paySplits.value = [{ channel: 'CASH', amountYuan: '', authCode: '' }]
    showToast(`收银成功：${d.orderNo}`)
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

/** 报价刷新：购物车/会员/券变化后向后端取权威应收金额。 */
async function refreshQuote() {
  if (!cart.value.length) {
    quote.value = null
    return
  }
  try {
    quote.value = await api.quoteOrder(
      cart.value,
      memberId.value,
      couponCode.value.trim() || null
    )
  } catch {
    quote.value = null
  }
}

watch(
  [() => cart.value, memberId, couponCode],
  () => {
    void refreshQuote()
  },
  { deep: true }
)

async function park() {
  try {
    await api.parkCart(JSON.stringify(cart.value), null)
    cart.value = []
    showToast('已挂单')
  } catch (e) {
    showToast(String(e))
  }
}

async function loadParked() {
  try {
    parked.value = await api.listParked()
    view.value = 'parked'
  } catch (e) {
    showToast(String(e))
  }
}

async function resume(parkId: number) {
  try {
    const json = await api.resumeCart(parkId)
    cart.value = JSON.parse(json)
    view.value = 'pos'
    showToast('已取单，促销将现场重算')
  } catch (e) {
    showToast(String(e))
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

const reportDate = ref(today())
const report = ref<{ order_count: number; total_sales_cents: number; refund_cents: number; discount_cents: number } | null>(null)
async function loadReport() {
  try {
    report.value = await api.dailyReport(reportDate.value)
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

// ===== 报表维度（§14）=====
const channelReport = ref<ChannelReport | null>(null)
const productRows = ref<ProductSales[]>([])
const categoryRows = ref<CategorySales[]>([])
const cashierRows = ref<CashierSales[]>([])
const reportFrom = ref(today())
const reportTo = ref(today())
async function loadReports() {
  try {
    ;[channelReport.value, productRows.value, categoryRows.value, cashierRows.value] = await Promise.all([
      api.channelReport(reportDate.value),
      api.productReport(reportFrom.value, reportTo.value),
      api.categoryReport(reportFrom.value, reportTo.value),
      api.cashierReport(reportFrom.value, reportTo.value),
    ])
  } catch (e) {
    showToast(String(e), true)
  }
}
async function exportReconcile() {
  try {
    const csv = await api.reconcileCsv(reportDate.value)
    const blob = new Blob(['\ufeff' + csv], { type: 'text/csv;charset=utf-8' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `reconcile-${reportDate.value}.csv`
    a.click()
    URL.revokeObjectURL(url)
    showToast('对账差异已导出')
  } catch (e) {
    showToast(String(e), true)
  }
}

// ===== 系统配置（§15/§14）=====
const configs = ref<ConfigRow[]>([])
const configDraft = ref<Record<string, string>>({})
/** 仅渲染后端标注为可编辑的配置项；门店名称与所属行业由「门店信息」卡片管理。 */
const editableConfigs = computed(() => configs.value.filter((c) => c.editable))
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
async function saveConfig(key: string) {
  try {
    await api.setConfig(key, configDraft.value[key] ?? '')
    showToast('配置已保存')
    await loadConfigs()
  } catch (e) {
    showToast(String(e), true)
  }
}

/** 开关型配置：点击即翻转并保存（'1' 启用 / '0' 停用）。 */
async function toggleConfigSwitch(key: string) {
  const next = configDraft.value[key] === '1' ? '0' : '1'
  configDraft.value[key] = next
  await saveConfig(key)
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

// ===== 渠道账单对账（PAY-05/06）=====
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

// ===== 优惠券管理（§13.7）=====
const coupons = ref<import('./api').CouponRow[]>([])
const couponForm = ref({ discountYuan: '', minOrderYuan: '', validUntil: '' })
async function loadCoupons() {
  try {
    coupons.value = await api.listCoupons()
  } catch (e) {
    showToast(String(e), true)
  }
}
async function addCoupon() {
  try {
    await api.issueCoupon(
      memberId.value,
      Math.round(Number(couponForm.value.discountYuan || '0') * CENTS_PER_YUAN),
      Math.round(Number(couponForm.value.minOrderYuan || '0') * CENTS_PER_YUAN),
      couponForm.value.validUntil.trim() || null
    )
    couponForm.value = { discountYuan: '', minOrderYuan: '', validUntil: '' }
    showToast('优惠券已发放')
    await loadCoupons()
  } catch (e) {
    showToast(String(e), true)
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
    showToast(String(e))
  }
}

async function addUser() {
  try {
    await api.createUser(uForm.value.username.trim(), uForm.value.password, uForm.value.roleName)
    uForm.value = { username: '', password: '', roleName: 'CASHIER' }
    showToast('员工账号已创建')
    await loadUsers()
  } catch (e) {
    showToast(String(e))
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
onMounted(() => {
  const saved = localStorage.getItem('checkoutr-theme') as 'light' | 'dark' | null
  if (saved) applyTheme(saved)
})
function goPOS() { view.value = 'pos'; focusScan(); loadPayChannels() }
function goProducts() { view.value = 'products' }
function goParked() { loadParked() }
function goReport() { view.value = 'report'; loadReport(); loadReports(); loadShift() }
function goSettings() { view.value = 'settings'; loadUsers(); loadConfigs(); loadPrintQueue(); loadCoupons(); loadPaymentSettings(); loadIndustries(); loadStoreProfile() }
function doLogout() { api.logout(); view.value = 'login'; currentUser.value = '' }
</script>

<template>
  <div class="app-root">
    <div v-if="bootError" class="fatal">启动失败：{{ bootError }}</div>

    <!-- 启动中 -->
    <div v-else-if="view === 'boot'" class="center-screen">
      <div class="spinner"></div>
      <p>正在初始化系统…</p>
    </div>

    <!-- 行业选择与门店设置（首次启动） -->
    <div v-else-if="view === 'industry'" class="center-screen">
      <div class="card auth-card industry-card">
        <div class="brand">
          <div class="brand-mark"><Icon name="scan" :size="ICON_SIZE.xxl" /></div>
          <div>
            <h1>Checkout-R</h1>
            <p class="muted">选择经营行业，系统据此预置商品分类</p>
          </div>
        </div>
        <h2>选择行业</h2>
        <div class="industry-list">
          <button
            v-for="ind in industries"
            :key="ind.id"
            :class="['industry-item', { active: pendingIndustry === ind.id }]"
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
        <div class="actions">
          <button class="btn" @click="loadIndustries">刷新行业列表</button>
          <button class="btn" @click="confirmIndustry">仅应用行业分类</button>
          <button class="btn primary" @click="finishIndustrySetup">完成设置并开始使用</button>
        </div>
        <p class="muted small">行业分类可在设置页随时调整，切换只新增分类不删除已有商品。</p>
      </div>
    </div>

    <!-- 激活 -->
    <div v-else-if="view === 'activate'" class="center-screen">
      <div class="card auth-card">
        <div class="brand">
          <div class="brand-mark"><Icon name="scan" :size="ICON_SIZE.xxl" /></div>
          <div>
            <h1>Checkout-R</h1>
            <p class="muted">收银系统授权</p>
          </div>
        </div>
        <h2>{{ renewMode ? '导入续期码' : '设备激活' }}</h2>

        <!-- 首启三选一之一：先试用 15 天。试用期由系统在首次启动时开启。 -->
        <div v-if="!renewMode && license?.status === 'TRIAL_ACTIVE'" class="trial-entry">
          <p class="muted small">可先免费试用，当前剩余 <strong>{{ trialDaysLeft }} 天</strong>，到期后需用激活码激活。</p>
          <button class="btn primary" @click="enterTrial">先试用 15 天，进入收银</button>
        </div>

        <template v-if="!renewMode && activateMode === 'online'">
          <p class="muted small">
            输入激活码后联网向激活服务换取授权，授权在本机验签后保存。
          </p>
          <label class="field-label">
            激活码
            <input
              v-model="activateToken"
              class="input mono"
              placeholder="CKR-XXXX-XXXX-XXXX-XXXX"
              autocapitalize="characters"
            />
          </label>
          <button class="btn" @click="openOfficialSite">
            <Icon name="box" :size="ICON_SIZE.md" />获取激活码
          </button>
          <label class="field-label">
            门店名称（选填，用于授权登记）
            <input v-model="activateCustomer" class="input" placeholder="如 城南生活超市" maxlength="30" />
          </label>
          <p v-if="activationServiceReachable === false" class="error">
            激活服务当前不可达：请检查网络，或在「设置 → 系统配置 → 在线激活服务」确认地址。
          </p>
          <p v-else-if="activationServiceReachable === true" class="muted small">激活服务连接正常。</p>
          <p v-if="activateError" class="error">{{ activateError }}</p>
          <button class="btn primary" @click="submitActivateOnline">联网激活</button>
          <button class="btn ghost" @click="activateMode = 'offline'">
            网络不可用？改用离线激活码
          </button>
        </template>

        <template v-else-if="!renewMode">
          <p class="muted small">
            离线备用通道：粘贴服务商提供的授权码（Base64URL.签名），本机验签后立即生效，全程不发起网络请求。
          </p>
          <textarea v-model="activateToken" rows="5" placeholder="粘贴激活码（Base64URL.签名）" class="input mono"></textarea>
          <p v-if="activateError" class="error">{{ activateError }}</p>
          <button class="btn primary" @click="submitActivate">离线激活</button>
          <button class="btn ghost" @click="activateMode = 'online'">返回在线激活</button>
        </template>

        <template v-else>
          <p class="muted small">粘贴续期码（Base64URL.签名），本机验签后立即生效。</p>
          <textarea v-model="activateToken" rows="5" placeholder="粘贴续期码（Base64URL.签名）" class="input mono"></textarea>
          <p v-if="activateError" class="error">{{ activateError }}</p>
          <button class="btn primary" @click="submitActivate">续期</button>
        </template>

        <button v-if="!renewMode" class="btn ghost" @click="renewMode = !renewMode">
          {{ renewMode ? '返回激活' : '已有授权，导入续期码' }}
        </button>
        <button v-else class="btn ghost" @click="renewMode = false">返回激活</button>
      </div>
    </div>

    <!-- 登录：首次仅显示「设置账号和密码」，之后仅显示登录表单 -->
    <div v-else-if="view === 'login'" class="center-screen">
      <div class="card auth-card">
        <div class="brand">
          <div class="brand-mark"><Icon name="user" :size="ICON_SIZE.xxl" /></div>
          <div>
            <h1>{{ needAdmin ? '设置账号和密码' : '登录' }}</h1>
            <p class="muted">{{ needAdmin ? '首次使用，请设置管理员账号和密码' : '输入账号密码开始收银' }}</p>
          </div>
        </div>
        <template v-if="needAdmin">
          <input v-model="adminUser" class="input" placeholder="用户名（默认 admin）" />
          <input v-model="adminPass" type="password" class="input" placeholder="密码（≥8位，含字母数字）" @keyup.enter="submitAdmin" />
          <p v-if="loginError" class="error">{{ loginError }}</p>
          <button class="btn primary" @click="submitAdmin">创建管理员并继续</button>
        </template>
        <template v-else>
          <input v-model="loginUser" class="input" placeholder="用户名" />
          <input v-model="loginPass" type="password" class="input" placeholder="密码" @keyup.enter="submitLogin" />
          <p v-if="loginError" class="error">{{ loginError }}</p>
          <button class="btn primary" @click="submitLogin">登录</button>
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
          <button :class="['nav-btn', { active: view === 'products' }]" @click="goProducts"><Icon name="box" :size="ICON_SIZE.lg" />商品</button>
          <button :class="['nav-btn', { active: view === 'categories' }]" @click="view = 'categories'"><Icon name="box" :size="ICON_SIZE.lg" />分类</button>
          <button :class="['nav-btn', { active: view === 'supply' }]" @click="view = 'supply'"><Icon name="park" :size="ICON_SIZE.lg" />采购盘点</button>
          <button :class="['nav-btn', { active: view === 'member' }]" @click="view = 'member'"><Icon name="users" :size="ICON_SIZE.lg" />会员营销</button>
          <button :class="['nav-btn', { active: view === 'parked' }]" @click="goParked"><Icon name="park" :size="ICON_SIZE.lg" />挂单</button>
          <button :class="['nav-btn', { active: view === 'refund' }]" @click="gotoRefund()"><Icon name="refresh" :size="ICON_SIZE.lg" />退款</button>
          <button :class="['nav-btn', { active: view === 'order' }]" @click="gotoOrder(orderNo)"><Icon name="print" :size="ICON_SIZE.lg" />订单</button>
          <button :class="['nav-btn', { active: view === 'report' }]" @click="goReport"><Icon name="report" :size="ICON_SIZE.lg" />报表</button>
          <button :class="['nav-btn', { active: view === 'settings' }]" @click="goSettings"><Icon name="settings" :size="ICON_SIZE.lg" />设置</button>
        </nav>
        <div class="user"><Icon name="user" :size="ICON_SIZE.md" /><span>{{ currentUser }}</span><button class="link" @click="doLogout">退出</button></div>
      </header>

      <div v-if="banner" class="banner">{{ banner }}</div>

      <!-- 收银 -->
      <main v-if="view === 'pos'" class="pos-layout">
        <section class="panel product-panel">
          <div class="search-row">
            <input
              ref="scanCapture"
              v-model="keyword"
              class="input"
              placeholder="扫码 / 条码 / 商品名"
              @keydown="onScanKeydown"
              @focus="focusScan"
            />
            <button class="btn" @click="search"><Icon name="scan" :size="ICON_SIZE.md" />识别</button>
          </div>
          <div class="product-list">
            <div v-for="p in products" :key="p.sku_id" class="product-item" @click="addToCart(p)">
              <div>
                <strong>{{ p.spu_name }}</strong>
                <span class="muted small"> {{ p.barcode }}</span>
              </div>
              <div class="price-stack">
                <span class="price">¥{{ formatMoney(p.price_cents) }}</span>
                <span v-if="memberPriceOf(p)" class="price member">会员 ¥{{ formatMoney(memberPriceOf(p)!) }}</span>
              </div>
            </div>
            <div v-if="!products.length" class="empty">扫码枪自动识别，或输入条码/商品名回车</div>
          </div>

          <div class="member-strip">
            <input v-model="memberPhone" class="input" placeholder="会员手机号/会员码" @keyup.enter="findMember" />
            <button class="btn" @click="findMember"><Icon name="user" :size="ICON_SIZE.sm" />识别会员</button>
          </div>
          <div v-if="memberBalances" class="member-balance">
            本金 ¥{{ formatMoney(memberBalances.principal_cents) }} · 赠金 ¥{{ formatMoney(memberBalances.bonus_cents) }} · 积分 {{ memberBalances.points }}
          </div>
        </section>

        <section class="panel cart-panel">
          <h3>购物车 <span class="badge">{{ cart.length }}</span></h3>
          <div class="cart-list">
            <div v-for="(l, i) in cart" :key="`${l.sku_id}-${i}`" class="cart-line">
              <div class="line-name">
                {{ l.name || `SKU ${l.sku_id}` }}
                <span v-if="l.member_price_cents" class="tag">会员价</span>
              </div>
              <div class="qty-ctrl">
                <button @click="changeQty(i, -1)"><Icon name="minus" :size="ICON_SIZE.sm" /></button>
                <span>{{ l.qty }}</span>
                <button @click="changeQty(i, 1)"><Icon name="plus" :size="ICON_SIZE.sm" /></button>
              </div>
              <span class="price">¥{{ formatMoney(l.qty * effectivePrice(l)) }}</span>
            </div>
            <div v-if="!cart.length" class="empty">购物车为空</div>
          </div>
          <div class="cart-foot">
            <div class="coupon-row">
              <input v-model="couponCode" class="input" placeholder="优惠券码（可选）" />
              <span v-if="quote?.coupon_note" class="muted small">{{ quote.coupon_note }}</span>
            </div>
            <div class="total">
              <span>应收</span>
              <strong>¥{{ formatMoney(payableAmount) }}</strong>
            </div>
            <div v-if="quote && quote.discount_cents > 0" class="muted small">
              原价 ¥{{ formatMoney(quote.total_cents) }} · 优惠 ¥{{ formatMoney(quote.discount_cents) }}
            </div>
            <div class="pay-splits">
              <div v-for="(s, i) in paySplits" :key="i" class="pay-split">
                <select v-model="s.channel" class="input">
                  <option v-if="channelEnabled.CASH !== false" value="CASH">现金</option>
                  <option v-if="channelEnabled.STORED_VALUE !== false" value="STORED_VALUE">储值</option>
                  <option v-if="channelEnabled.WECHAT !== false" value="WECHAT">微信</option>
                  <option v-if="channelEnabled.ALIPAY !== false" value="ALIPAY">支付宝</option>
                </select>
                <input v-model="s.amountYuan" class="input" placeholder="金额(元)" type="number" step="0.01" />
                <input
                  v-if="(s.channel === 'WECHAT' || s.channel === 'ALIPAY') && !nativeMode"
                  v-model="s.authCode"
                  class="input"
                  placeholder="顾客付款码"
                />
                <button v-if="paySplits.length > 1" class="btn ghost" @click="removePaySplit(i)">移除</button>
              </div>
              <label class="checkbox-label">
                <input v-model="nativeMode" type="checkbox" />
                Native 支付（顾客扫屏幕二维码，微信/支付宝）
              </label>
              <button class="btn ghost full" @click="addPaySplit">+ 添加支付方式</button>
              <input
                v-if="paySplits.some((s) => s.channel === 'CASH')"
                v-model="cashTendered"
                class="input"
                placeholder="现金收钞(元)"
                type="number"
              />
              <div class="pay-shortfall" :class="{ ok: payShortfall <= 0 }">
                待收 ¥{{ formatMoney(payShortfall > 0 ? payShortfall : 0) }}
              </div>
            </div>
            <div class="actions">
              <button class="btn" @click="park">挂单</button>
              <button class="btn primary" @click="submitCheckout"><Icon name="check" :size="ICON_SIZE.md" />收款</button>
            </div>
            <button v-if="lastOrder" class="btn ghost full" @click="printReceipt"><Icon name="print" :size="ICON_SIZE.md" />重打小票 {{ lastOrder }}</button>
          </div>
        </section>
      </main>

      <!-- 商品管理 -->
      <ProductAdmin v-else-if="view === 'products'" />

      <!-- 商品分类 -->
      <CategoryAdmin v-else-if="view === 'categories'" />

      <!-- 采购与盘点 -->
      <SupplyAdmin v-else-if="view === 'supply'" />

      <!-- 会员营销 -->
      <MemberAdmin v-else-if="view === 'member'" />

      <!-- 挂单 -->
      <main v-else-if="view === 'parked'" class="main-simple">
        <div class="card wide">
          <h3>待取挂单</h3>
          <div v-for="p in parked" :key="String((p as any).park_id)" class="row-item">
            <span>单号 #{{ (p as any).park_id }} · {{ (p as any).created_at }}</span>
            <button class="btn" @click="resume((p as any).park_id)">取单</button>
          </div>
          <div v-if="!parked.length" class="empty">暂无挂单</div>
        </div>
      </main>

      <!-- 报表 -->
      <main v-else-if="view === 'report'" class="main-simple">
        <div class="card wide">
          <h3>日结报表</h3>
          <div class="search-row">
            <input v-model="reportDate" class="input" type="date" />
            <button class="btn" @click="loadReport">查询</button>
          </div>
          <div v-if="report" class="stats">
            <div class="stat"><span>订单数</span><strong>{{ report.order_count }}</strong></div>
            <div class="stat"><span>销售额</span><strong>¥{{ formatMoney(report.total_sales_cents) }}</strong></div>
            <div class="stat"><span>退款额</span><strong>¥{{ formatMoney(report.refund_cents) }}</strong></div>
            <div class="stat"><span>优惠额</span><strong>¥{{ formatMoney(report.discount_cents) }}</strong></div>
          </div>
        </div>

        <div class="card wide">
          <h3>班次管理</h3>
          <div class="search-row">
            <input v-model="openingFloat" class="input" type="number" placeholder="开班备用金(元)" />
            <button class="btn" @click="doOpenShift">开班</button>
            <span v-if="currentShiftId" class="badge">当前班次 {{ currentShiftId }}</span>
          </div>
          <div class="search-row">
            <input v-model="actualCash" class="input" type="number" placeholder="关班实收现金(元)" />
            <button class="btn" @click="doCloseShift">关班（需授权）</button>
          </div>
          <div class="search-row">
            <input v-model.number="shiftId" class="input" type="number" placeholder="班次 ID" />
            <button class="btn" @click="loadShift">查询汇总</button>
          </div>
          <table v-if="shiftRows.length" class="tbl">
            <thead><tr><th>收银员</th><th>订单数</th><th>应收</th><th>实收</th><th>现金净额</th></tr></thead>
            <tbody>
              <tr v-for="r in shiftRows" :key="r.cashier_id">
                <td>{{ r.cashier_id }}</td>
                <td>{{ r.order_count }}</td>
                <td>{{ formatMoney(r.total_cents) }}</td>
                <td>{{ formatMoney(r.pay_cents) }}</td>
                <td>{{ formatMoney(r.cash_net_cents) }}</td>
              </tr>
            </tbody>
          </table>
        </div>

        <div class="card wide">
          <h3>多维销售报表</h3>
          <div class="search-row">
            <input v-model="reportFrom" class="input" type="date" />
            <input v-model="reportTo" class="input" type="date" />
            <button class="btn" @click="loadReports">加载报表</button>
            <button class="btn" @click="exportReconcile">导出对账CSV</button>
          </div>
          <div v-if="channelReport" class="stats">
            <div class="stat"><span>渠道净额合计</span><strong>¥{{ formatMoney(channelReport.channels.reduce((s, c) => s + c.amount_cents, 0)) }}</strong></div>
            <div class="stat"><span>账实相符</span><strong>{{ channelReport.balanced ? '是' : '否' }}</strong></div>
          </div>
          <table v-if="channelReport?.channels.length" class="tbl">
            <thead><tr><th>支付渠道</th><th>净额</th><th>笔数</th></tr></thead>
            <tbody>
              <tr v-for="c in channelReport.channels" :key="c.channel">
                <td>{{ c.channel }}</td><td>¥{{ formatMoney(c.amount_cents) }}</td><td>{{ c.count }}</td>
              </tr>
            </tbody>
          </table>
          <h4>按商品</h4>
          <table v-if="productRows.length" class="tbl">
            <thead><tr><th>SKU</th><th>数量</th><th>销售额</th><th>优惠</th></tr></thead>
            <tbody>
              <tr v-for="p in productRows" :key="p.sku_id">
                <td>{{ p.sku_id }}</td><td>{{ p.qty }}</td><td>¥{{ formatMoney(p.sales_cents) }}</td><td>¥{{ formatMoney(p.discount_cents) }}</td>
              </tr>
            </tbody>
          </table>
          <h4>按类目</h4>
          <table v-if="categoryRows.length" class="tbl">
            <thead><tr><th>类目</th><th>数量</th><th>销售额</th></tr></thead>
            <tbody>
              <tr v-for="c in categoryRows" :key="c.category_id">
                <td>{{ c.category_name }}</td><td>{{ c.qty }}</td><td>¥{{ formatMoney(c.sales_cents) }}</td>
              </tr>
            </tbody>
          </table>
          <h4>按收银员</h4>
          <table v-if="cashierRows.length" class="tbl">
            <thead><tr><th>收银员ID</th><th>订单数</th><th>实收</th></tr></thead>
            <tbody>
              <tr v-for="c in cashierRows" :key="c.cashier_id">
                <td>{{ c.cashier_id }}</td><td>{{ c.order_count }}</td><td>¥{{ formatMoney(c.pay_cents) }}</td>
              </tr>
            </tbody>
          </table>
        </div>

        <div class="card wide">
          <h3>订单操作（作废 / 退货）</h3>
          <div class="search-row">
            <input v-model="opOrderNo" class="input" placeholder="输入订单号" />
            <button class="btn" @click="loadOrderItems">查询明细</button>
          </div>
          <table v-if="opItems.length" class="tbl">
            <thead><tr><th>商品</th><th>数量</th><th>已退</th><th>单价</th><th>本次退货数</th></tr></thead>
            <tbody>
              <tr v-for="r in opItems" :key="r.item_id">
                <td>{{ r.product_name }}</td>
                <td>{{ r.qty }}</td>
                <td>{{ r.returned_qty }}</td>
                <td>{{ formatMoney(r.unit_price_cents) }}</td>
                <td><input v-model="opQty[r.item_id]" class="input input-sm" type="number" min="0" :max="r.qty - r.returned_qty" /></td>
              </tr>
            </tbody>
          </table>
          <div v-if="opItems.length" class="op-form">
            <button class="btn" @click="doVoid">作废订单</button>
            <button class="btn primary" @click="doRefund">退货退款</button>
          </div>
          <p class="muted small">作废：仅待支付单或当日现金单；扫码已支付订单只能退货退款。退货受角色限额约束。</p>
        </div>
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

      <!-- 设置中心：左侧子导航 + 右侧内容 -->
      <main v-else class="settings-center">
        <nav class="settings-nav">
          <button
            v-for="t in settingsTabs"
            :key="t.id"
            :class="['settings-nav-btn', { active: settingsTab === t.id }]"
            @click="settingsTab = t.id"
          >
            <Icon :name="t.icon" :size="ICON_SIZE.md" />{{ t.label }}
          </button>
        </nav>

        <div class="settings-content">
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
              <p class="muted small">当前：{{ storeProfile?.industry_name || '未设置' }} · {{ storeProfile?.store_name || '未命名门店' }}。切换行业只新增分类模板，不影响已有商品。</p>
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

          <!-- 系统配置 -->
          <div v-else-if="settingsTab === 'system'" class="card wide">
            <h3>系统配置</h3>
            <div v-for="c in editableConfigs" :key="c.config_key" class="config-row config-row-stacked">
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
                  :type="c.kind === 'text' ? 'text' : 'number'"
                  :step="c.kind === 'decimal' ? '0.01' : '1'"
                />
                <button class="btn btn-sm" @click="saveConfig(c.config_key)">保存</button>
              </div>
            </div>
            <p class="muted small">扫码判定间隔 20–200 毫秒；业务日切点格式 HH:MM；最大折扣率 0–1。</p>
          </div>

          <!-- 支付设置 -->
          <div v-else-if="settingsTab === 'payment'" class="card wide">
            <h3>支付设置</h3>
            <div class="search-row">
              <span class="config-key">网关模式</span>
              <span class="muted small">微信：{{ gatewayModes.wechat === 'REAL' ? '真实渠道' : '本地确认（未配置凭据）' }}；支付宝：{{ gatewayModes.alipay === 'REAL' ? '真实渠道' : '本地确认（未配置凭据）' }}。保存凭据后需重启应用生效。</span>
            </div>
            <h4>支付渠道开关</h4>
            <div class="config-row" v-for="(v, k) in channelDraft" :key="k">
              <span class="config-key">{{ k }}</span>
              <Switch :model-value="v === '1'" :label="k" @update:model-value="toggleChannel(k, v)" />
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

          <!-- 渠道对账 -->
          <div v-else-if="settingsTab === 'recon'" class="card wide">
            <h3>渠道账单对账（PAY-05/06）</h3>
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
              <thead><tr><th>渠道</th><th>差异类型</th><th>订单号</th><th>本地</th><th>渠道</th><th>说明</th></tr></thead>
              <tbody>
                <tr v-for="d in reconDiffs" :key="d.recon_id">
                  <td>{{ d.channel }}</td>
                  <td>{{ d.diff_type }}</td>
                  <td class="mono">{{ d.order_no || '—' }}</td>
                  <td>{{ formatMoney(d.local_amount_cents) }}</td>
                  <td>{{ formatMoney(d.channel_amount_cents) }}</td>
                  <td class="muted small">{{ d.detail }}</td>
                </tr>
              </tbody>
            </table>
          </div>

          <!-- 优惠券 -->
          <div v-else-if="settingsTab === 'coupon'" class="card wide">
            <h3>优惠券管理</h3>
            <div class="op-form">
              <input v-model="couponForm.discountYuan" class="input" type="number" placeholder="优惠额(元)" />
              <input v-model="couponForm.minOrderYuan" class="input" type="number" placeholder="门槛(元)" />
              <input v-model="couponForm.validUntil" class="input" type="datetime-local" placeholder="有效期" />
              <button class="btn" @click="addCoupon">发券</button>
            </div>
            <table v-if="coupons.length" class="tbl">
              <thead><tr><th>券码</th><th>优惠额</th><th>门槛</th><th>状态</th></tr></thead>
              <tbody>
                <tr v-for="c in coupons" :key="c.coupon_code">
                  <td>{{ c.coupon_code }}</td><td>¥{{ formatMoney(c.discount_cents) }}</td>
                  <td>¥{{ formatMoney(c.min_order_cents) }}</td><td>{{ c.status }}</td>
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
        <div class="modal">
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
        <div class="modal native-modal">
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

      <div v-if="toast" class="toast" :class="{ err: toast.startsWith('错误') }">{{ toast }}</div>
    </div>
  </div>
</template>
