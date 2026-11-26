<script setup lang="ts">
import { computed } from 'vue'
import { useAppContext } from '../composables/useAppContext'

/**
 * 经营总览仪表盘（Android 分叉）：指标卡用触屏卡片形态（复用 .tcard 视觉），
 * 单列堆叠布局适配横屏矮屏与竖屏；数据由 ops.runDashboard 前端聚合。
 * 环比口径：delta = (本期−对比期)/对比期；对比期 ≤0 时无意义，显示「—」。
 */
const { dashData, dashGrain, dashLoading, setDashGrain, formatMoney } = useAppContext()

const GRAINS = [
  { id: 'day', label: '日' },
  { id: 'week', label: '周' },
  { id: 'month', label: '月' },
] as const

function delta(cur: number | null, prev: number | null | undefined): number | null {
  if (cur == null || prev == null || prev <= 0) return null
  return Math.round(((cur - prev) / prev) * 100)
}

const kpis = computed(() => {
  const d = dashData.value
  if (!d) return []
  const c = d.current
  const p = d.compare
  return [
    { label: '净营业额', value: formatMoney(c.netCents), delta: delta(c.netCents, p?.netCents) },
    { label: '订单数', value: String(c.orders), delta: delta(c.orders, p?.orders) },
    { label: '客单价', value: formatMoney(c.avgCents), delta: delta(c.avgCents, p?.avgCents) },
    { label: '净利润', value: c.profitCents == null ? '—' : formatMoney(c.profitCents), delta: delta(c.profitCents, p?.profitCents) },
  ]
})

const buckets = computed(() => {
  const d = dashData.value
  if (!d) return []
  const cmp = d.compare?.daily ?? []
  return d.current.daily.map((day, i) => ({
    label: bucketLabel(day.key, d.grain),
    cur: day.netCents,
    cmp: cmp[i]?.netCents ?? null,
  })).slice(-60) // 极端保护：最多渲染最近 60 桶，避免跨年区间把图压扁
})
const maxBar = computed(() => Math.max(1, ...buckets.value.flatMap((b) => [b.cur, b.cmp ?? 0])))

function bucketLabel(key: string, grain: string): string {
  if (grain === 'month') return key.slice(2)
  const d = key.slice(5)
  return grain === 'week' ? `${d} 周` : d
}
function barPct(v: number): string {
  return `${Math.max(2, Math.round((v / maxBar.value) * 100))}%`
}

const topProducts = computed(() => dashData.value?.current.topProducts ?? [])
const payments = computed(() => (dashData.value?.current.payments ?? []).filter((p) => p.amountCents > 0))
const isEmpty = computed(() => {
  const d = dashData.value
  return !!d && d.current.orders === 0 && (d.compare?.orders ?? 0) === 0
})
</script>

<template>
  <div class="rpt-dash">
    <!-- 粒度切换：日 / 周 / 月；48px 热区 -->
    <div class="rpt-dash-head">
      <div class="seg rpt-grain" role="tablist" aria-label="对比粒度">
        <button
          v-for="g in GRAINS"
          :key="g.id"
          :class="['seg-btn', { active: dashGrain === g.id }]"
          role="tab"
          :aria-selected="dashGrain === g.id"
          @click="setDashGrain(g.id)"
        >{{ g.label }}</button>
      </div>
      <span v-if="dashData?.compareRange" class="muted small">
        对比 {{ dashData.compareRange.from }} ~ {{ dashData.compareRange.to }}
      </span>
    </div>

    <!-- 加载骨架：形状对齐真实指标卡（标签条 + 大数字条），禁通用转圈 -->
    <div v-if="dashLoading" class="rpt-kpis" aria-label="加载中">
      <div v-for="i in 4" :key="i" class="rpt-kpi rpt-skeleton"><span class="rpt-skeleton-label"></span><span class="rpt-skeleton-value"></span></div>
    </div>

    <template v-else-if="dashData">
      <!-- 空态：区间与对比期都无订单 -->
      <div v-if="isEmpty" class="card rpt-empty">
        <p>所选区间暂无订单数据</p>
        <p class="muted small">试试快捷区间「近7天」或「本月」，或调整日期后重查</p>
      </div>

      <template v-else>
        <!-- 核心指标 ×4：触屏卡片，大数字 + 环比徽标同排 -->
        <div class="rpt-kpis">
          <div v-for="k in kpis" :key="k.label" class="rpt-kpi">
            <span class="rpt-kpi-label">{{ k.label }}</span>
            <div class="rpt-kpi-row">
              <strong class="rpt-kpi-value">{{ k.value }}</strong>
              <span
                v-if="k.delta != null"
                class="rpt-delta"
                :class="k.delta >= 0 ? 'up' : 'down'"
                :aria-label="`${k.label}环比${k.delta >= 0 ? '增' : '降'}${Math.abs(k.delta)}%`"
              >{{ k.delta >= 0 ? '▲' : '▼' }}{{ Math.abs(k.delta) }}%</span>
              <span v-else class="rpt-delta none">—</span>
            </div>
          </div>
        </div>

        <!-- 趋势对比：本期实色 + 对比期灰阶；触屏卡片内纵向滑动 -->
        <div class="card rpt-trend">
          <h4>营业额趋势 <span class="muted small">（{{ dashGrain === 'day' ? '逐日' : dashGrain === 'week' ? '逐周' : '逐月' }} · 实色本期 / 灰色对比期）</span></h4>
          <div class="rpt-bars">
            <div v-for="b in buckets" :key="b.label" class="rpt-bar-row">
              <span class="rpt-bar-label">{{ b.label }}</span>
              <div class="rpt-bar-track">
                <div class="rpt-bar cur" :style="{ width: barPct(b.cur) }"></div>
                <div v-if="b.cmp != null" class="rpt-bar cmp" :style="{ width: barPct(b.cmp) }"></div>
              </div>
              <span class="rpt-bar-value">{{ formatMoney(b.cur) }}</span>
            </div>
          </div>
        </div>

        <div class="rpt-side">
          <!-- 支付占比 -->
          <div class="card rpt-pay">
            <h4>支付方式</h4>
            <div v-if="payments.length" class="rpt-pay-list">
              <div v-for="p in payments" :key="p.channel" class="rpt-pay-row">
                <span>{{ p.channel }}</span>
                <span class="mono">{{ formatMoney(p.amountCents) }}</span>
                <span class="muted small">{{ p.shareText }}</span>
              </div>
            </div>
            <div v-else class="empty">区间内无支付记录</div>
          </div>
          <!-- TOP5 商品 -->
          <div class="card rpt-top">
            <h4>销量 TOP5</h4>
            <ol v-if="topProducts.length" class="rpt-top-list">
              <li v-for="(t, i) in topProducts" :key="t.name">
                <span class="rpt-top-rank">{{ i + 1 }}</span>
                <span class="rpt-top-name">{{ t.name }}</span>
                <span class="mono muted small">×{{ t.qty }}</span>
                <span class="mono">{{ formatMoney(t.salesCents) }}</span>
              </li>
            </ol>
            <div v-else class="empty">区间内无商品动销</div>
          </div>
        </div>
      </template>
    </template>
  </div>
</template>
