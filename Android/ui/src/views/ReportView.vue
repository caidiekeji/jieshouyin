<script setup lang="ts">
import { useAppContext } from '../composables/useAppContext'

const ctx = useAppContext()
const {
  activeReport,
  activeReportId,
  actualCash,
  centerFrom,
  centerTo,
  currentShiftId,
  differenceReason,
  doCloseShift,
  doOpenShift,
  doRefund,
  doVoid,
  exportActiveReport,
  formatMoney,
  gotoOrder,
  loadOrderItems,
  loadShift,
  opItems,
  opOrderNo,
  opQty,
  openTool,
  openingFloat,
  reportGroups,
  reportLoading,
  reportTool,
  runActiveReport,
  selectReport,
  shiftId,
  shiftRows,
  view,
} = ctx

/** 订单号单元格才可跳转；同一判断供 class、焦点与键盘共用，避免规则分叉 */
function orderNoOf(ci: number, cell: unknown): string | null {
  return activeReport.value?.columns[ci]?.key === 'order_no' && cell ? String(cell) : null
}
function openOrderNo(ci: number, cell: unknown) {
  const no = orderNoOf(ci, cell)
  if (no) gotoOrder(no)
}
</script>

<template>
      <main v-if="view === 'report' || view === 'shift'" :class="['admin-shell', { 'shift-only': view === 'shift' }]">
        <!-- 交班视图（收银员）：不展示报表目录，避免触及 report.view 权限；仅班次管理内容 -->
        <aside v-if="view === 'report'" class="admin-side report-side">
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
                      <td
                        v-for="(cell, ci) in row"
                        :key="ci"
                        :class="{ 'num-cell': activeReport.columns[ci]?.kind === 'money' || activeReport.columns[ci]?.kind === 'qty' || activeReport.columns[ci]?.kind === 'percent', 'cell-link': !!orderNoOf(ci, cell) }"
                        :tabindex="orderNoOf(ci, cell) ? 0 : undefined"
                        @click="openOrderNo(ci, cell)"
                        @keydown.enter="openOrderNo(ci, cell)"
                        @keydown.space.prevent="openOrderNo(ci, cell)"
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
              <p class="muted small">开班设置备用金；关班需接班收银员或店长授权，并核对现金差额。</p>
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
</template>
