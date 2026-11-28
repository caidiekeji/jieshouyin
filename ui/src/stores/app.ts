import { defineStore } from 'pinia'
import { api, type LicenseSnapshot, type StoreProfile, type IndustryOption } from '../api'
import {
  MS_PER_DAY,
  TOAST_DURATION_MS,
  ERROR_DURATION_MS,
} from '../constants'

export type View =
  | 'boot' | 'industry' | 'activate' | 'login' | 'pos'
  | 'products' | 'categories' | 'tables' | 'supply' | 'member'
  | 'parked' | 'report' | 'shift' | 'settings' | 'refund' | 'order'
export type Role = 'CASHIER' | 'MANAGER' | 'ADMIN'
export type SettingsTab = 'overview' | 'appearance' | 'staff' | 'technicians' | 'system' | 'payment' | 'points' | 'recon' | 'print'
export type SettingsIcon = 'store' | 'sun' | 'shield' | 'sliders' | 'wallet' | 'star' | 'report' | 'print' | 'users'

const ACTIVATION_PENDING_KEY = 'blcj.activation_pending'
const LOCK_CODE_KEY = 'blcj.lock_code'

/** 品牌更名迁移：checkoutr.* 旧 localStorage 键一次性搬到 blcj.*，搬完删除旧键（幂等） */
function migrateLegacyLocalStorageKeys(): void {
  const pairs: Array<[string, string]> = [
    ['checkoutr.activation_pending', ACTIVATION_PENDING_KEY],
    ['checkoutr.lock_code', LOCK_CODE_KEY],
    ['checkoutr-theme', 'blcj-theme'],
  ]
  for (const [oldKey, newKey] of pairs) {
    const legacy = localStorage.getItem(oldKey)
    if (legacy != null && localStorage.getItem(newKey) == null) {
      localStorage.setItem(newKey, legacy)
      localStorage.removeItem(oldKey)
    }
  }
}
migrateLegacyLocalStorageKeys()

const LOCK_HASH_RE = /^[0-9a-f]{64}$/
const ROLE_RANK: Record<Role, number> = { CASHIER: 1, MANAGER: 2, ADMIN: 3 }
const LICENSE_STATUS_LABEL: Record<string, string> = {
  UNACTIVATED: '未激活',
  TRIAL_ACTIVE: '试用中',
  ACTIVE: '已激活',
  GRACE_PERIOD: '宽限期内',
  OFFLINE_DEGRADED: '已到期（降级）',
}

export interface SettingsTabDef {
  id: SettingsTab
  label: string
  icon: SettingsIcon
  minRole: Role
  /** 限定可见行业；缺省为全部行业 */
  industries?: string[]
}

export const useAppStore = defineStore('app', {
  state: () => ({
    view: 'boot' as View,
    goodsTab: 'list' as 'list' | 'category',
    bootError: '',
    license: null as LicenseSnapshot | null,
    needAdmin: false,
    onboarding: false,
    activationPending: false,
    adminUser: 'admin',
    adminPass: '',
    loginUser: '',
    loginPass: '',
    loginError: '',
    activateToken: '',
    activateError: '',
    renewMode: false,
    activateMode: 'online' as 'online' | 'offline',
    activateCustomer: '',
    activationServiceReachable: null as boolean | null,
    probingService: false,
    theme: 'light' as 'light' | 'dark',
    // 结算后自动打印小票开关：关闭时隐藏收银处重打入口
    printAuto: true,
    currentUser: '',
    currentRole: 'CASHIER' as Role,
    lockCode: '',
    lockCodeDraft: '',
    lockSetupOpen: false,
    pendingLockAfterSetup: false,
    locked: false,
    lockInput: '',
    lockError: '',
    settingsTab: 'overview' as SettingsTab,
    storeProfile: null as StoreProfile | null,
    industries: [] as IndustryOption[],
    pendingIndustry: '',
    storeNameDraft: '',
    // 交班：登录恢复与交班视图共用
    currentShiftId: 0,
    shiftId: 1,
    // 支付渠道开关与网关模式：POS 与支付设置页共用
    channelEnabled: {} as Record<string, boolean>,
    gatewayModes: { wechat: 'LOCAL', alipay: 'LOCAL' } as { wechat: string; alipay: string },
    orderNo: '',
    orderListDate: '',
    refundOrderNo: '',
    toast: '',
    toastKind: 'info' as 'info' | 'error',
    authDialog: null as null | {
      permission: string
      title: string
      resolve: ((nonce: string) => void) | null
      reject: ((reason: string) => void) | null
    },
    authUser: '',
    authPass: '',
    authError: '',
  }),
  getters: {
    needsActivation(state): boolean {
      const status = state.license?.status
      if (!status || status === 'ACTIVE') return false
      return status === 'UNACTIVATED' || state.activationPending
    },
    authStep(state): number {
      if (state.view === 'login') return state.needAdmin ? 1 : 2
      if (state.view === 'industry') return 3
      if (state.view === 'activate') return 4
      return 0
    },
    industry(state): string {
      return state.storeProfile?.industry ?? ''
    },
    atLeast(): (role: Role) => boolean {
      return (role: Role) => ROLE_RANK[this.currentRole] >= ROLE_RANK[role]
    },
    settingsTabs(): SettingsTabDef[] {
      return [
        { id: 'overview', label: '门店概览', icon: 'store', minRole: 'MANAGER' },
        { id: 'appearance', label: '外观主题', icon: 'sun', minRole: 'MANAGER' },
        { id: 'staff', label: '员工管理', icon: 'shield', minRole: 'ADMIN' },
        { id: 'technicians', label: '技师管理', icon: 'users', minRole: 'MANAGER', industries: ['WELLNESS'] },
        { id: 'system', label: '系统配置', icon: 'sliders', minRole: 'ADMIN' },
        { id: 'payment', label: '支付设置', icon: 'wallet', minRole: 'MANAGER' },
        { id: 'points', label: '积分配置', icon: 'star', minRole: 'MANAGER' },
        { id: 'recon', label: '渠道对账', icon: 'report', minRole: 'MANAGER' },
        { id: 'print', label: '打印设置', icon: 'print', minRole: 'MANAGER' },
      ]
    },
    visibleSettingsTabs(): SettingsTabDef[] {
      return this.settingsTabs.filter(
        (t) => this.atLeast(t.minRole) && (!t.industries || t.industries.includes(this.industry)),
      )
    },
    banner(state): string {
      if (!state.license) return ''
      if (state.license.status === 'TRIAL_ACTIVE') {
        const ends = state.license.trial_ends_at
        if (!ends) return '试用期进行中'
        const days = Math.max(1, Math.ceil((new Date(ends).getTime() - Date.now()) / MS_PER_DAY))
        return `试用期剩余 ${days} 天，到期后需激活`
      }
      if (state.license.status === 'GRACE_PERIOD') return '授权宽限期内，请尽快续期'
      if (state.license.status === 'OFFLINE_DEGRADED') return '授权已到期，前台收银仍可用，后台管理受限'
      return ''
    },
    licenseStatusLabel(state): string {
      const s = state.license?.status
      if (!s) return '未知'
      return LICENSE_STATUS_LABEL[s] ?? s
    },
    trialDaysLeft(state): number {
      if (state.license?.status !== 'TRIAL_ACTIVE') return 0
      const ends = state.license.trial_ends_at
      if (!ends) return 15
      return Math.max(1, Math.ceil((new Date(ends).getTime() - Date.now()) / MS_PER_DAY))
    },
  },
  actions: {
    showToast(msg: string, isError = false) {
      this.toast = msg
      this.toastKind = isError ? 'error' : 'info'
      setTimeout(() => (this.toast = ''), isError ? ERROR_DURATION_MS : TOAST_DURATION_MS)
    },
    normalizeRole(roleName: string): Role {
      return roleName === 'ADMIN' ? 'ADMIN' : roleName === 'MANAGER' ? 'MANAGER' : 'CASHIER'
    },
    setActivationPending(pending: boolean) {
      this.activationPending = pending
      try {
        if (pending) localStorage.setItem(ACTIVATION_PENDING_KEY, '1')
        else localStorage.removeItem(ACTIVATION_PENDING_KEY)
      } catch {
        // 本地存储不可用时只影响跨重启提示，不阻断当前流程
      }
    },
    readActivationPending(): boolean {
      try {
        return localStorage.getItem(ACTIVATION_PENDING_KEY) === '1'
      } catch {
        return false
      }
    },
    async bootstrap() {
      try {
        this.license = await api.bootstrap()
        try {
          this.printAuto = await api.getPrintAuto()
        } catch {
          // 开关读取失败保留默认开，不阻断启动
        }
        await this.loadStoreProfile()
        await this.loadIndustries()
        this.needAdmin = !(await api.hasUsers())
        this.activationPending = this.readActivationPending()
        this.onboarding =
          this.needAdmin ||
          !(this.storeProfile?.configured ?? false) ||
          this.activationPending
        this.view = 'login'
      } catch (e) {
        this.bootError = String(e)
      }
    },
    async loadStoreProfile() {
      try {
        this.storeProfile = await api.getStoreProfile()
        this.storeNameDraft = this.storeProfile.store_name
        if (this.storeProfile.industry) this.pendingIndustry = this.storeProfile.industry
      } catch (e) {
        this.showToast(String(e), true)
      }
    },
    async loadIndustries() {
      try {
        this.industries = await api.listIndustries()
      } catch (e) {
        this.showToast(String(e), true)
      }
    },
    async confirmIndustry() {
      if (!this.pendingIndustry) {
        this.showToast('请选择行业', true)
        return
      }
      try {
        this.storeProfile = await api.selectIndustry(this.pendingIndustry)
        this.showToast(`已切换为${this.storeProfile.industry_name}`)
      } catch (e) {
        this.showToast(String(e), true)
      }
    },
    async saveStoreName() {
      try {
        this.storeProfile = await api.setStoreName(this.storeNameDraft)
        this.showToast('店名已保存')
      } catch (e) {
        this.showToast(String(e), true)
      }
    },
    async finishIndustrySetup() {
      if (!this.pendingIndustry) {
        this.showToast('请选择经营行业', true)
        return
      }
      if (!this.storeNameDraft.trim()) {
        this.showToast('请填写店名', true)
        return
      }
      try {
        if (this.pendingIndustry !== this.storeProfile?.industry) {
          this.storeProfile = await api.selectIndustry(this.pendingIndustry)
        }
        this.storeProfile = await api.setStoreName(this.storeNameDraft)
        await this.loadStoreProfile()
        if (this.needsActivation) {
          this.view = 'activate'
          void this.probeActivationService()
          this.showToast('行业已保存，下一步激活设备')
        } else {
          this.onboarding = false
          this.view = 'pos'
          this.showToast('初始化完成，可以开始收银')
        }
      } catch (e) {
        this.showToast(String(e), true)
      }
    },
    gotoRefund(orderNo?: string) {
      this.refundOrderNo = orderNo ?? ''
      this.view = 'refund'
    },
    gotoOrder(no: string) {
      this.orderNo = no
      this.view = 'order'
    },
    /** 订单列表：orderNo 置空让订单页进入列表模式；date 为报表跳转带入的过滤日 */
    gotoOrderList(date?: string) {
      this.orderListDate = date ?? ''
      this.orderNo = ''
      this.view = 'order'
    },
    async probeActivationService() {
      if (this.probingService) return
      this.probingService = true
      try {
        this.activationServiceReachable = await api.probeActivationService()
      } catch {
        this.activationServiceReachable = false
      } finally {
        this.probingService = false
      }
    },
    async openOfficialSite() {
      this.activateError = ''
      try {
        await api.openActivationSite()
      } catch (e) {
        this.activateError = String(e)
      }
    },
    async submitActivateOnline() {
      this.activateError = ''
      if (!this.activateToken.trim()) {
        this.activateError = '请输入激活码'
        return
      }
      try {
        this.license = await api.activateOnline(
          this.activateToken.trim(),
          this.activateCustomer.trim() || null
        )
        if (this.license.status !== 'UNACTIVATED') {
          this.setActivationPending(false)
          this.needAdmin = !(await api.hasUsers())
          if (this.needAdmin || !(await this.applyCurrentSession())) {
            this.view = 'login'
          } else {
            await this.routeAfterLogin()
          }
          this.showToast('激活成功')
        }
      } catch (e) {
        this.activationServiceReachable = false
        this.activateError = String(e)
      }
    },
    /** 设置页内激活：仅更新授权状态与提示，不切换视图（区别于首次启动的 submitActivateOnline）。 */
    async activateInSettings() {
      this.activateError = ''
      if (!this.activateToken.trim()) {
        this.activateError = '请输入激活码'
        return false
      }
      try {
        this.license = await api.activateOnline(
          this.activateToken.trim(),
          this.activateCustomer.trim() || null
        )
        if (this.license.status === 'UNACTIVATED') {
          this.activateError = '激活码无效或已过期'
          return false
        }
        this.setActivationPending(false)
        this.activateToken = ''
        this.showToast('设备已激活')
        return true
      } catch (e) {
        this.activateError = String(e)
        return false
      }
    },
    async refreshLicense() {
      this.activateError = ''
      try {
        this.license = await api.refreshLicenseOnline()
        this.showToast('授权状态已刷新')
      } catch (e) {
        this.activateError = String(e)
      }
    },
    async submitActivate() {
      this.activateError = ''
      try {
        this.license = this.renewMode
          ? await api.renew(this.activateToken)
          : await api.activate(this.activateToken)
        if (this.license.status !== 'UNACTIVATED') {
          this.setActivationPending(false)
          this.needAdmin = !(await api.hasUsers())
          if (this.needAdmin || !(await this.applyCurrentSession())) {
            this.view = 'login'
          } else {
            await this.routeAfterLogin()
          }
          this.showToast('激活成功')
        }
      } catch (e) {
        this.activateError = String(e)
      }
    },
    async applyCurrentSession(): Promise<boolean> {
      try {
        const session = await api.currentSession()
        if (session) {
          this.currentRole = this.normalizeRole(session.role_name)
          return true
        }
      } catch {
        // 未登录或会话过期：保持最低权限默认值
      }
      return false
    },
    async routeAfterLogin() {
      await this.loadStoreProfile()
      if (!(this.storeProfile?.configured ?? false)) {
        this.pendingIndustry = this.storeProfile?.industry || this.pendingIndustry || ''
        this.storeNameDraft = this.storeProfile?.store_name || this.storeNameDraft
        this.view = 'industry'
        return
      }
      if (this.needsActivation) {
        this.view = 'activate'
        void this.probeActivationService()
        return
      }
      this.onboarding = false
      this.view = 'pos'
      await this.restoreCurrentShift()
    },
    async submitAdmin() {
      this.loginError = ''
      try {
        await api.initAdmin(this.adminUser, this.adminPass)
        this.needAdmin = false
        this.setActivationPending(true)
        this.onboarding = true
        this.loginUser = this.adminUser
        this.loginPass = ''
        this.adminPass = ''
        this.view = 'login'
        this.showToast('管理员已创建，请使用该账号登录')
      } catch (e) {
        this.loginError = String(e)
      }
    },
    async submitLogin() {
      this.loginError = ''
      try {
        const session = await api.login(this.loginUser, this.loginPass)
        this.currentRole = this.normalizeRole(session.role_name)
        this.currentUser = this.loginUser
        await this.routeAfterLogin()
      } catch (e) {
        this.loginError = String(e)
      }
    },
    enterTrial() {
      if (this.license?.status !== 'TRIAL_ACTIVE') {
        this.showToast('试用期不可用，请使用激活码激活', true)
        return
      }
      this.setActivationPending(false)
      this.onboarding = false
      this.view = 'pos'
      this.showToast(`已进入试用，剩余 ${this.trialDaysLeft} 天`)
    },
    // ===== 锁机码 =====
    loadLockCode() {
      try {
        const stored = localStorage.getItem(LOCK_CODE_KEY) || ''
        this.lockCode = stored
        if (stored && !LOCK_HASH_RE.test(stored)) {
          void this.hashLockCode(stored)
            .then((digest) => {
              localStorage.setItem(LOCK_CODE_KEY, digest)
              this.lockCode = digest
            })
            .catch(() => {})
        }
      } catch {
        this.lockCode = ''
      }
    },
    async hashLockCode(code: string): Promise<string> {
      const buf = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(code))
      return Array.from(new Uint8Array(buf)).map((b) => b.toString(16).padStart(2, '0')).join('')
    },
    async saveLockCode() {
      const code = this.lockCodeDraft.trim()
      if (!code) {
        this.showToast('请输入锁机码', true)
        return
      }
      try {
        const digest = await this.hashLockCode(code)
        localStorage.setItem(LOCK_CODE_KEY, digest)
        this.lockCode = digest
        this.lockCodeDraft = ''
        this.lockSetupOpen = false
        const shouldLock = this.pendingLockAfterSetup
        this.pendingLockAfterSetup = false
        if (shouldLock) {
          this.lockInput = ''
          this.lockError = ''
          this.locked = true
        }
        this.showToast('锁机码已设置')
      } catch (e) {
        this.showToast(String(e), true)
      }
    },
    clearLockCode() {
      try {
        localStorage.removeItem(LOCK_CODE_KEY)
      } catch {
        // 忽略
      }
      this.lockCode = ''
      this.lockCodeDraft = ''
      this.showToast('锁机码已清除')
    },
    lockScreen() {
      if (this.locked) return
      this.lockInput = ''
      this.lockError = ''
      if (!this.lockCode) {
        this.pendingLockAfterSetup = true
        this.lockSetupOpen = true
        return
      }
      this.locked = true
    },
    async unlock() {
      let matched = false
      try {
        matched = LOCK_HASH_RE.test(this.lockCode)
          ? (await this.hashLockCode(this.lockInput)) === this.lockCode
          : this.lockInput === this.lockCode
      } catch {
        matched = false
      }
      if (matched) {
        this.locked = false
        this.lockInput = ''
        this.lockError = ''
        this.showToast('已解锁')
      } else {
        this.lockError = '锁机码不正确'
      }
    },
    cancelLockSetup() {
      this.lockSetupOpen = false
      this.pendingLockAfterSetup = false
    },
    applyTheme(t: 'light' | 'dark') {
      this.theme = t
      document.documentElement.dataset.theme = t
      localStorage.setItem('blcj-theme', t)
    },
    // ===== 二次授权 =====
    requireSensitiveAuth(permission: string, title: string): Promise<string> {
      return new Promise((resolve, reject) => {
        this.authDialog = { permission, title, resolve, reject }
        this.authUser = ''
        this.authPass = ''
        this.authError = ''
      })
    },
    async confirmSensitiveAuth() {
      const d = this.authDialog
      if (!d) return
      try {
        const nonce = await api.requestSensitiveAuth(d.permission, this.authUser.trim(), this.authPass)
        d.resolve?.(nonce)
        this.authDialog = null
      } catch (e) {
        this.authError = String(e)
      }
    },
    cancelSensitiveAuth() {
      const d = this.authDialog
      d?.reject?.('已取消二次授权')
      this.authDialog = null
    },
    // ===== 支付渠道（POS 与设置共用）=====
    async loadPayChannels() {
      try {
        const c = await api.getPayChannels()
        this.channelEnabled = {
          CASH: c.cash,
          WECHAT: c.wechat,
          ALIPAY: c.alipay,
          STORED_VALUE: c.stored_value,
        }
      } catch {
        this.channelEnabled = { CASH: true, WECHAT: true, ALIPAY: true, STORED_VALUE: true }
      }
      try {
        const g = await api.paymentGatewayStatus()
        this.gatewayModes = { wechat: g.wechat_mode, alipay: g.alipay_mode }
      } catch {
        this.gatewayModes = { wechat: 'LOCAL', alipay: 'LOCAL' }
      }
    },
    /** 应用重启/重新登录后恢复后端仍然 OPEN 的班次，避免关班打到 #0。 */
    async restoreCurrentShift() {
      try {
        const s = await api.currentShift()
        if (s && s.shift_id > 0) {
          this.currentShiftId = s.shift_id
          this.shiftId = s.shift_id
        }
      } catch {
        // 无收银权限或后端未开放班：保持 0，等用户开班
      }
    },
    // ===== 导航 =====
    goPOS() { this.view = 'pos'; void this.loadPayChannels() },
    goProducts() { this.goodsTab = 'list'; this.view = 'products' },
    goSettings() { this.view = 'settings' },
    async doLogout() {
      try {
        await api.logout()
      } catch (e) {
        this.showToast(`退出失败：${String(e)}`)
        return
      }
      this.view = 'login'
      this.currentUser = ''
      this.currentRole = 'CASHIER'
      this.settingsTab = 'overview'
    },
  },
})
