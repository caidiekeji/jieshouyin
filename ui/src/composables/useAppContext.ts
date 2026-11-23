import { computed, nextTick } from 'vue'
import { storeToRefs } from 'pinia'
import { formatMoney } from '../api'
import { ICON_SIZE, CENTS_PER_YUAN } from '../constants'
import { useAppStore } from '../stores/app'
import { usePosStore } from '../stores/pos'
import { useOpsStore } from '../stores/ops'

// 现金弹窗展示常量：界面固有，随视图层保留
const QUICK_CASH_YUAN = [10, 20, 50, 100]
const CASH_KEYPAD = ['7', '8', '9', '4', '5', '6', '1', '2', '3', '.', '0', '退格'] as const

export function useAppContext() {
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
  activationServiceReachable, probingService, theme, printAuto, currentUser, currentRole,
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
  configs, configDraft, credChannel, credDraft, credSaved, credBusy, channelDraft,
  reconDate, reconText, reconSummary, reconDiffs, printQueue,
  receiptTemplateJson, receiptPaperWidth, receiptPreview,
  opOrderNo, opItems, opQty, opMode, users, uForm,
} = opsRefs

// ===== 模板直接引用的 computed =====
const needsActivation = computed(() => app.needsActivation)
const authStep = computed(() => app.authStep)
const industry = computed(() => app.industry)
const sceneBoardStage = computed(() => pos.sceneBoardStage)
const sceneCatalogOnly = computed(() => pos.sceneCatalogOnly)
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
const activateInSettings = () => app.activateInSettings()
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
/** 顶部交班入口：收银员动作，进入独立班次管理视图（不依赖 report.view 权限） */
const goShift = () => {
  const app = useAppStore()
  app.view = 'shift'
  ops.openTool('shift')
}
const configLabel = (k: string) => ops.configLabel(k)
const configOptions = (k: string) => ops.configOptions(k)
const loadConfigs = () => ops.loadConfigs()
const saveConfig = (k: string) => ops.saveConfig(k)
const toggleConfigSwitch = (k: string) => ops.toggleConfigSwitch(k)
const configGroupChanged = (title: string) => ops.configGroupChanged(title)
const saveConfigGroup = (title: string) => ops.saveConfigGroup(title)
const savePointsConfigs = () => ops.savePointsConfigs()
const loadPaymentSettings = () => ops.loadPaymentSettings()
const toggleChannel = (k: string, cur: string) => ops.toggleChannel(k, cur)
const saveCredential = (f: string) => ops.saveCredential(f)
const removeCredential = (f: string) => ops.removeCredential(f)
const saveAllCredentials = () => ops.saveAllCredentials()
/** 商户凭据弹窗开闭：状态在 ops store，弹窗统一由 DialogLayer 渲染 */
const openCredDialog = (channel: 'WECHAT' | 'ALIPAY') => ops.openCredDialog(channel)
const closeCredDialog = () => ops.closeCredDialog()
const importBill = (c: 'WECHAT' | 'ALIPAY') => ops.importBill(c)
const loadReconDiffs = () => ops.loadReconDiffs()
const loadPrintQueue = () => ops.loadPrintQueue()
const doRetryPrint = (no: string) => ops.doRetryPrint(no)
const previewReceipt = () => ops.previewReceipt()
const saveReceiptTemplate = () => ops.saveReceiptTemplate()
const loadOrderItems = () => ops.loadOrderItems()
const doVoid = () => ops.doVoid()
const doRefund = () => ops.doRefund()
const loadUsers = () => ops.loadUsers()
const addUser = () => ops.addUser()
const toggleUser = (u: Parameters<typeof ops.toggleUser>[0]) => ops.toggleUser(u)
const goSettings = () => ops.goSettings()

const goPOS = () => { app.goPOS(); void nextTick(() => pos.focusScan()) }
const goProducts = () => app.goProducts()

  return {
    ICON_SIZE,
    CENTS_PER_YUAN,
    formatMoney,
    QUICK_CASH_YUAN,
    CASH_KEYPAD,
    activateCustomer,
    activateError,
    activateInSettings,
    activateMode,
    activateToken,
    activationPending,
    activationServiceReachable,
    activeReport,
    activeReportId,
    activeTable,
    actualCash,
    addFromCatalog,
    addPaySplit,
    addToCart,
    addUser,
    adminPass,
    adminUser,
    applyCouponDialog,
    applyCurrentSession,
    applyPaymentCode,
    applyTheme,
    atLeast,
    authDialog,
    authError,
    authPass,
    authStep,
    authUser,
    backToDiningTables,
    banner,
    bootError,
    cancelLockSetup,
    cancelNative,
    cancelSensitiveAuth,
    cart,
    cashDialogInput,
    cashDialogOpen,
    cashOverpaid,
    cashShort,
    cashTendered,
    cashTenderedCents,
    centerFrom,
    centerTo,
    changeDue,
    changeQty,
    channelDraft,
    channelEnabled,
    clearActiveTable,
    clearCart,
    clearCoupon,
    clearLockCode,
    clearMember,
    closeNativeDialog,
    configDraft,
    configLabel,
    configOptions,
    configs,
    confirmIndustry,
    confirmMemberDialog,
    confirmNativeLocal,
    confirmSensitiveAuth,
    couponCode,
    couponDialogInput,
    couponDialogOpen,
    couponDraft,
    couponHint,
    couponRecoverable,
    credChannel,
    credDraft,
    credSaved,
    credBusy,
    credDialogOpen,
    currentCredFields,
    currentRole,
    currentShiftId,
    currentUser,
    differenceReason,
    sceneBoardStage,
    sceneCatalogOnly,
    doCloseShift,
    doLogout,
    doOpenShift,
    doRefund,
    doRetryPrint,
    doVoid,
    effectivePrice,
    enterMixedMode,
    enterTrial,
    exactCash,
    exportActiveReport,
    findMember,
    finishIndustrySetup,
    flushTableCart,
    focusScan,
    gatewayModes,
    goPOS,
    goProducts,
    goReport,
    goShift,
    goSettings,
    goodsTab,
    gotoOrder,
    gotoRefund,
    handleScannedCode,
    hashLockCode,
    importBill,
    industries,
    industry,
    isLocalGateway,
    keyword,
    lastOrder,
    license,
    licenseStatusLabel,
    loadConfigs,
    loadIndustries,
    loadLockCode,
    loadOrderItems,
    loadParked,
    loadPayChannels,
    loadPaymentSettings,
    loadPosCatalog,
    loadPosCategory,
    loadPrintQueue,
    loadReconDiffs,
    loadReportCatalog,
    loadShift,
    loadStoreProfile,
    loadTechnicians,
    loadUsers,
    lockCode,
    lockCodeDraft,
    lockError,
    lockInput,
    lockScreen,
    lockSetupOpen,
    locked,
    loginError,
    loginPass,
    loginUser,
    memberBalances,
    memberDialogInput,
    memberDialogOpen,
    memberFindError,
    memberId,
    memberPhone,
    memberPriceOf,
    nativeDialog,
    nativeMode,
    needAdmin,
    needsActivation,
    onCashInput,
    onDutyTechnicians,
    onScanKeydown,
    onboarding,
    opItems,
    opMode,
    opOrderNo,
    opQty,
    openCouponDialog,
    openMemberDialog,
    openOfficialSite,
    openParkDialog,
    openTool,
    openingFloat,
    orderNo,
    paidPreview,
    park,
    parkDialogOpen,
    parked,
    payAuthCode,
    payChannel,
    payCodeInput,
    payMethods,
    payMode,
    payShortfall,
    paySplits,
    payableAmount,
    paying,
    pendingIndustry,
    pointsConfigs,
    pointsToast,
    posCatalogBusy,
    posCategories,
    posCategoryId,
    posDialogOpen,
    posProducts,
    posRootCategories,
    pressCashKey,
    printAuto,
    printQueue,
    printReceipt,
    previewReceipt,
    receiptPaperWidth,
    receiptPreview,
    receiptTemplateJson,
    saveReceiptTemplate,
    probeActivationService,
    probingService,
    productImageSrc,
    products,
    pushWeighToCart,
    quote,
    readActivationPending,
    rebindTableMember,
    reconDate,
    reconDiffs,
    reconSummary,
    reconText,
    refreshCartMemberPrices,
    refreshChessFee,
    refreshLicense,
    refreshQuote,
    refundOrderNo,
    releaseActiveTable,
    removeCredential,
    removePaySplit,
    renewMode,
    reportGroups,
    reportLoading,
    openCredDialog,
    closeCredDialog,
    saveAllCredentials,
    reportMetas,
    reportTool,
    requireSensitiveAuth,
    resetPayState,
    restoreCurrentShift,
    resume,
    returnTableLine,
    routeAfterLogin,
    runActiveReport,
    sanitizeCashInput,
    saveConfig,
    configGroupChanged,
    saveConfigGroup,
    savePointsConfigs,
    saveCredential,
    saveLockCode,
    saveStoreName,
    scanCapture,
    scheduleTableSave,
    search,
    searchBusy,
    selectPayChannel,
    selectPosCategory,
    selectReport,
    selectSceneTable,
    setActivationPending,
    setLineQty,
    settingsTab,
    settingsTabs,
    shiftId,
    shiftRows,
    showSceneError,
    showToast,
    startNativePolling,
    stopChessFeeTimer,
    stopNativePolling,
    storeNameDraft,
    storeProfile,
    submitActivate,
    submitActivateOnline,
    submitAdmin,
    submitCheckout,
    submitLogin,
    submitTableOrder,
    subtotal,
    systemConfigGroups,
    tableSentQty,
    technicians,
    theme,
    toast,
    toastKind,
    toggleChannel,
    toggleConfigSwitch,
    toggleUser,
    trialDaysLeft,
    uForm,
    unlock,
    users,
    view,
    visibleSettingsTabs,
  }
}
