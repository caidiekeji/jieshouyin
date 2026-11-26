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
const SYSTEM_CONFIG_GROUPS: Array<{ title: string; keys: string[] }> = [
  { title: '营业规则', keys: ['store_timezone', 'business_day_cutoff', 'max_discount_ratio', 'allow_negative_stock_default'] },
  { title: '支付时序', keys: ['micropay_poll_ms', 'micropay_timeout_s', 'native_poll_ms', 'native_timeout_s'] },
  { title: '收银设备', keys: ['scan_gap_ms'] },
  { title: '经营费用', keys: ['expense_rent_yuan', 'expense_utility_yuan', 'expense_labor_yuan'] },
  { title: '打印设置', keys: ['print_host', 'print_port', 'print_auto'] },
  { title: '行业规则', keys: ['chess_fee_unit', 'chess_min_minutes', 'chess_round_minutes', 'table_member_bind', 'table_allow_member_rebind'] },
  { title: '激活服务', keys: ['activation_endpoint'] },
]

type ReportTool = 'dashboard' | 'shift' | 'orderOp'

/** 仪表盘聚合粒度：日 / 周（周一起算）/ 月 */
export type DashGrain = 'day' | 'week' | 'month'
/** 单个时间桶：桶首日（周/月桶）+ 净营业额 + 订单数 + 净利润（profit 缺行时为 null） */
export interface DashDay { key: string; netCents: number; orders: number; profitCents: number | null }
/** 一期快照：本期或对比期的聚合结果 */
export interface DashSnapshot {
  netCents: number
  orders: number
  /** 客单价 = 净营业额 / 订单数（订单数为 0 时取 0） */
  avgCents: number
  discountCents: number
  refundCents: number
  profitCents: number | null
  topProducts: { name: string; qty: number; salesCents: number }[]
  payments: { channel: string; amountCents: number; shareText: string }[]
  daily: DashDay[]
}
export interface DashData {
  grain: DashGrain
  range: { from: string; to: string }
  current: DashSnapshot
  compare: DashSnapshot | null
  compareRange: { from: string; to: string } | null
}

// ===== 仪表盘纯函数（无平台差异，两端逐字同步）=====

/** 报表金额展示串（两位小数）→ 分整数。禁 parseFloat×100（浮点误差），按字符串拼整数。 */
function parseMoney(s: string): number {
  const m = /^(-?)(\d+)\.(\d{2})$/.exec(String(s).trim())
  return m ? Number(`${m[1]}${m[2]}${m[3]}`) : 0
}
function strToDate(s: string): Date {
  const [y, mo, d] = s.split('-').map(Number)
  return new Date(Date.UTC(y, mo - 1, d))
}
function iso(d: Date): string {
  return d.toISOString().slice(0, 10)
}
function addDays(d: Date, n: number): Date {
  const x = new Date(d)
  x.setUTCDate(x.getUTCDate() + n)
  return x
}
/** 周一索引（周一=0…周日=6），周桶以周一起算 */
function mondayIndex(d: Date): number {
  return (d.getUTCDay() + 6) % 7
}

/** 对比期映射：week→区间归一到周一~周日再整体前移 7 天；month→各月前移 1 个自然月；其余→等长前移（单日即前一天） */
function cmpRange(grain: DashGrain, from: string, to: string): { from: string; to: string } {
  const f = strToDate(from)
  const t = strToDate(to)
  if (grain === 'week') {
    const monday = addDays(f, -mondayIndex(f))
    const sunday = addDays(t, 6 - mondayIndex(t))
    return { from: iso(addDays(monday, -7)), to: iso(addDays(sunday, -7)) }
  }
  if (grain === 'month') {
    const prev = (y: number, mo: number) => (mo === 1 ? [y - 1, 12] : [y, mo - 1])
    const [fy, fm] = prev(f.getUTCFullYear(), f.getUTCMonth() + 1)
    const [ty, tm] = prev(t.getUTCFullYear(), t.getUTCMonth() + 1)
    const lastDay = (y: number, mo: number) => new Date(Date.UTC(y, mo, 0)).getUTCDate()
    return {
      from: `${fy}-${String(fm).padStart(2, '0')}-01`,
      to: `${ty}-${String(tm).padStart(2, '0')}-${String(lastDay(ty, tm)).padStart(2, '0')}`,
    }
  }
  const days = Math.round((t.getTime() - f.getTime()) / 86400000) + 1
  const shift = (d: Date) => addDays(d, -days)
  return { from: iso(shift(f)), to: iso(shift(t)) }
}

/** 时间桶键：日=当日；周=桶内周一日期；月=YYYY-MM */
function bucketKey(d: Date, grain: DashGrain): string {
  if (grain === 'month') return iso(d).slice(0, 7)
  if (grain === 'week') return iso(addDays(d, -mondayIndex(d)))
  return iso(d)
}

/** 销售日报行 [日期,订单数,净额,优惠,退款] → 桶聚合序列（金额展示串先转分再相加） */
function aggSalesDays(rows: string[][], grain: DashGrain): Map<string, { net: number; orders: number; discount: number; refund: number }> {
  const out = new Map<string, { net: number; orders: number; discount: number; refund: number }>()
  for (const r of rows) {
    const k = bucketKey(strToDate(r[0]), grain)
    const cur = out.get(k) ?? { net: 0, orders: 0, discount: 0, refund: 0 }
    cur.net += parseMoney(r[2])
    cur.orders += Number(r[1]) || 0
    cur.discount += parseMoney(r[3])
    cur.refund += parseMoney(r[4])
    out.set(k, cur)
  }
  return out
}

/** 利润报表行 [日期,实收,退款,净额,成本,毛利,分摊费用,净利润] → 桶聚合净利润 */
function aggProfitDays(rows: string[][], grain: DashGrain): Map<string, number> {
  const out = new Map<string, number>()
  for (const r of rows) {
    const k = bucketKey(strToDate(r[0]), grain)
    out.set(k, (out.get(k) ?? 0) + parseMoney(r[7]))
  }
  return out
}

/** 商品排行行 [排名,名称,销量,销售额] → TOP5 */
function ranking2top(r: ReportResult): DashSnapshot['topProducts'] {
  return r.rows.slice(0, 5).map((row) => ({ name: row[1], qty: Number(row[2]) || 0, salesCents: parseMoney(row[3]) }))
}

/** 支付渠道行 [渠道,笔数,金额,占比,收讫,找零] → 占比列表（渠道码转中文，未知渠道显示原码） */
const PAY_CHANNEL_LABELS: Record<string, string> = {
  CASH: '现金',
  WECHAT: '微信',
  ALIPAY: '支付宝',
  STORED_VALUE: '储值',
}
function pay2list(r: ReportResult): DashSnapshot['payments'] {
  return r.rows.map((row) => ({
    channel: PAY_CHANNEL_LABELS[row[0]] ?? row[0],
    amountCents: parseMoney(row[2]),
    shareText: row[3],
  }))
}

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
    // 经营总览（仪表盘）：前端聚合 sales_daily/profit/product_ranking/payment_summary
    dashGrain: 'day' as DashGrain,
    dashData: null as DashData | null,
    dashLoading: false,
    // 系统配置
    configs: [] as ConfigRow[],
    configDraft: {} as Record<string, string>,
    /** 加载时的基线快照：分组保存时只提交与基线不同的项 */
    configBaseline: {} as Record<string, string>,
    // 支付设置
    credChannel: 'WECHAT' as 'WECHAT' | 'ALIPAY',
    credDraft: {} as Record<string, string>,
    credSaved: {} as Record<string, boolean>,
    credBusy: false,
    /** 商户凭据弹窗开闭：状态收在 store，弹窗由 DialogLayer 统一渲染 */
    credDialogOpen: false,
    channelDraft: {} as Record<string, string>,
    // 修改密码弹窗（用户下拉菜单唤起）
    pwdDialogOpen: false,
    pwdDraft: { old: '', next: '', confirm: '' },
    pwdError: '',
    pwdBusy: false,
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
    systemConfigGroups(state): Array<{ title: string; items: ConfigRow[] }> {
      const grouped = SYSTEM_CONFIG_GROUPS.map((g) => ({
        title: g.title,
        items: g.keys
          .map((k) => state.configs.find((c) => c.config_key === k))
          .filter((c): c is ConfigRow => !!c && c.editable),
      })).filter((g) => g.items.length)
      const assigned = SYSTEM_CONFIG_GROUPS.flatMap((g) => g.keys)
      const orphans = state.configs.filter(
        (c) => c.editable && !assigned.includes(c.config_key) &&
          !POINTS_CONFIG_KEYS.includes(c.config_key) && !PAY_CHANNEL_CONFIG_KEYS.includes(c.config_key)
      )
      if (orphans.length) grouped.push({ title: '其他', items: orphans })
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
      if (t === 'dashboard') void this.runDashboard()
    },
    /** 切换仪表盘粒度并重查 */
    setDashGrain(g: DashGrain) {
      this.dashGrain = g
      void this.runDashboard()
    },
    /** 快捷区间预设：同时确定区间与粒度（今日/昨日/近N天→日；本周→周；本月/上月→月） */
    applyDashPreset(preset: string) {
      const t = strToDate(today())
      const yday = addDays(t, -1)
      const mon = addDays(t, -mondayIndex(t))
      const sun = addDays(mon, 6)
      const monthFirst = (d: Date) => new Date(Date.UTC(d.getUTCFullYear(), d.getUTCMonth(), 1))
      const monthLast = (d: Date) => new Date(Date.UTC(d.getUTCFullYear(), d.getUTCMonth() + 1, 0))
      const prevMonth = new Date(Date.UTC(t.getUTCFullYear(), t.getUTCMonth() - 1, 1))
      const table: Record<string, { from: Date; to: Date; grain: DashGrain }> = {
        today: { from: t, to: t, grain: 'day' },
        yesterday: { from: yday, to: yday, grain: 'day' },
        last7: { from: addDays(t, -6), to: t, grain: 'day' },
        last30: { from: addDays(t, -29), to: t, grain: 'day' },
        thisWeek: { from: mon, to: sun, grain: 'week' },
        thisMonth: { from: monthFirst(t), to: monthLast(t), grain: 'month' },
        lastMonth: { from: monthFirst(prevMonth), to: monthLast(prevMonth), grain: 'month' },
      }
      const hit = table[preset]
      if (!hit) return
      this.centerFrom = iso(hit.from)
      this.centerTo = iso(hit.to)
      this.dashGrain = hit.grain
      void this.runDashboard()
    },
    /** 日期条/快捷区间变更后重查当前视图 */
    refreshReportView() {
      if (this.reportTool === 'dashboard') void this.runDashboard()
      else void this.runActiveReport()
    },

    /** 经营总览：本期 + 对比期并发取数，纯函数聚合出环比（口径全部复用现有报表） */
    async runDashboard() {
      this.dashLoading = true
      try {
        const grain = this.dashGrain
        const from = this.centerFrom
        const to = this.centerTo
        const cmp = cmpRange(grain, from, to)
        const [curSales, cmpSales, curProfit, cmpProfit, ranking, payments] = await Promise.all([
          api.genericReport('sales_daily', from, to),
          api.genericReport('sales_daily', cmp.from, cmp.to),
          api.genericReport('profit', from, to),
          api.genericReport('profit', cmp.from, cmp.to),
          api.genericReport('product_ranking', from, to),
          api.genericReport('payment_summary', from, to),
        ])
        const buildSnap = (sales: ReportResult, profit: ReportResult): DashSnapshot => {
          const salesBuckets = aggSalesDays(sales.rows, grain)
          const profitBuckets = aggProfitDays(profit.rows, grain)
          let net = 0, orders = 0, discount = 0, refund = 0, profitTotal = 0
          const daily: DashDay[] = []
          for (const [k, v] of salesBuckets) {
            const hasProfit = profitBuckets.has(k)
            const p = profitBuckets.get(k) ?? 0
            net += v.net
            orders += v.orders
            discount += v.discount
            refund += v.refund
            profitTotal += p
            daily.push({ key: k, netCents: v.net, orders: v.orders, profitCents: hasProfit ? p : null })
          }
          daily.sort((a, b) => a.key.localeCompare(b.key))
          return {
            netCents: net,
            orders,
            avgCents: orders > 0 ? Math.round(net / orders) : 0,
            discountCents: discount,
            refundCents: refund,
            profitCents: profit.rows.length ? profitTotal : null,
            topProducts: ranking2top(ranking),
            payments: pay2list(payments),
            daily,
          }
        }
        this.dashData = {
          grain,
          range: { from, to },
          current: buildSnap(curSales, curProfit),
          compare: buildSnap(cmpSales, cmpProfit),
          compareRange: cmp,
        }
      } catch (e) {
        this.showToast(String(e), true)
      } finally {
        this.dashLoading = false
      }
    },

    /** 利润报表费用引导：跳设置页系统配置（权限由设置页守卫统一拦截） */
    goExpenseConfig() {
      const app = useAppStore()
      app.settingsTab = 'system'
      app.view = 'settings'
    },
    /** 报表导出：后端生成 CSV 并落盘（桌面另存为对话框 / Android 应用数据目录）。
        Blob+<a> 下载在 Tauri WebView（尤其 Android）不生效，必须走原生命令。 */
    async exportActiveReport() {
      const r = this.activeReport
      if (!r) return
      try {
        const path = await api.exportReport(r.id, this.centerFrom, this.centerTo)
        if (!path) return // 用户取消保存对话框
        this.showToast(`已导出：${path}`)
      } catch (e) {
        this.showToast(String(e), true)
      }
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
        this.configBaseline = { ...draft }
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
    /** 分组内已修改（与基线不同）的配置项数量，0 表示无修改 */
    configGroupChanged(title: string): number {
      const group = this.systemConfigGroups.find((g) => g.title === title)
      if (!group) return 0
      return group.items.filter(
        (c) => (this.configDraft[c.config_key] ?? '') !== (this.configBaseline[c.config_key] ?? '')
      ).length
    },
    /** 分组统一保存：一次提交组内全部已修改项，避免每个字段一个保存按钮 */
    async saveConfigGroup(title: string) {
      const group = this.systemConfigGroups.find((g) => g.title === title)
      if (!group) return
      const changed = group.items.filter(
        (c) => (this.configDraft[c.config_key] ?? '') !== (this.configBaseline[c.config_key] ?? '')
      )
      if (!changed.length) {
        this.showToast('本组没有修改')
        return
      }
      let saved = 0
      for (const c of changed) {
        try {
          await api.setConfig(c.config_key, this.configDraft[c.config_key] ?? '')
          saved++
          if (c.config_key === 'print_auto') {
            useAppStore().printAuto = this.configDraft[c.config_key] === '1'
          }
        } catch (e) {
          this.showToast(`「${c.label}」保存失败：${String(e)}`, true)
          break
        }
      }
      await this.loadConfigs()
      this.showToast(saved === changed.length ? `已保存 ${saved} 项配置` : `已保存 ${saved}/${changed.length} 项配置`)
    },
    /** 积分配置统一保存（同分组保存模式） */
    async savePointsConfigs() {
      const changed = this.pointsConfigs.filter(
        (c) => (this.configDraft[c.config_key] ?? '') !== (this.configBaseline[c.config_key] ?? '')
      )
      if (!changed.length) {
        this.showToast('积分配置没有修改')
        return
      }
      let saved = 0
      for (const c of changed) {
        try {
          await api.setConfig(c.config_key, this.configDraft[c.config_key] ?? '')
          saved++
        } catch (e) {
          this.showToast(`「${c.label}」保存失败：${String(e)}`, true)
          break
        }
      }
      await this.loadConfigs()
      this.showToast(saved === changed.length ? '积分配置已保存' : `已保存 ${saved}/${changed.length} 项`)
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
    /** 打开凭据弹窗：切渠道后打开，弹窗本体在 DialogLayer */
    openCredDialog(channel: 'WECHAT' | 'ALIPAY') {
      this.credChannel = channel
      this.credDraft = {}
      this.credDialogOpen = true
    },
    closeCredDialog() {
      this.credDialogOpen = false
    },
    // ===== 修改密码 =====
    openPwdDialog() {
      this.pwdDraft = { old: '', next: '', confirm: '' }
      this.pwdError = ''
      this.pwdDialogOpen = true
    },
    closePwdDialog() {
      this.pwdDialogOpen = false
    },
    async submitChangePassword() {
      const { old, next, confirm } = this.pwdDraft
      if (!old || !next) {
        this.pwdError = '请填写原密码与新密码'
        return
      }
      if (next !== confirm) {
        this.pwdError = '两次输入的新密码不一致'
        return
      }
      this.pwdBusy = true
      this.pwdError = ''
      try {
        await api.changePassword(old, next)
        this.pwdDialogOpen = false
        this.showToast('密码已修改')
      } catch (e) {
        this.pwdError = String(e)
      } finally {
        this.pwdBusy = false
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
