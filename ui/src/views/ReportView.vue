<script setup lang="ts">
import DateRangePicker from '../components/DateRangePicker.vue'
import ReportDashboard from '../components/ReportDashboard.vue'
import { useAppContext } from '../composables/useAppContext'

const ctx = useAppContext()
const {
  activeReport,
  activeReportId,
  actualCash,
  atLeast,
  centerFrom,
  currentShiftId,
  differenceReason,
  doCloseShift,
  doOpenShift,
  doRefund,
  doVoid,
  exportActiveReport,
  exporting,
  formatMoney,
  goExpenseConfig,
  gotoOrder,
  gotoOrderList,
  loadOrderItems,
  loadShift,
  opItems,
  opOrderNo,
  opQty,
  openTool,
  openingFloat,
  refreshReportView,
  reportGroups,
  reportLoading,
  reportTool,
  selectReport,
  shiftId,
  shiftRows,
  view,
} = ctx

/** 订单号单元格才可跳详情；「订单数」单元格跳订单列表；同一判断供 class、焦点与键盘共用，避免规则分叉 */
function orderNoOf(ci: number, cell: unknown): string | null {
  return activeReport.value?.columns[ci]?.key === 'order_no' && cell ? String(cell) : null
}
/** 订单数列：点击跳订单列表并按该行营业日期过滤（行无日期列时用报表起始日） */
function orderCountOf(ci: number): boolean {
  return activeReport.value?.columns[ci]?.key === 'orders'
}
function rowDateOf(row: string[]): string {
  const di = activeReport.value?.columns.findIndex((c) => c.key === 'date') ?? -1
  return di >= 0 && row[di] ? row[di] : centerFrom.value
}
function cellLinkOf(ci: number, cell: unknown): boolean {
  return !!orderNoOf(ci, cell) || orderCountOf(ci)
}
function openCell(ci: number, row: string[]) {
  const cell = row[ci]
  if (orderNoOf(ci, cell)) gotoOrder(String(cell))
  else if (orderCountOf(ci)) gotoOrderList(rowDateOf(row))
}
/** 指标卡「订单数」：点击跳订单列表（报表起始日起） */
const isOrderCountMetric = (mt: { label: string }) => mt.label === '订单数'
function openOrderMetric() {
  gotoOrderList(centerFrom.value)
}
</script>

<template>
      <main v-if="view === 'report' || view === 'shift'" :class="['admin-shell', { 'shift-only': view === 'shift' }]">
        <!-- 报表中心：经营总览置顶，明细报表按域分组；收银员交班视图不展示目录 -->
        <aside v-if="view === 'report'" class="admin-side report-side">
          <div class="admin-side-title">报表中心</div>
          <button :class="['report-nav-item', { active: reportTool === 'dashboard' }]" @click="openTool('dashboard')">经营总览</button>
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
          <!-- 经营总览（仪表盘）：环比对比 + 趋势 + 支付/TOP5 -->
          <ReportDashboard v-if="reportTool === 'dashboard'" />

          <!-- 报表查询：统一骨架 = 日期条 + 指标卡 + 表格 -->
          <template v-else-if="!reportTool">
            <div class="toolbar report-toolbar">
              <DateRangePicker />
              <button class="btn primary" :disabled="reportLoading" @click="refreshReportView">
                {{ reportLoading ? '查询中…' : '查询' }}
              </button>
              <button class="btn" :disabled="!activeReport || exporting" @click="exportActiveReport">
                <span v-if="exporting" class="btn-spinner" aria-label="导出中"></span>
                {{ exporting ? '导出中…' : '导出CSV' }}
              </button>
            </div>
            <template v-if="activeReport">
              <div class="report-head">
                <h3>{{ activeReport.title }}</h3>
              </div>
              <!-- 利润报表：房租/水电/人工按天均摊已参与核算，费用值与口径在此明示 -->
              <div v-if="activeReportId === 'profit'" class="card rpt-expense">
                <p class="rpt-expense-text">{{ activeReport.description }}</p>
                <button v-if="atLeast('ADMIN')" class="btn btn-sm" @click="goExpenseConfig">调整月度费用</button>
                <p v-else class="muted small">月度费用由管理员在「设置 → 系统配置」维护</p>
              </div>
              <div v-if="activeReport.metrics.length" class="stats">
                <div
                  v-for="(mt, i) in activeReport.metrics"
                  :key="i"
                  class="stat"
                  :class="{ 'stat-link': isOrderCountMetric(mt) }"
                  :role="isOrderCountMetric(mt) ? 'button' : undefined"
                  :tabindex="isOrderCountMetric(mt) ? 0 : undefined"
                  @click="isOrderCountMetric(mt) && openOrderMetric()"
                  @keydown.enter="isOrderCountMetric(mt) && openOrderMetric()"
                  @keydown.space.prevent="isOrderCountMetric(mt) && openOrderMetric()"
                >
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
                      <td
                        v-for="(cell, ci) in row"
                        :key="ci"
                        :class="{ 'num-cell': activeReport.columns[ci]?.kind === 'money' || activeReport.columns[ci]?.kind === 'qty' || activeReport.columns[ci]?.kind === 'percent', 'cell-link': cellLinkOf(ci, cell) }"
                        :tabindex="cellLinkOf(ci, cell) ? 0 : undefined"
                        @click="openCell(ci, row)"
                        @keydown.enter="openCell(ci, row)"
                        @keydown.space.prevent="openCell(ci, row)"
                      >{{ cell }}</td>
                    </tr>
                  </tbody>
                </table>
                <div v-else class="empty">该区间暂无数据</div>
              </div>
            </template>
            <div v-else class="empty">请选择左侧报表</div>
          </template>
          <!-- 工具：班次管理（分节卡片 + 限宽字段，避免输入框被通栏拉宽） -->
          <template v-else-if="reportTool === 'shift'">
            <div class="report-head">
              <h3>班次管理 <span v-if="currentShiftId" class="badge">当前班次 #{{ currentShiftId }}</span></h3>
            </div>

            <!-- 开班 -->
            <div class="card shift-card">
              <h4>开班</h4>
              <div class="shift-fields">
                <label class="field-label">备用金（元）
                  <input v-model="openingFloat" class="input shift-input" type="number" min="0" aria-label="开班备用金（元）" />
                </label>
                <button class="btn" @click="doOpenShift">开班</button>
              </div>
            </div>

            <!-- 关班 -->
            <div class="card shift-card">
              <h4>关班</h4>
              <div class="shift-fields">
                <label class="field-label">实收现金（元）
                  <input v-model="actualCash" class="input shift-input" type="number" min="0" aria-label="关班实收现金（元）" />
                </label>
                <label class="field-label shift-field-grow">差异原因<span class="muted">（有现金差额时必填）</span>
                  <input v-model="differenceReason" class="input" type="text" aria-label="差异原因" />
                </label>
                <button class="btn" @click="doCloseShift">关班（需授权）</button>
              </div>
            </div>

            <!-- 汇总查询 -->
            <div class="card shift-card">
              <h4>交班汇总</h4>
              <div class="shift-fields">
                <label class="field-label">班次 ID
                  <input v-model.number="shiftId" class="input shift-input" type="number" min="1" aria-label="班次 ID" />
                </label>
                <button class="btn" @click="loadShift">查询</button>
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
            </div>
          </template>
          <!-- 工具：订单作废 / 退货 -->
          <template v-else>
            <div class="report-head">
              <h3>订单作废 / 退货</h3>
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
</template>
