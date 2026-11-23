import { defineStore } from 'pinia'
import {
  api,
  formatMoney,
  today,
  type ConfigRow,
  type ReportMeta,
  type ReportResult,
  type OrderItemRow,
} from '../api'
import { CENTS_PER_YUAN, PAY_CRED_FIELDS } from '../constants'
import { useAppStore } from './app'
import { usePosStore } from './pos'

interface ShiftRow { cashier_id: number; order_count: number; total_cents: number; pay_cents: number; cash_net_cents: number }
interface UserRowUI { user_id: number; username: string; role_name: string; status: number }

const POINTS_CONFIG_KEYS = ['points_enabled', 'points_per_yuan', 'points_redeem_rate']
const PAY_CHANNEL_CONFIG_KEYS = ['pay_channel_cash_enabled', 'pay_channel_wechat_enabled', 'pay_channel_alipay_enabled', 'pay_channel_sv_enabled']
const SYSTEM_CONFIG_GROUPS: Array<{ title: string; hint: string; keys: string[] }> = [
  { title: '营业规则', hint: '影响营业日切与收银基础规则', keys: ['store_timezone', 'business_day_cutoff', 'max_discount_ratio', 'allow_negative_stock_default'] },
  { title: '支付时序', hint: '付款码 / 主扫的轮询与超时参数', keys: ['micropay_poll_ms', 'micropay_timeout_s', 'native_poll_ms', 'native_timeout_s'] },
  { title: '收银设备', hint: '扫码枪等外设参数', keys: ['scan_gap_ms'] },
  { title: '经营费用', hint: '利润报表按天均摊的月度固定费用', keys: ['expense_rent_yuan', 'expense_utility_yuan', 'expense_labor_yuan'] },
  { title: '打印设置', hint: '网络打印机地址与结算自动打印', keys: ['print_host', 'print_port', 'print_auto'] },
  { title: '行业规则', hint: '棋牌房费与桌台会员策略，默认值可被单包间覆盖', keys: ['chess_fee_unit', 'chess_min_minutes', 'chess_round_minutes', 'table_member_bind', 'table_allow_member_rebind'] },
  { title: '激活服务', hint: '在线激活服务地址，留空使用内置服务', keys: ['activation_endpoint'] },
]

type ReportTool = 'shift' | 'orderOp'

export const useOpsStore = defineStore('ops', {
  state: () => ({
    // 交班
    shiftRows: [] as ShiftRow[],
    openingFloat: '0',
    actualCash: '0',
    differenceReason: '',
    // 报表
    reportMetas: [] as ReportMeta[],
    activeReportId: 'sales_daily',
    activeReport: null as ReportResult | null,
    reportLoading: false,
    centerFrom: today(),
    centerTo: today(),
    reportTool: null as ReportTool | null,
    // 系统配置
    configs: [] as ConfigRow[],
    configDraft: {} as Record<string, string>,
    // 支付设置
    credChannel: 'WECHAT' as 'WECHAT' | 'ALIPAY',
    credDraft: {} as Record<string, string>,
    credSaved: {} as Record<string, boolean>,
    credBusy: false,
    channelDraft: {} as Record<string, string>,
    // 对账
    reconDate: today(),
    reconText: '',
    reconSummary: null as null | { matched: number; missing_remote: number; missing_local: number; amount_mismatch: number; status_mismatch: number },
    reconDiffs: [] as {
      recon_id: number; channel: string; diff_type: string; order_no: string | null
      local_amount_cents: number; channel_amount_cents: number; detail: string | null; created_at: string
    }[],
    // 打印队列
    printQueue: [] as string[],
    // 小票模板设置
    receiptTemplateJson: '',
    receiptPaperWidth: 58,
    receiptPreview: '',
    // 订单操作
    opOrderNo: '',
    opItems: [] as OrderItemRow[],
    opQty: {} as Record<number, string>,
    opMode: 'refund' as 'void' | 'refund',
    // 员工
    users: [] as UserRowUI[],
    uForm: { username: '', password: '', roleName: 'CASHIER' },
  }),
  getters: {
    reportGroups(state): Array<{ name: string; items: ReportMeta[] }> {
      const groups: Array<{ name: string; items: ReportMeta[] }> = []
      for (const meta of state.reportMetas) {
        let g = groups.find((x) => x.name === meta.group)
        if (!g) {
          g = { name: meta.group, items: [] }
          groups.push(g)
        }
        g.items.push(meta)
      }
      return groups
    },
    pointsConfigs(state): ConfigRow[] {
      return POINTS_CONFIG_KEYS
        .map((key) => state.configs.find((c) => c.config_key === key))
        .filter((c): c is ConfigRow => !!c)
    },
    systemConfigGroups(state): Array<{ title: string; hint: string; items: ConfigRow[] }> {
      const grouped = SYSTEM_CONFIG_GROUPS.map((g) => ({
        title: g.title,
        hint: g.hint,
        items: g.keys
          .map((k) => state.configs.find((c) => c.config_key === k))
          .filter((c): c is ConfigRow => !!c && c.editable),
      })).filter((g) => g.items.length)
      const assigned = SYSTEM_CONFIG_GROUPS.flatMap((g) => g.keys)
      const orphans = state.configs.filter(
        (c) => c.editable && !assigned.includes(c.config_key) &&
          !POINTS_CONFIG_KEYS.includes(c.config_key) && !PAY_CHANNEL_CONFIG_KEYS.includes(c.config_key)
      )
      if (orphans.length) grouped.push({ title: '其他', hint: '尚未归类的配置项', items: orphans })
      return grouped
    },
    currentCredFields(state): Array<{ key: string; label: string; multiline?: boolean }> {
      return PAY_CRED_FIELDS[state.credChannel]
    },
  },
  actions: {
    showToast(msg: string, isError = false) {
      useAppStore().showToast(msg, isError)
    },
    // ===== 交班 =====
    async loadShift() {
      const app = useAppStore()
      try {
        this.shiftRows = (await api.shiftReport(app.shiftId)) as ShiftRow[]
      } catch (e) {
        this.showToast(String(e), true)
      }
    },
    async doOpenShift() {
      const app = useAppStore()
      try {
        app.currentShiftId = await api.openShift(Math.round(Number(this.openingFloat || '0') * CENTS_PER_YUAN))
        app.shiftId = app.currentShiftId
        this.showToast(`已开班，班次 ${app.currentShiftId}`)
        await this.loadShift()
      } catch (e) {
        this.showToast(String(e), true)
      }
    },
    async doCloseShift() {
      const app = useAppStore()
      try {
        const nonce = await app.requireSensitiveAuth('pos.shift.close', '关班授权')
        const r = await api.closeShift(
          app.currentShiftId,
          Math.round(Number(this.actualCash || '0') * CENTS_PER_YUAN),
          this.differenceReason.trim() || null,
          nonce
        )
        this.showToast(`已关班，现金差额 ${formatMoney(r.difference_cents)}`)
        app.currentShiftId = 0
        this.differenceReason = ''
      } catch (e) {
        if (String(e) !== '已取消二次授权') this.showToast(String(e), true)
      }
    },

    // ===== 报表 =====
    async loadReportCatalog() {
      try {
        this.reportMetas = await api.reportCatalog()
        if (this.reportMetas.length && !this.reportMetas.some((m) => m.id === this.activeReportId)) {
          this.activeReportId = this.reportMetas[0].id
        }
      } catch (e) {
        this.showToast(String(e), true)
      }
    },
    async runActiveReport() {
      this.reportLoading = true
      try {
        this.activeReport = await api.genericReport(this.activeReportId, this.centerFrom, this.centerTo)
      } catch (e) {
        this.showToast(String(e), true)
      } finally {
        this.reportLoading = false
      }
    },
    selectReport(id: string) {
      this.reportTool = null
      this.activeReportId = id
      void this.runActiveReport()
    },
    openTool(t: ReportTool) {
      this.reportTool = t
    },
    exportActiveReport() {
      const r = this.activeReport
      if (!r) return
      const esc = (s: string) => `"${s.replace(/"/g, '""')}"`
      const lines = [r.columns.map((c) => esc(c.label)).join(',')]
      for (const row of r.rows) lines.push(row.map(esc).join(','))
      const blob = new Blob(['\ufeff' + lines.join('\r\n')], { type: 'text/csv;charset=utf-8' })
      const url = URL.createObjectURL(blob)
      const a = document.createElement('a')
      a.href = url
      a.download = `${r.id}-${this.centerFrom}_${this.centerTo}.csv`
      a.click()
      URL.revokeObjectURL(url)
      this.showToast('报表已导出')
    },
    goReport() {
      const app = useAppStore()
      app.view = 'report'
      this.reportTool = null
      void this.loadReportCatalog().then(() => this.runActiveReport())
    },

    // ===== 系统配置 =====
    configLabel(key: string): string {
      return this.configs.find((c) => c.config_key === key)?.label ?? key
    },
    configOptions(key: string): Array<{ value: string; label: string }> {
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
    },
    async loadConfigs() {
      try {
        this.configs = await api.getConfigs()
        const draft: Record<string, string> = {}
        for (const c of this.configs) draft[c.config_key] = c.config_value
        this.configDraft = draft
        const gap = Number(draft['scan_gap_ms'])
        if (Number.isFinite(gap) && gap > 0) usePosStore().setScanGapMs(gap)
      } catch (e) {
        this.showToast(String(e), true)
      }
    },
    async saveConfig(key: string): Promise<boolean> {
      try {
        await api.setConfig(key, this.configDraft[key] ?? '')
        this.showToast('配置已保存')
        await this.loadConfigs()
        if (key === 'print_auto') useAppStore().printAuto = this.configDraft[key] === '1'
        return true
      } catch (e) {
        this.showToast(String(e), true)
        return false
      }
    },
    async toggleConfigSwitch(key: string) {
      const prev = this.configDraft[key]
      const next = prev === '1' ? '0' : '1'
      this.configDraft[key] = next
      if (!(await this.saveConfig(key))) this.configDraft[key] = prev
    },

    // ===== 支付设置 =====
    async loadPaymentSettings() {
      const app = useAppStore()
      try {
        const g = await api.paymentGatewayStatus()
        app.gatewayModes = { wechat: g.wechat_mode, alipay: g.alipay_mode }
        if (g.wechat_error) this.showToast(`微信网关状态异常：${g.wechat_error}`, true)
        if (g.alipay_error) this.showToast(`支付宝网关状态异常：${g.alipay_error}`, true)
      } catch (e) {
        this.showToast(String(e), true)
      }
      try {
        const c = await api.getPayChannels()
        this.channelDraft = {
          pay_channel_cash_enabled: c.cash ? '1' : '0',
          pay_channel_wechat_enabled: c.wechat ? '1' : '0',
          pay_channel_alipay_enabled: c.alipay ? '1' : '0',
          pay_channel_sv_enabled: c.stored_value ? '1' : '0',
        }
      } catch (e) {
        this.showToast(String(e), true)
      }
    },
    async toggleChannel(key: string, current: string) {
      const next = current === '1' ? '0' : '1'
      try {
        await api.setConfig(key, next)
        this.channelDraft[key] = next
        this.showToast('渠道开关已保存')
        await useAppStore().loadPayChannels()
      } catch (e) {
        this.showToast(String(e), true)
      }
    },
    async saveCredential(field: string) {
      const value = this.credDraft[field]
      if (!value || !value.trim()) {
        this.showToast('凭据内容为空', true)
        return
      }
      try {
        await api.setPaymentCredential(this.credChannel, field, value)
        this.credDraft[field] = ''
        this.credSaved[field] = true
        this.showToast('凭据已写入系统凭据库')
        await this.loadPaymentSettings()
      } catch (e) {
        this.showToast(String(e), true)
      }
    },
    async removeCredential(field: string) {
      try {
        await api.deletePaymentCredential(this.credChannel, field)
        this.credSaved[field] = false
        this.showToast('凭据已删除')
        await this.loadPaymentSettings()
      } catch (e) {
        this.showToast(String(e), true)
      }
    },
    /** 凭据弹窗统一保存：一次写入所有已填字段，避免每个字段单独一个保存按钮 */
    async saveAllCredentials() {
      const filled = Object.entries(this.credDraft).filter(([, v]) => v && v.trim())
      if (!filled.length) {
        this.showToast('没有要保存的修改；留空表示不修改该项', true)
        return
      }
      this.credBusy = true
      try {
        for (const [field, value] of filled) {
          await api.setPaymentCredential(this.credChannel, field, value)
          this.credSaved[field] = true
          this.credDraft[field] = ''
        }
        this.showToast(`已保存 ${filled.length} 项凭据，重启应用后生效`)
        await this.loadPaymentSettings()
      } catch (e) {
        this.showToast(String(e), true)
      } finally {
        this.credBusy = false
      }
    },

    // ===== 对账 =====
    async importBill(channel: 'WECHAT' | 'ALIPAY') {
      if (!this.reconText.trim()) {
        this.showToast('请先粘贴账单文本', true)
        return
      }
      try {
        this.reconSummary = channel === 'WECHAT'
          ? await api.importWechatBill(this.reconDate, this.reconText)
          : await api.importAlipayBill(this.reconDate, this.reconText)
        this.reconText = ''
        await this.loadReconDiffs()
        this.showToast('账单已导入并完成对账')
      } catch (e) {
        this.showToast(String(e), true)
      }
    },
    async loadReconDiffs() {
      try {
        this.reconDiffs = await api.listReconcileDiffs(this.reconDate)
      } catch (e) {
        this.showToast(String(e), true)
      }
    },

    // ===== 打印队列 =====
    async loadPrintQueue() {
      try {
        this.printQueue = await api.printRetries()
      } catch (e) {
        this.showToast(String(e), true)
      }
    },
    async doRetryPrint(orderNo: string) {
      try {
        this.showToast(await api.retryPrint(orderNo))
        await this.loadPrintQueue()
      } catch (e) {
        this.showToast(String(e), true)
      }
    },

    // ===== 订单作废/退货 =====
    async loadOrderItems() {
      const no = this.opOrderNo.trim()
      if (!no) return
      try {
        this.opItems = await api.getOrderItems(no)
      } catch (e) {
        this.showToast(String(e))
      }
    },
    async doVoid() {
      const app = useAppStore()
      try {
        const nonce = await app.requireSensitiveAuth('pos.void', '作废授权')
        await api.voidOrder(this.opOrderNo.trim(), nonce)
        this.showToast('订单已作废')
        this.opItems = []
        this.opOrderNo = ''
      } catch (e) {
        if (String(e) !== '已取消二次授权') this.showToast(String(e), true)
      }
    },
    async doRefund() {
      const app = useAppStore()
      const items = this.opItems
        .filter((r) => {
          const q = parseInt(this.opQty[r.item_id] || '0', 10)
          return q > 0 && q <= r.qty - r.returned_qty
        })
        .map((r) => ({ order_item_id: r.item_id, qty: parseInt(this.opQty[r.item_id], 10) }))
      if (!items.length) {
        this.showToast('请填写有效的退货数量', true)
        return
      }
      try {
        const nonce = await app.requireSensitiveAuth('pos.refund', '退货授权')
        const total = await api.refundOrder(this.opOrderNo.trim(), items, nonce)
        this.showToast(`退货成功，退款 ${formatMoney(total)}`)
        this.opItems = []
        this.opOrderNo = ''
      } catch (e) {
        if (String(e) !== '已取消二次授权') this.showToast(String(e), true)
      }
    },

    // ===== 员工 =====
    async loadUsers() {
      try {
        this.users = await api.listUsers()
      } catch (e) {
        this.showToast(String(e), true)
      }
    },
    async addUser() {
      const username = this.uForm.username.trim()
      const password = this.uForm.password
      if (!username) {
        this.showToast('请输入员工用户名', true)
        return
      }
      if (password.length < 8 || !/[a-zA-Z]/.test(password) || !/\d/.test(password)) {
        this.showToast('密码至少 8 位，且同时包含字母与数字', true)
        return
      }
      try {
        await api.createUser(username, password, this.uForm.roleName)
        this.uForm = { username: '', password: '', roleName: 'CASHIER' }
        this.showToast('员工账号已创建')
        await this.loadUsers()
      } catch (e) {
        this.showToast(String(e), true)
      }
    },
    async toggleUser(u: UserRowUI) {
      try {
        await api.setUserStatus(u.user_id, u.status === 1 ? 0 : 1)
        await this.loadUsers()
      } catch (e) {
        this.showToast(String(e))
      }
    },
    async loadReceiptTemplate() {
      try {
        const t = await api.getReceiptTemplate()
        this.receiptTemplateJson = t.content_json
        this.receiptPaperWidth = t.paper_width_mm
        await this.previewReceipt()
      } catch (e) {
        this.showToast(String(e), true)
      }
    },
    async previewReceipt() {
      try {
        this.receiptPreview = await api.previewReceipt(this.receiptTemplateJson)
      } catch (e) {
        this.receiptPreview = ''
        this.showToast(String(e), true)
      }
    },
    async saveReceiptTemplate(): Promise<boolean> {
      try {
        this.showToast(await api.saveReceiptTemplate(this.receiptTemplateJson, this.receiptPaperWidth))
        await this.previewReceipt()
        return true
      } catch (e) {
        this.showToast(String(e), true)
        return false
      }
    },
    goSettings() {
      const app = useAppStore()
      app.view = 'settings'
      void this.loadReceiptTemplate()
      void this.loadUsers()
      void this.loadConfigs()
      void this.loadPrintQueue()
      void this.loadPaymentSettings()
      void app.loadIndustries()
      void app.loadStoreProfile()
    },
  },
})
