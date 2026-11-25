import { defineStore } from 'pinia'
import { nextTick } from 'vue'
import { convertFileSrc } from '@tauri-apps/api/core'
import {
  api,
  type ProductHit,
  type CartLine,
  type Category,
  type Technician,
  type OrderQuote,
  type ServiceTable,
  formatMoney,
} from '../api'
import {
  CENTS_PER_YUAN,
  NATIVE_POLL_INTERVAL_MS,
  PAY_CHANNEL_LABEL,
  SCAN_GAP_MS_DEFAULT,
  SCAN_MIN_LENGTH,
  SEARCH_DEBOUNCE_MS,
} from '../constants'
import { useAppStore } from './app'

type PayChannel = 'CASH' | 'STORED_VALUE' | 'WECHAT' | 'ALIPAY'
interface PaySplit { channel: string; amountYuan: string; authCode: string }
type PayMethod = { id: PayChannel; label: string; icon: 'cash' | 'wechat' | 'alipay' | 'wallet' }

/** 行成交价：识别会员后按会员价显示（结算时与促销取优由后端裁决）。 */
function effectivePriceOf(l: { unit_price_cents: number; member_price_cents?: number | null }): number {
  const mp = l.member_price_cents
  if (mp != null && mp > 0 && mp < l.unit_price_cents) return mp
  return l.unit_price_cents
}

const PAY_METHODS: PayMethod[] = [
  { id: 'CASH', label: '现金', icon: 'cash' },
  { id: 'WECHAT', label: '微信', icon: 'wechat' },
  { id: 'ALIPAY', label: '支付宝', icon: 'alipay' },
  { id: 'STORED_VALUE', label: '储值', icon: 'wallet' },
]

let tableSaveTimer: ReturnType<typeof setTimeout> | null = null
let chessFeeTimer: ReturnType<typeof setInterval> | null = null
let nativeTimer: ReturnType<typeof setInterval> | null = null
let searchTimer: ReturnType<typeof setTimeout> | null = null
let quoteSeq = 0

export const usePosStore = defineStore('pos', {
  state: () => ({
    // 桌台
    activeTable: null as ServiceTable | null,
    tableSentQty: {} as Record<number, number>,
    // 商品与搜索
    keyword: '',
    products: [] as ProductHit[],
    posCategories: [] as Category[],
    posProducts: [] as ProductHit[],
    posCategoryId: null as number | null,
    posCatalogBusy: false,
    searchBusy: false,
    scanBuffer: '',
    scanTimes: [] as number[],
    scanGapMs: SCAN_GAP_MS_DEFAULT,
    scanCapture: null as HTMLInputElement | null,
    // 购物车与报价
    cart: [] as (CartLine & { name: string })[],
    quote: null as OrderQuote | null,
    technicians: [] as Technician[],
    cashTendered: '',
    lastOrder: '',
    paying: false,
    // 会员与券
    memberPhone: '',
    memberId: null as number | null,
    memberBalances: null as null | { principal_cents: number; bonus_cents: number; points: number },
    memberFindError: '',
    couponCode: '',
    couponDialogOpen: false,
    couponDraft: '',
    couponDialogInput: null as HTMLInputElement | null,
    memberDialogOpen: false,
    memberDialogInput: null as HTMLInputElement | null,
    // 支付
    paySplits: [{ channel: 'CASH', amountYuan: '', authCode: '' }] as PaySplit[],
    payChannel: 'CASH' as PayChannel,
    payMode: 'simple' as 'simple' | 'mixed',
    cashDialogOpen: false,
    cashDialogInput: null as HTMLInputElement | null,
    payAuthCode: '',
    payCodeInput: null as HTMLInputElement | null,
    nativeDialog: null as null | { orderNo: string; codeUrl: string; qrSvg: string },
    nativeMode: false,
    // 挂单
    parked: [] as unknown[],
    parkDialogOpen: false,
    // 弹窗聚焦
  }),
  getters: {
    /** 台位看板阶段：餐饮/棋牌/养生未选中台位时，首屏只显示看板（新建/进入后再点单收银） */
    sceneBoardStage(state): boolean {
      const app = useAppStore()
      return (
        (app.industry === 'RESTAURANT' || app.industry === 'BOARD_GAME' || app.industry === 'WELLNESS') &&
        !state.activeTable
      )
    },
    /** 养生/母婴以场景看板为唯一浏览入口，隐藏通用商品分类（避免双份点单 UI） */
    sceneCatalogOnly(state): boolean {
      const app = useAppStore()
      return app.industry === 'BABY' || app.industry === 'WELLNESS'
    },
    posRootCategories(state): Category[] {
      return state.posCategories.filter((c) => c.parent_id == null)
    },
    onDutyTechnicians(state): Technician[] {
      return state.technicians.filter((t) => t.on_duty)
    },
    subtotal(state): number {
      return state.cart.reduce((s, l) => s + l.qty * effectivePriceOf(l), 0)
    },
    payableAmount(state): number {
      return state.quote?.payable_cents ?? this.subtotal
    },
    paidPreview(state): number {
      return state.paySplits.reduce((s, p) => s + (Number(p.amountYuan) || 0) * CENTS_PER_YUAN, 0)
    },
    payShortfall(): number {
      return this.payableAmount - this.paidPreview
    },
    payMethods(): PayMethod[] {
      const app = useAppStore()
      return PAY_METHODS.filter((m) => app.channelEnabled[m.id] !== false)
    },
    isLocalGateway(): boolean {
      const app = useAppStore()
      return app.gatewayModes.wechat === 'LOCAL' && app.gatewayModes.alipay === 'LOCAL'
    },
    cashTenderedCents(state): number {
      return Math.round(Number(state.cashTendered || '0') * CENTS_PER_YUAN)
    },
    changeDue(): number {
      return Math.max(0, this.cashTenderedCents - this.payableAmount)
    },
    cashShort(): number {
      return Math.max(0, this.payableAmount - this.cashTenderedCents)
    },
    cashOverpaid(): boolean {
      return this.cashTenderedCents > 0 && this.cashTenderedCents >= this.payableAmount
    },
    couponRecoverable(state): boolean {
      const reason = state.quote?.coupon_reason
      return reason === 'BELOW_THRESHOLD' || reason === 'MEMBER_MISMATCH'
    },
    couponHint(state): string {
      const q = state.quote
      if (!q?.coupon_note) return ''
      if (q.coupon_reason === 'BELOW_THRESHOLD' && q.coupon_shortfall_cents != null) {
        return `还差 ¥${formatMoney(q.coupon_shortfall_cents)} 可用券，加购后自动生效 · 不用请清除券`
      }
      return q.coupon_note
    },
  },
  actions: {
    // ===== 通用 =====
    showToast(msg: string, isError = false) {
      useAppStore().showToast(msg, isError)
    },
    showSceneError(message: string) {
      this.showToast(message, true)
    },
    pointsToast(base: string, points: number | undefined): string {
      return points && points > 0 ? `${base}，积分 +${points}` : base
    },
    effectivePrice(l: { unit_price_cents: number; member_price_cents?: number | null }): number {
      return effectivePriceOf(l)
    },
    memberPriceOf(p: ProductHit): number | null {
      if (this.memberId == null) return null
      const mp = p.member_price_cents
      if (mp == null || mp <= 0 || mp >= p.price_cents) return null
      return mp
    },
    productImageSrc(p: { image_path?: string | null; image_url?: string | null }): string | null {
      if (p.image_path) return convertFileSrc(p.image_path)
      if (p.image_url) return p.image_url
      return null
    },
    posDialogOpen(): boolean {
      const app = useAppStore()
      return (
        app.lockSetupOpen ||
        this.memberDialogOpen ||
        this.couponDialogOpen ||
        this.cashDialogOpen ||
        this.parkDialogOpen ||
        !!app.authDialog ||
        !!this.nativeDialog
      )
    },

    // ===== 桌台 =====
    scheduleTableSave() {
      if (!this.activeTable) return
      if (tableSaveTimer) clearTimeout(tableSaveTimer)
      tableSaveTimer = setTimeout(() => {
        tableSaveTimer = null
        void this.flushTableCart()
      }, 400)
    },
    async flushTableCart(): Promise<boolean> {
      // 未开台桌台（餐饮进入未点菜）没有会话，不保存
      if (!this.activeTable || this.activeTable.status !== 'OCCUPIED') return true
      try {
        await api.saveServiceTableCart(this.activeTable.table_id, JSON.stringify(this.cart))
        return true
      } catch (e) {
        this.showToast(`桌台购物车保存失败：${String(e)}`, true)
        return false
      }
    },
    async selectSceneTable(table: ServiceTable, lines: CartLine[]) {
      if (this.activeTable && this.activeTable.table_id !== table.table_id) {
        if (!(await this.flushTableCart())) {
          this.showToast('原桌台购物车未保存，已取消切换桌台', true)
          return
        }
      }
      if (tableSaveTimer) { clearTimeout(tableSaveTimer); tableSaveTimer = null }
      this.activeTable = table
      this.couponCode = ''
      this.clearMember()
      this.cart = lines.map((l) => ({ ...l, name: l.name || `商品 ${l.sku_id}`, member_price_cents: null }))
      if (table.member_id && table.member_phone) {
        this.memberPhone = table.member_phone
        await this.findMember()
      }
      if (table.scene === 'CHESS' || table.scene === 'WELLNESS') await this.refreshChessFee()
      this.showToast(`已选中 ${table.code}${lines.length ? '，继续点单' : '，可以点单'}`)
    },
    async refreshChessFee() {
      const table = this.activeTable
      if (!table || (table.scene !== 'CHESS' && table.scene !== 'WELLNESS') || !table.fee_sku_id) {
        this.stopChessFeeTimer()
        return
      }
      try {
        const fee = await api.chessTableFee(table.table_id)
        const others = this.cart.filter((l) => !l.is_chess_fee)
        this.cart = [
          ...others,
          {
            sku_id: table.fee_sku_id, qty: 1, unit_price_cents: fee.fee_cents,
            name: `包间费 ${fee.billable_minutes} 分钟（${fee.fee_unit === 'HOUR' ? '按小时价折算' : '按分钟价'}）`,
            is_chess_fee: true, chess_minutes: fee.billable_minutes,
          },
        ]
        if (!chessFeeTimer) chessFeeTimer = setInterval(() => void this.refreshChessFee(), 30000)
      } catch (e) {
        this.showToast(String(e), true)
      }
    },
    stopChessFeeTimer() {
      if (chessFeeTimer) clearInterval(chessFeeTimer)
      chessFeeTimer = null
    },
    async clearActiveTable() {
      if (!this.activeTable) return
      if (tableSaveTimer) clearTimeout(tableSaveTimer)
      tableSaveTimer = null
      this.stopChessFeeTimer()
      this.activeTable = null
      this.cart = []
      this.clearMember()
    },
    async backToDiningTables() {
      if (!(await this.flushTableCart())) {
        this.showToast('当前桌台购物车未保存，已取消返回', true)
        return
      }
      await this.clearActiveTable()
    },
    async releaseActiveTable(): Promise<string | null> {
      const table = this.activeTable
      if (!table) return null
      this.activeTable = null
      this.stopChessFeeTimer()
      if (tableSaveTimer) { clearTimeout(tableSaveTimer); tableSaveTimer = null }
      try {
        await api.closeServiceTable(table.table_id, false)
        return null
      } catch (e) {
        return `订单已完成，但桌台 ${table.code} 清台失败：${String(e)}，请手动清台`
      }
    },
    onIndustryChange() {
      // 切换行业后阻断旧行业信息：台位会话、购物车、POS 目录全部复位重载
      this.activeTable = null
      this.stopChessFeeTimer()
      this.cart = []
      this.tableSentQty = {}
      this.posCategoryId = null
      this.posProducts = []
      void this.loadPosCatalog()
      void this.loadTechnicians()
      if (tableSaveTimer) { clearTimeout(tableSaveTimer); tableSaveTimer = null }
    },

    // ===== 会员 =====
    clearMember() {
      this.memberPhone = ''
      this.memberId = null
      this.memberBalances = null
      for (const l of this.cart) l.member_price_cents = null
    },
    async findMember(): Promise<boolean> {
      const phone = this.memberPhone.trim()
      if (!phone) {
        this.memberId = null
        this.memberBalances = null
        for (const l of this.cart) l.member_price_cents = null
        this.memberFindError = '请输入手机号或会员码'
        return false
      }
      try {
        const list = await api.listMembers(phone, 10)
        const m = list.find((x) => x.phone === phone || x.member_code === phone)
        if (!m) {
          this.memberId = null
          this.memberBalances = null
          for (const l of this.cart) l.member_price_cents = null
          this.memberFindError = `未找到会员「${phone}」，请核对后重试`
          return false
        }
        this.memberFindError = ''
        this.memberId = m.member_id
        this.memberBalances = { principal_cents: m.principal_cents, bonus_cents: m.bonus_cents, points: m.points }
        await this.refreshCartMemberPrices()
        this.showToast(`会员已识别：${m.name ?? m.member_code}`)
        return true
      } catch (e) {
        this.memberFindError = `识别失败：${String(e)}`
        return false
      }
    },
    async refreshCartMemberPrices() {
      if (!this.cart.length) return
      try {
        const rows = await api.memberPrices(this.cart.map((l) => l.sku_id))
        const map = new Map(rows.map((r) => [r.sku_id, r.member_price_cents]))
        for (const l of this.cart) l.member_price_cents = map.get(l.sku_id) ?? null
      } catch {
        // 会员价刷新失败不阻断收银
      }
    },
    async rebindTableMember() {
      const table = this.activeTable
      if (!table) return
      if (this.memberId == null) {
        this.showToast('请先识别要换绑的会员', true)
        return
      }
      if (!window.confirm(`把桌台 ${table.code} 换绑到当前会员？`)) return
      try {
        const updated = await api.rebindServiceTableMember(table.table_id, this.memberId)
        this.activeTable = updated
        this.showToast(`已换绑会员 ${updated.member_phone ?? ''}`)
      } catch (e) {
        this.showToast(`换绑失败：${String(e)}`, true)
      }
    },
    openMemberDialog() {
      this.memberFindError = ''
      this.memberDialogOpen = true
    },
    async confirmMemberDialog() {
      if (await this.findMember()) this.memberDialogOpen = false
    },

    // ===== 扫码与商品 =====
    onScanKeydown(e: KeyboardEvent) {
      const now = Date.now()
      if (e.key === 'Enter') {
        const times = this.scanTimes
        const code = this.scanBuffer.trim()
        const fast = code.length >= SCAN_MIN_LENGTH && times.length >= 2 && now - times[0] < (times.length * this.scanGapMs)
        this.scanBuffer = ''
        this.scanTimes = []
        if (fast) {
          e.preventDefault()
          void this.handleScannedCode(code)
        } else if (code) {
          e.preventDefault()
          this.showToast('未匹配到精确条码，请从候选列表选择商品', true)
        }
        return
      }
      if (e.key.length === 1) {
        this.scanBuffer += e.key
        this.scanTimes.push(now)
      } else if (e.key === 'Backspace') {
        this.scanBuffer = this.scanBuffer.slice(0, -1)
      }
    },
    async handleScannedCode(code: string) {
      if (this.cart.length && this.payShortfall > 0) {
        const channel = await api.detectPaymentCode(code)
        if (channel) {
          await this.applyPaymentCode(channel, code)
          return
        }
      }
      if (/^2[24]\d{11}$/.test(code)) {
        try {
          const hit = await api.lookupWeigh(code)
          this.pushWeighToCart(hit)
          return
        } catch { /* 退回普通条码识别 */ }
      }
      this.keyword = code
      if (searchTimer) { clearTimeout(searchTimer); searchTimer = null }
      await this.search()
    },
    async applyPaymentCode(channel: string, code: string) {
      await this.refreshQuote()
      if (this.payShortfall <= 0) return
      this.payMode = 'simple'
      this.payChannel = channel as PayChannel
      this.payAuthCode = code
      this.nativeMode = false
      this.showToast(`已识别${PAY_CHANNEL_LABEL[channel] ?? channel}付款码，正在收款`)
      await this.submitCheckout()
    },
    pushWeighToCart(hit: { sku_id: number; product_name: string; qty: number; unit_price_cents: number; line_total_cents: number }) {
      this.cart.push({ sku_id: hit.sku_id, qty: hit.qty, unit_price_cents: hit.unit_price_cents, name: hit.product_name })
      this.showToast(`已加入称重商品：${hit.product_name}`)
    },
    focusScan() {
      this.scanCapture?.focus()
    },
    async search() {
      if (!this.keyword.trim()) {
        this.products = []
        this.searchBusy = false
        return
      }
      try {
        this.products = await api.lookup(this.keyword.trim())
      } catch (e) {
        this.showToast(String(e))
      } finally {
        this.searchBusy = false
      }
    },
    onKeywordChange(val: string) {
      if (searchTimer) clearTimeout(searchTimer)
      if (!val.trim()) {
        this.products = []
        this.searchBusy = false
        return
      }
      this.searchBusy = true
      searchTimer = setTimeout(() => void this.search(), SEARCH_DEBOUNCE_MS)
    },
    setScanGapMs(ms: number) {
      if (Number.isFinite(ms) && ms > 0) this.scanGapMs = ms
    },
    async loadPosCategory(categoryId: number | null) {
      this.posCategoryId = categoryId
      try {
        this.posProducts = await api.posProducts(categoryId)
      } catch (e) {
        this.posProducts = []
        this.showToast(String(e), true)
      }
    },
    async loadPosCatalog() {
      if (this.posCatalogBusy) return
      this.posCatalogBusy = true
      try {
        this.posCategories = await api.posCategories()
        const roots = this.posCategories.filter((c) => c.parent_id == null)
        const current = roots.find((c) => c.category_id === this.posCategoryId)
        const fallback = roots.find((c) => c.category_id !== 1) ?? roots[0]
        await this.loadPosCategory(current?.category_id ?? fallback?.category_id ?? null)
      } catch (e) {
        this.showToast(String(e), true)
      } finally {
        this.posCatalogBusy = false
      }
    },
    selectPosCategory(categoryId: number) {
      if (categoryId === this.posCategoryId) return
      void this.loadPosCategory(categoryId)
    },
    addFromCatalog(p: ProductHit) {
      if (p.is_weighed) {
        this.showToast('称重商品请用称重码扫码入车，避免按件误算', true)
        this.focusScan()
        return
      }
      this.addToCart(p)
    },
    async addToCart(p: ProductHit) {
      // 餐饮：进入未开台桌台只浏览，加入第一道菜时才真正开台占用
      if (this.activeTable && this.activeTable.scene === 'DINING' && this.activeTable.status === 'FREE') {
        try {
          this.activeTable = await api.openServiceTable(this.activeTable.table_id, '[]', this.memberId)
        } catch (e) {
          this.showToast(`开台失败：${String(e)}`, true)
          return
        }
      }
      const existing = this.cart.find((l) => l.sku_id === p.sku_id)
      if (existing?.is_chess_fee) {
        this.showToast('包间房费按计时自动生成，无需手动添加', true)
        return
      }
      if (existing) existing.qty += 1
      else this.cart.push({
        sku_id: p.sku_id, qty: 1, unit_price_cents: p.price_cents,
        member_price_cents: this.memberPriceOf(p), name: p.spu_name,
        image_path: p.image_path, image_url: p.image_url,
      })
      this.keyword = ''
      this.products = []
    },
    changeQty(index: number, delta: number) {
      const line = this.cart[index]
      if (!line || line.is_chess_fee) return
      const sent = this.tableSentQty[line.sku_id] || 0
      if (delta < 0 && line.qty - 1 < sent) {
        this.showToast(`该菜已下厨 ${sent} 份，减菜请用「退菜」`, true)
        return
      }
      line.qty += delta
      if (line.qty <= 0) this.cart.splice(index, 1)
    },
    setLineQty(index: number, raw: string) {
      const line = this.cart[index]
      if (!line || line.is_chess_fee) return
      const qty = Math.floor(Number(raw))
      const sent = this.tableSentQty[line.sku_id] || 0
      if (!Number.isFinite(qty) || qty <= 0) {
        if (sent > 0) {
          this.showToast(`该菜已下厨 ${sent} 份，减菜请用「退菜」`, true)
          return
        }
        this.cart.splice(index, 1)
        return
      }
      if (qty < sent) {
        this.showToast(`该菜已下厨 ${sent} 份，不能改成 ${qty}，减菜请用「退菜」`, true)
        return
      }
      line.qty = qty
    },
    clearCart() {
      if (!this.cart.length) return
      if (this.activeTable?.scene === 'CHESS' || this.activeTable?.scene === 'WELLNESS') {
        this.showToast('包间房费按计时自动生成，不能清空购物车', true)
        return
      }
      if (Object.keys(this.tableSentQty).length) {
        this.showToast('本桌已有下厨菜品，请逐项退菜后再清空', true)
        return
      }
      if (!window.confirm('清空当前购物车的全部商品？')) return
      this.cart = []
      this.showToast('已清空购物车')
    },

    // ===== 报价 =====
    async refreshQuote(opts: { silent?: boolean } = {}): Promise<boolean> {
      if (!this.cart.length) {
        this.quote = null
        return false
      }
      const seq = ++quoteSeq
      try {
        const q = await api.quoteOrder(
          this.cart, this.memberId, this.couponCode.trim() || null, this.activeTable?.table_id ?? null
        )
        if (seq !== quoteSeq) return false
        this.quote = q
        return true
      } catch (e) {
        if (seq !== quoteSeq) return false
        this.quote = null
        if (!opts.silent) this.showToast(`应收金额计算失败：${String(e)}`, true)
        return false
      }
    },

    // ===== 券 =====
    openCouponDialog() {
      this.couponDraft = this.couponCode
      this.couponDialogOpen = true
    },
    async applyCouponDialog() {
      const code = this.couponDraft.trim()
      if (!code) {
        this.showToast('请输入优惠券码', true)
        return
      }
      this.couponCode = code
      this.couponDialogOpen = false
      const ok = await this.refreshQuote({ silent: true })
      if (!ok) {
        this.couponCode = ''
        this.couponDraft = ''
        this.showToast('优惠券校验失败，已取消使用', true)
        return
      }
      const reason = this.quote?.coupon_reason
      if (this.couponRecoverable) {
        this.showToast(this.couponHint)
        return
      }
      if (reason) {
        const note = this.quote?.coupon_note ?? '优惠券不可用'
        this.couponCode = ''
        this.couponDraft = ''
        void this.refreshQuote({ silent: true })
        this.showToast(`${note}，已取消使用`, true)
        return
      }
      this.showToast('优惠券已应用')
    },
    clearCoupon() {
      this.couponCode = ''
      this.couponDraft = ''
      void this.refreshQuote()
    },

    // ===== 支付 =====
    resetPayState() {
      this.cashTendered = ''
      this.couponCode = ''
      this.couponDraft = ''
      this.payAuthCode = ''
      this.nativeMode = false
      this.payMode = 'simple'
      this.payChannel = 'CASH'
      this.paySplits = [{ channel: 'CASH', amountYuan: '', authCode: '' }]
    },
    resetPaySplits() {
      this.paySplits = [{ channel: 'CASH', amountYuan: '', authCode: '' }]
    },
    addPaySplit() {
      this.paySplits.push({ channel: 'STORED_VALUE', amountYuan: '', authCode: '' })
    },
    removePaySplit(index: number) {
      if (this.paySplits.length > 1) this.paySplits.splice(index, 1)
    },
    enterMixedMode() {
      this.payMode = 'mixed'
      this.resetPaySplits()
    },
    selectPayChannel(id: PayChannel) {
      this.payMode = 'simple'
      this.resetPaySplits()
      this.payChannel = id
      if (id === 'CASH' && this.cart.length) {
        this.cashTendered = ''
        this.cashDialogOpen = true
      }
    },
    sanitizeCashInput(raw: string): string {
      let s = raw.replace(/[^\d.]/g, '')
      const dot = s.indexOf('.')
      if (dot >= 0) s = `${s.slice(0, dot)}.${s.slice(dot + 1).replace(/\./g, '').slice(0, 2)}`
      s = s.replace(/^0+(?=\d)/, '')
      return s.slice(0, 9)
    },
    onCashInput(e: Event) {
      const el = e.target as HTMLInputElement
      const s = this.sanitizeCashInput(el.value)
      this.cashTendered = s
      if (el.value !== s) el.value = s
    },
    pressCashKey(key: string) {
      if (key === '退格') {
        this.cashTendered = this.cashTendered.slice(0, -1)
        return
      }
      if (key === '.') {
        if (!this.cashTendered) this.cashTendered = '0.'
        else if (!this.cashTendered.includes('.')) this.cashTendered += '.'
        return
      }
      const cur = this.cashTendered
      if (cur.includes('.') && (cur.split('.')[1]?.length ?? 0) >= 2) return
      this.cashTendered = this.sanitizeCashInput(cur === '0' ? key : cur + key)
    },
    exactCash() {
      this.cashTendered = (this.payableAmount / CENTS_PER_YUAN).toFixed(2)
    },
    async submitCheckout() {
      if (!this.cart.length) {
        this.showToast('购物车为空', true)
        return
      }
      if (this.paying) return
      if (this.couponRecoverable) {
        this.showToast(this.couponHint, true)
        return
      }
      if (this.payMode === 'simple' && this.payChannel === 'CASH' && !this.cashDialogOpen) {
        this.cashTendered = ''
        this.cashDialogOpen = true
        return
      }
      this.paying = true
      try {
        let payments: Array<{
          channel: string; amount_cents: number; tendered_cents: number | null
          change_cents: number | null; auth_code: string | null; mode: string | null
        }>
        if (this.payMode === 'simple') {
          const ch = this.payChannel
          const amount = this.payableAmount
          if (amount < 0) {
            this.showToast('支付金额异常（负数），请刷新报价', true)
            return
          }
          if (amount === 0) {
            payments = []
          } else if (ch === 'WECHAT' || ch === 'ALIPAY') {
            if (!this.nativeMode && !this.payAuthCode.trim()) {
              this.showToast('请扫描或输入顾客付款码', true)
              return
            }
            payments = [{
              channel: ch, amount_cents: amount, tendered_cents: null, change_cents: null,
              auth_code: this.nativeMode ? null : this.payAuthCode.trim(),
              mode: this.nativeMode ? 'NATIVE' : null,
            }]
          } else if (ch === 'CASH') {
            const t = Math.round(Number(this.cashTendered || '0') * CENTS_PER_YUAN)
            if (amount > 0 && t <= 0) {
              this.showToast('请输入实收现金金额', true)
              return
            }
            if (t > 0 && t < amount) {
              this.showToast('实收现金不足应收金额', true)
              return
            }
            payments = [{
              channel: 'CASH', amount_cents: amount,
              tendered_cents: t > 0 ? t : null, change_cents: t > 0 ? t - amount : null,
              auth_code: null, mode: null,
            }]
          } else {
            if (this.memberId == null) {
              this.showToast('储值支付需先识别会员', true)
              return
            }
            payments = [{
              channel: 'STORED_VALUE', amount_cents: amount,
              tendered_cents: null, change_cents: null, auth_code: null, mode: null,
            }]
          }
        } else {
          payments = this.paySplits.filter((s) => Number(s.amountYuan) > 0).map((s) => {
            const amount = Math.round(Number(s.amountYuan) * CENTS_PER_YUAN)
            const isScan = s.channel === 'WECHAT' || s.channel === 'ALIPAY'
            let tendered = amount
            let change = 0
            if (s.channel === 'CASH') {
              const t = Math.round(Number(this.cashTendered || '0') * CENTS_PER_YUAN)
              if (t > 0) {
                if (t < amount) throw new Error('实收现金不足以支付该笔金额')
                tendered = t
                change = t - amount
              }
            }
            return {
              channel: s.channel, amount_cents: amount,
              tendered_cents: s.channel === 'CASH' ? tendered : null,
              change_cents: s.channel === 'CASH' ? change : null,
              auth_code: isScan && !this.nativeMode ? s.authCode || null : null,
              mode: isScan && this.nativeMode ? 'NATIVE' : null,
            }
          })
          const sum = payments.reduce((s, p) => s + p.amount_cents, 0)
          if (sum !== (this.quote?.payable_cents ?? -1)) {
            this.showToast(
              `组合支付合计 ${(sum / CENTS_PER_YUAN).toFixed(2)} 元与应收 ${((this.quote?.payable_cents ?? 0) / CENTS_PER_YUAN).toFixed(2)} 元不一致`,
              true
            )
            return
          }
          if (payments.some((p) => p.channel === 'STORED_VALUE') && this.memberId == null) {
            this.showToast('储值支付需先识别会员', true)
            return
          }
          if (!payments.length) {
            this.showToast('请填写支付金额', true)
            return
          }
        }
        const r = await api.checkout(
          this.cart, payments, this.memberId, this.couponCode.trim() || null,
          this.activeTable?.code ?? null, this.activeTable?.table_id ?? null
        )
        this.lastOrder = r.order_no
        if (r.status === 'PENDING_PAYMENT' && r.code_url) {
          const svg = await api.qrSvg(r.code_url)
          this.nativeDialog = { orderNo: r.order_no, codeUrl: r.code_url, qrSvg: svg }
          this.startNativePolling(r.order_no)
          return
        }
        const tableWarn = await this.releaseActiveTable()
        this.clearMember()
        this.cart = []
        this.resetPayState()
        this.cashDialogOpen = false
        this.showToast(
          this.pointsToast(`收银成功：${r.order_no}`, r.points_earned) + (tableWarn ? `；${tableWarn}` : ''),
          !!tableWarn
        )
      } catch (e) {
        this.showToast(String(e), true)
      } finally {
        this.paying = false
      }
    },

    // ===== Native 支付轮询 =====
    startNativePolling(orderNo: string) {
      this.stopNativePolling()
      nativeTimer = setInterval(async () => {
        try {
          const r = await api.pollNativePay(orderNo)
          if (r.status === 'COMPLETED') {
            this.stopNativePolling()
            this.closeNativeDialog()
            const tableWarn = await this.releaseActiveTable()
            this.clearMember()
            this.cart = []
            this.resetPayState()
            this.showToast(
              this.pointsToast(`收银成功：${orderNo}`, r.points_earned) + (tableWarn ? `；${tableWarn}` : ''),
              !!tableWarn
            )
          } else if (r.status === 'CLOSED' || r.status.startsWith('FAILED')) {
            this.stopNativePolling()
            this.closeNativeDialog()
            this.showToast(`扫码支付未完成（${r.status === 'CLOSED' ? '订单已关闭' : '支付失败'}），订单可在「订单作废/退货」中处理`, true)
          }
        } catch (e) {
          this.showToast(String(e), true)
        }
      }, NATIVE_POLL_INTERVAL_MS)
    },
    stopNativePolling() {
      if (nativeTimer) { clearInterval(nativeTimer); nativeTimer = null }
    },
    closeNativeDialog() {
      this.nativeDialog = null
      this.nativeMode = false
    },
    async cancelNative() {
      const d = this.nativeDialog
      if (!d) return
      this.stopNativePolling()
      try {
        await api.cancelNativePay(d.orderNo)
        this.showToast('已取消 Native 支付，订单可在订单操作中作废', true)
      } catch (e) {
        this.showToast(String(e), true)
      }
      this.closeNativeDialog()
    },
    async confirmNativeLocal() {
      const d = this.nativeDialog
      if (!d) return
      this.stopNativePolling()
      try {
        const points = await api.confirmNativePay(d.orderNo)
        this.closeNativeDialog()
        const tableWarn = await this.releaseActiveTable()
        this.clearMember()
        this.cart = []
        this.resetPayState()
        this.showToast(
          this.pointsToast(`收银成功：${d.orderNo}`, points) + (tableWarn ? `；${tableWarn}` : ''),
          !!tableWarn
        )
      } catch (e) {
        this.showToast(String(e), true)
      }
    },

    // ===== 桌台下单/退菜 =====
    async submitTableOrder() {
      const table = this.activeTable
      if (!table || table.scene !== 'DINING') return
      if (!this.cart.length) {
        this.showToast('购物车为空', true)
        return
      }
      if (!(await this.flushTableCart())) {
        this.showToast('购物车未保存，未执行下单', true)
        return
      }
      try {
        const result = await api.submitServiceTableOrder(table.table_id, 'DINING')
        if (!result.items.length) {
          this.showToast(result.print_message || '没有新增菜品需要下单')
          return
        }
        this.activeTable = { ...table, sent_json: JSON.stringify(this.cart) }
        this.showToast(`已下单 ${result.items.length} 类菜品；${result.print_message}`)
      } catch (e) {
        this.showToast(`下单未完成：${String(e)}`, true)
      }
    },
    async returnTableLine(line: CartLine) {
      const table = this.activeTable
      if (!table || table.scene !== 'DINING') return
      const sent = this.tableSentQty[line.sku_id] || 0
      if (sent <= 0) {
        this.showToast('该菜尚未下单，直接减少数量即可', true)
        return
      }
      const raw = window.prompt(`退菜数量（已下单 ${sent} 份）`, '1')
      if (raw == null) return
      const qty = Math.floor(Number(raw))
      if (!Number.isFinite(qty) || qty <= 0) {
        this.showToast('退菜数量必须大于 0', true)
        return
      }
      try {
        const updated = await api.returnServiceTableLine(table.table_id, 'DINING', line.sku_id, qty, '前台退菜')
        this.activeTable = updated
        this.cart = (JSON.parse(updated.cart_json || '[]') as CartLine[]).map((l) => ({
          ...l, name: l.name || `商品 ${l.sku_id}`, member_price_cents: null,
        }))
        this.showToast(`已退 ${qty} 份，桌台与后厨已同步`)
      } catch (e) {
        this.showToast(`退菜失败：${String(e)}`, true)
      }
    },

    // ===== 技师 =====
    async loadTechnicians() {
      try {
        this.technicians = await api.listTechnicians(localDateString())
      } catch (e) {
        this.technicians = []
        this.showToast(String(e), true)
      }
    },

    // ===== 挂单 =====
    async park() {
      if (!this.cart.length) {
        this.showToast('购物车为空，无需挂单', true)
        return
      }
      try {
        await api.parkCart(JSON.stringify(this.cart), null)
        this.cart = []
        this.showToast('已挂单')
        if (this.parkDialogOpen) await this.loadParked()
      } catch (e) {
        this.showToast(String(e), true)
      }
    },
    async loadParked() {
      try {
        this.parked = await api.listParked()
      } catch (e) {
        this.showToast(String(e), true)
      }
    },
    async resume(parkId: number) {
      try {
        const json = await api.resumeCart(parkId)
        const parsed = JSON.parse(json) as Array<Record<string, unknown>>
        for (const line of parsed) delete line.member_price_cents
        this.cart = parsed as unknown as typeof this.cart
        this.parkDialogOpen = false
        useAppStore().view = 'pos'
        this.showToast('已取单，促销将现场重算')
      } catch (e) {
        this.showToast(String(e), true)
      }
    },
    async openParkDialog() {
      this.parkDialogOpen = true
      await this.loadParked()
    },
    async printReceipt() {
      if (!this.lastOrder) return
      try {
        const r = await api.printReceipt(this.lastOrder)
        this.showToast(r)
      } catch (e) {
        this.showToast(String(e))
      }
    },

    // ===== 折现等交互辅助（保留供模板调用）=====
    onPayChannelFocusWatch() {
      // 由视图层的 watch 触发，聚焦付款码输入
      void nextTick(() => this.payCodeInput?.focus())
    },
    onCashDialogOpenWatch() {
      void nextTick(() => this.cashDialogInput?.focus())
    },
    onMemberDialogOpenWatch() {
      void nextTick(() => this.memberDialogInput?.focus())
    },
    onCouponDialogOpenWatch() {
      void nextTick(() => this.couponDialogInput?.focus())
    },
  },
})

function localDateString(d = new Date()): string {
  const y = d.getFullYear()
  const m = String(d.getMonth() + 1).padStart(2, '0')
  const day = String(d.getDate()).padStart(2, '0')
  return `${y}-${m}-${day}`
}
