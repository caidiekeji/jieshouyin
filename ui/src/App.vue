<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, watch } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { storeToRefs } from 'pinia'
import Icon from './components/Icon.vue'
import RestaurantScene from './components/RestaurantScene.vue'
import KitchenRoutePanel from './components/KitchenRoutePanel.vue'
import ChessScene from './components/ChessScene.vue'
import BabyScene from './components/BabyScene.vue'
import WellnessScene from './components/WellnessScene.vue'
import TechnicianPanel from './components/TechnicianPanel.vue'
import ProductAdmin from './components/ProductAdmin.vue'
import CategoryAdmin from './components/CategoryAdmin.vue'
import SupplyAdmin from './components/SupplyAdmin.vue'
import MemberAdmin from './components/MemberAdmin.vue'
import OrderDetailPanel from './components/OrderDetailPanel.vue'
import RefundPanel from './components/RefundPanel.vue'
import Switch from './components/Switch.vue'
import SetupSteps from './components/SetupSteps.vue'
import { ICON_SIZE, CENTS_PER_YUAN } from './constants'
import { formatMoney } from './api'

// 现金弹窗展示常量：界面固有，随视图层保留
const QUICK_CASH_YUAN = [10, 20, 50, 100]
const CASH_KEYPAD = ['7', '8', '9', '4', '5', '6', '1', '2', '3', '.', '0', '退格'] as const
import { useAppStore } from './stores/app'
import { usePosStore } from './stores/pos'
import { useOpsStore } from './stores/ops'

const app = useAppStore()
const pos = usePosStore()
const ops = useOpsStore()

// ===== 模板直接引用的状态（解构保持响应式）=====
const appRefs = storeToRefs(app)
const posRefs = storeToRefs(pos)
const opsRefs = storeToRefs(ops)

const {
  view, goodsTab, bootError, license, needAdmin, onboarding, activationPending,
  adminUser, adminPass, loginUser, loginPass, loginError,
  activateToken, activateError, renewMode, activateMode, activateCustomer,
  activationServiceReachable, probingService, theme, currentUser, currentRole,
  lockCode, lockCodeDraft, lockSetupOpen, locked, lockInput, lockError,
  settingsTab, storeProfile, industries, pendingIndustry, storeNameDraft,
  orderNo, refundOrderNo, toast, toastKind,
  authDialog, authUser, authPass, authError,
  currentShiftId, shiftId, channelEnabled, gatewayModes,
} = appRefs

const {
  activeTable, keyword, products, posCategories, posProducts, posCategoryId,
  posCatalogBusy, searchBusy, scanCapture, cart, quote, technicians,
  cashTendered, lastOrder, paying, memberPhone, memberId, memberBalances,
  memberFindError, couponCode, couponDialogOpen, couponDraft, couponDialogInput,
  memberDialogOpen, memberDialogInput, paySplits, payChannel, payMode,
  cashDialogOpen, cashDialogInput, payAuthCode, payCodeInput,
  nativeDialog, nativeMode, parked, parkDialogOpen,
} = posRefs

const {
  shiftRows, openingFloat, actualCash, differenceReason,
  reportMetas, activeReportId, activeReport, reportLoading, centerFrom, centerTo, reportTool,
  configs, configDraft, credChannel, credDraft, credSaved, channelDraft,
  reconDate, reconText, reconSummary, reconDiffs, printQueue,
  opOrderNo, opItems, opQty, opMode, users, uForm,
} = opsRefs

// ===== 模板直接引用的 computed =====
const needsActivation = computed(() => app.needsActivation)
const authStep = computed(() => app.authStep)
const industry = computed(() => app.industry)
const diningTableStage = computed(() => pos.diningTableStage)
const atLeast = app.atLeast
const settingsTabs = computed(() => app.settingsTabs)
const visibleSettingsTabs = computed(() => app.visibleSettingsTabs)
const banner = computed(() => app.banner)
const licenseStatusLabel = computed(() => app.licenseStatusLabel)
const trialDaysLeft = computed(() => app.trialDaysLeft)

const posRootCategories = computed(() => pos.posRootCategories)
const onDutyTechnicians = computed(() => pos.onDutyTechnicians)
const tableSentQty = computed(() => pos.tableSentQty)
const subtotal = computed(() => pos.subtotal)
const payableAmount = computed(() => pos.payableAmount)
const paidPreview = computed(() => pos.paidPreview)
const payShortfall = computed(() => pos.payShortfall)
const payMethods = computed(() => pos.payMethods)
const isLocalGateway = computed(() => pos.isLocalGateway)
const cashTenderedCents = computed(() => pos.cashTenderedCents)
const changeDue = computed(() => pos.changeDue)
const cashShort = computed(() => pos.cashShort)
const cashOverpaid = computed(() => pos.cashOverpaid)
const couponRecoverable = computed(() => pos.couponRecoverable)
const couponHint = computed(() => pos.couponHint)

const reportGroups = computed(() => ops.reportGroups)
const pointsConfigs = computed(() => ops.pointsConfigs)
const systemConfigGroups = computed(() => ops.systemConfigGroups)
const currentCredFields = computed(() => ops.currentCredFields)

// ===== 模板事件 → store actions（保持模板方法名不变）=====
const showToast = (msg: string, isError = false) => app.showToast(msg, isError)
const showSceneError = (msg: string) => pos.showSceneError(msg)
const pointsToast = (base: string, points: number | undefined) => pos.pointsToast(base, points)
const effectivePrice = (l: { unit_price_cents: number; member_price_cents?: number | null }) => pos.effectivePrice(l)
const memberPriceOf = (p: Parameters<typeof pos.memberPriceOf>[0]) => pos.memberPriceOf(p)
const productImageSrc = (p: Parameters<typeof pos.productImageSrc>[0]) => pos.productImageSrc(p)
const posDialogOpen = () => pos.posDialogOpen()

const readActivationPending = () => app.readActivationPending()
const setActivationPending = (v: boolean) => app.setActivationPending(v)
const loadStoreProfile = () => app.loadStoreProfile()
const loadIndustries = () => app.loadIndustries()
const confirmIndustry = () => app.confirmIndustry()
const saveStoreName = () => app.saveStoreName()
const finishIndustrySetup = () => app.finishIndustrySetup()
const probeActivationService = () => app.probeActivationService()
const openOfficialSite = () => app.openOfficialSite()
const submitActivateOnline = () => app.submitActivateOnline()
const refreshLicense = () => app.refreshLicense()
const submitActivate = () => app.submitActivate()
const applyCurrentSession = () => app.applyCurrentSession()
const routeAfterLogin = () => app.routeAfterLogin()
const restoreCurrentShift = () => app.restoreCurrentShift()
const submitAdmin = () => app.submitAdmin()
const submitLogin = () => app.submitLogin()
const enterTrial = () => app.enterTrial()
const loadLockCode = () => app.loadLockCode()
const hashLockCode = (code: string) => app.hashLockCode(code)
const saveLockCode = () => app.saveLockCode()
const clearLockCode = () => app.clearLockCode()
const lockScreen = () => app.lockScreen()
const unlock = () => app.unlock()
const cancelLockSetup = () => app.cancelLockSetup()
const applyTheme = (t: 'light' | 'dark') => app.applyTheme(t)
const requireSensitiveAuth = (p: string, t: string) => app.requireSensitiveAuth(p, t)
const confirmSensitiveAuth = () => app.confirmSensitiveAuth()
const cancelSensitiveAuth = () => app.cancelSensitiveAuth()
const gotoRefund = (no?: string) => app.gotoRefund(no)
const gotoOrder = (no: string) => app.gotoOrder(no)
const doLogout = () => app.doLogout()
const loadPayChannels = () => app.loadPayChannels()

const scheduleTableSave = () => pos.scheduleTableSave()
const flushTableCart = () => pos.flushTableCart()
const selectSceneTable = (t: Parameters<typeof pos.selectSceneTable>[0], lines: Parameters<typeof pos.selectSceneTable>[1]) => pos.selectSceneTable(t, lines)
const refreshChessFee = () => pos.refreshChessFee()
const stopChessFeeTimer = () => pos.stopChessFeeTimer()
const clearActiveTable = () => pos.clearActiveTable()
const backToDiningTables = () => pos.backToDiningTables()
const releaseActiveTable = () => pos.releaseActiveTable()
const clearMember = () => pos.clearMember()
const findMember = () => pos.findMember()
const refreshCartMemberPrices = () => pos.refreshCartMemberPrices()
const rebindTableMember = () => pos.rebindTableMember()
const openMemberDialog = () => pos.openMemberDialog()
const confirmMemberDialog = () => pos.confirmMemberDialog()
const onScanKeydown = (e: KeyboardEvent) => pos.onScanKeydown(e)
const handleScannedCode = (code: string) => pos.handleScannedCode(code)
const applyPaymentCode = (c: string, code: string) => pos.applyPaymentCode(c, code)
const pushWeighToCart = (hit: Parameters<typeof pos.pushWeighToCart>[0]) => pos.pushWeighToCart(hit)
const focusScan = () => pos.focusScan()
const search = () => pos.search()
const loadPosCategory = (id: number | null) => pos.loadPosCategory(id)
const loadPosCatalog = () => pos.loadPosCatalog()
const selectPosCategory = (id: number) => pos.selectPosCategory(id)
const addFromCatalog = (p: Parameters<typeof pos.addFromCatalog>[0]) => pos.addFromCatalog(p)
const addToCart = (p: Parameters<typeof pos.addToCart>[0]) => pos.addToCart(p)
const changeQty = (i: number, d: number) => pos.changeQty(i, d)
const setLineQty = (i: number, raw: string) => pos.setLineQty(i, raw)
const clearCart = () => pos.clearCart()
const refreshQuote = (opts?: { silent?: boolean }) => pos.refreshQuote(opts)
const openCouponDialog = () => pos.openCouponDialog()
const applyCouponDialog = () => pos.applyCouponDialog()
const clearCoupon = () => pos.clearCoupon()
const resetPayState = () => pos.resetPayState()
const addPaySplit = () => pos.addPaySplit()
const removePaySplit = (i: number) => pos.removePaySplit(i)
const enterMixedMode = () => pos.enterMixedMode()
const selectPayChannel = (id: Parameters<typeof pos.selectPayChannel>[0]) => pos.selectPayChannel(id)
const sanitizeCashInput = (raw: string) => pos.sanitizeCashInput(raw)
const onCashInput = (e: Event) => pos.onCashInput(e)
const pressCashKey = (key: string) => pos.pressCashKey(key)
const exactCash = () => pos.exactCash()
const submitCheckout = () => pos.submitCheckout()
const startNativePolling = (no: string) => pos.startNativePolling(no)
const stopNativePolling = () => pos.stopNativePolling()
const closeNativeDialog = () => pos.closeNativeDialog()
const cancelNative = () => pos.cancelNative()
const confirmNativeLocal = () => pos.confirmNativeLocal()
const submitTableOrder = () => pos.submitTableOrder()
const returnTableLine = (line: Parameters<typeof pos.returnTableLine>[0]) => pos.returnTableLine(line)
const loadTechnicians = () => pos.loadTechnicians()
const park = () => pos.park()
const loadParked = () => pos.loadParked()
const resume = (id: number) => pos.resume(id)
const openParkDialog = () => pos.openParkDialog()
const printReceipt = () => pos.printReceipt()

const loadShift = () => ops.loadShift()
const doOpenShift = () => ops.doOpenShift()
const doCloseShift = () => ops.doCloseShift()
const loadReportCatalog = () => ops.loadReportCatalog()
const runActiveReport = () => ops.runActiveReport()
const selectReport = (id: string) => ops.selectReport(id)
const openTool = (t: Parameters<typeof ops.openTool>[0]) => ops.openTool(t)
const exportActiveReport = () => ops.exportActiveReport()
const goReport = () => ops.goReport()
const configLabel = (k: string) => ops.configLabel(k)
const configOptions = (k: string) => ops.configOptions(k)
const loadConfigs = () => ops.loadConfigs()
const saveConfig = (k: string) => ops.saveConfig(k)
const toggleConfigSwitch = (k: string) => ops.toggleConfigSwitch(k)
const loadPaymentSettings = () => ops.loadPaymentSettings()
const toggleChannel = (k: string, cur: string) => ops.toggleChannel(k, cur)
const saveCredential = (f: string) => ops.saveCredential(f)
const removeCredential = (f: string) => ops.removeCredential(f)
const importBill = (c: 'WECHAT' | 'ALIPAY') => ops.importBill(c)
const loadReconDiffs = () => ops.loadReconDiffs()
const loadPrintQueue = () => ops.loadPrintQueue()
const doRetryPrint = (no: string) => ops.doRetryPrint(no)
const loadOrderItems = () => ops.loadOrderItems()
const doVoid = () => ops.doVoid()
const doRefund = () => ops.doRefund()
const loadUsers = () => ops.loadUsers()
const addUser = () => ops.addUser()
const toggleUser = (u: Parameters<typeof ops.toggleUser>[0]) => ops.toggleUser(u)
const goSettings = () => ops.goSettings()

const goPOS = () => { app.goPOS(); void nextTick(() => pos.focusScan()) }
const goProducts = () => app.goProducts()

// ===== 视图层 watch（状态联动）=====
watch(cart, () => pos.scheduleTableSave(), { deep: true })
watch(industry, () => pos.onIndustryChange())
watch(keyword, (val) => pos.onKeywordChange(val))
watch(view, (v) => {
  if (v === 'pos') {
    void pos.loadPosCatalog()
    void pos.loadTechnicians()
  }
})
watch([view, currentRole], () => {
  const guarded: string[] = ['products', 'categories', 'supply', 'member', 'refund', 'order', 'report', 'settings', 'parked']
  if (guarded.includes(view.value) && !app.atLeast('MANAGER')) view.value = 'pos'
  if (view.value === 'settings') {
    const tab = app.settingsTabs.find((t) => t.id === settingsTab.value)
    if (tab && !app.atLeast(tab.minRole)) settingsTab.value = 'overview'
  }
})
watch(
  () => [payMode.value, payChannel.value, nativeMode.value] as const,
  () => {
    if (payMode.value === 'simple' && !nativeMode.value &&
        (payChannel.value === 'WECHAT' || payChannel.value === 'ALIPAY')) {
      void nextTick(() => pos.onPayChannelFocusWatch())
    }
  }
)
watch(cashDialogOpen, (open) => { if (open) void nextTick(() => pos.onCashDialogOpenWatch()) })
watch(memberDialogOpen, (open) => { if (open) void nextTick(() => pos.onMemberDialogOpenWatch()) })
watch(couponDialogOpen, (open) => { if (open) void nextTick(() => pos.onCouponDialogOpenWatch()) })
watch(
  [() => cart.value, memberId],
  () => { void pos.refreshQuote() },
  { deep: true }
)

// ===== 快捷键（视图层）=====
function handlePosShortcut(e: KeyboardEvent): boolean {
  if (view.value !== 'pos' || e.repeat) return false
  const key = e.key.toLowerCase()
  if (e.key === 'F2' && !posDialogOpen()) {
    e.preventDefault(); focusScan(); return true
  }
  if (e.key === 'F3' && !posDialogOpen()) {
    e.preventDefault(); openMemberDialog(); return true
  }
  if (e.ctrlKey && !e.altKey && key === 'g' && !posDialogOpen()) {
    e.preventDefault(); void park(); return true
  }
  if (e.ctrlKey && !e.altKey && key === 'q' && !posDialogOpen()) {
    e.preventDefault(); void openParkDialog(); return true
  }
  const tag = (e.target as HTMLElement | null)?.tagName
  const editing = tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT'
  if (e.ctrlKey && !e.altKey && key === 'z' && !editing && !posDialogOpen()) {
    e.preventDefault(); clearCart(); return true
  }
  return false
}
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

onMounted(async () => {
  loadLockCode()
  void listen('lock-request', () => { lockScreen() }).catch(() => {})
  const saved = localStorage.getItem('checkoutr-theme') as 'light' | 'dark' | null
  if (saved) applyTheme(saved)
  window.addEventListener('keydown', onGlobalKeydown)
  await app.bootstrap()
})
onUnmounted(() => {
  window.removeEventListener('keydown', onGlobalKeydown)
  pos.stopNativePolling()
  pos.stopChessFeeTimer()
})
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
      <main
        v-if="view === 'pos'"
        class="pos-layout"
        :class="{ 'dining-table-stage': diningTableStage }"
      >
        <!-- 餐饮桌台首页：未选中桌台时首屏只显示桌台看板，新建/进入桌台后再点菜或收银 -->
        <section v-if="diningTableStage" class="panel dining-home-panel">
          <RestaurantScene
            :active-table-id="null"
            :cart="[]"
            :member-id="memberId"
            :can-manage="atLeast('MANAGER')"
            @select="selectSceneTable"
            @cleared="clearActiveTable"
            @error="showSceneError"
          />
        </section>

        <!-- 点菜/收银阶段：餐饮已选桌台，或其他行业 -->
        <template v-else>
          <section class="panel product-panel">
            <!-- 餐饮进入桌台后：显示当前桌台与返回桌台入口 -->
            <div v-if="industry === 'RESTAURANT' && activeTable" class="dining-active-bar">
              <div>
                <strong>{{ activeTable.code }}</strong>
                <span class="muted small">{{ activeTable.name }} · 用餐中</span>
              </div>
              <button class="btn btn-sm ghost" @click="backToDiningTables">← 返回桌台</button>
            </div>

            <ChessScene
              v-if="industry === 'BOARD_GAME'"
              :active-table-id="activeTable?.table_id ?? null"
              :cart="cart"
              :member-id="memberId"
              :can-manage="atLeast('MANAGER')"
              @select="selectSceneTable"
              @cleared="clearActiveTable"
              @error="showSceneError"
            />
            <BabyScene
              v-if="industry === 'BABY'"
              @add="addToCart"
              @error="showSceneError"
            />
            <WellnessScene
              v-if="industry === 'WELLNESS'"
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
        </template>
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
              <input v-model="differenceReason" class="input" type="text" placeholder="差异原因(有现金差额时必填)" />
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

          <!-- 打印设置：厨打档口路由 + 重试队列；不占收银首屏 -->
          <template v-else-if="settingsTab === 'print'">
            <div v-if="industry === 'RESTAURANT'" class="card wide">
              <h3>厨打档口路由</h3>
              <p class="muted small">按菜品分类把厨房小票分发到指定档口打印机；未配置的分类走系统配置里的主打印机。仅店长及以上可修改。</p>
              <KitchenRoutePanel :can-manage="atLeast('MANAGER')" />
            </div>
            <div class="card wide">
              <h3>打印重试队列</h3>
              <div v-if="printQueue.length" class="print-queue">
                <div v-for="no in printQueue" :key="no" class="print-queue-row">
                  <span>{{ no }}</span>
                  <button class="btn btn-sm" @click="doRetryPrint(no)">重打</button>
                </div>
              </div>
              <p v-else class="muted small">无待重打订单</p>
            </div>
          </template>
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
