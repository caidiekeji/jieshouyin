<script setup lang="ts">
import { computed, ref } from 'vue'
import Icon from './Icon.vue'
import { ICON_SIZE } from '../constants'
import { useAppContext } from '../composables/useAppContext'

/**
 * 自研日期区间选择器：弃原生 input[type=date]（WebView 渲染不可控、触屏体验差）。
 * 快捷区间 chips 走 ops.applyDashPreset（联动仪表盘粒度）；日历面板选区点「应用」才提交。
 * 桌面双月；Android 端有独立分叉实现（单月、48px 热区），不要互相覆盖。
 */
const { centerFrom, centerTo, applyDashPreset, refreshReportView } = useAppContext()

const open = ref(false)
const draftFrom = ref('')
const draftTo = ref('')
/** 面板左月视口（YYYY-MM），右月为其下一月 */
const viewMonth = ref('')

const PRESETS = [
  { id: 'today', label: '今日' },
  { id: 'yesterday', label: '昨日' },
  { id: 'last7', label: '近7天' },
  { id: 'last30', label: '近30天' },
  { id: 'thisWeek', label: '本周' },
  { id: 'thisMonth', label: '本月' },
  { id: 'lastMonth', label: '上月' },
]

const WEEK_HEADS = ['一', '二', '三', '四', '五', '六', '日']

interface Cell { date: string; day: number; inMonth: boolean }

function strToDate(s: string): Date {
  const [y, mo, d] = s.split('-').map(Number)
  return new Date(Date.UTC(y, mo - 1, d))
}
function iso(d: Date): string {
  return d.toISOString().slice(0, 10)
}

/** 一个月的 6×7 网格：周一开头，前后月补位灰显 */
function cellsOf(y: number, mo: number): Cell[] {
  const first = new Date(Date.UTC(y, mo - 1, 1))
  const start = new Date(first)
  start.setUTCDate(1 - ((first.getUTCDay() + 6) % 7))
  const out: Cell[] = []
  for (let i = 0; i < 42; i++) {
    const d = new Date(start)
    d.setUTCDate(start.getUTCDate() + i)
    out.push({ date: iso(d), day: d.getUTCDate(), inMonth: d.getUTCMonth() + 1 === mo })
  }
  return out
}
const leftCells = computed<Cell[]>(() => {
  const [y, mo] = viewMonth.value.split('-').map(Number)
  return cellsOf(y, mo)
})
const rightCells = computed<Cell[]>(() => {
  const [y, mo] = viewMonth.value.split('-').map(Number)
  const nxt = mo === 12 ? [y + 1, 1] : [y, mo + 1]
  return cellsOf(nxt[0], nxt[1])
})
const leftLabel = computed(() => viewMonth.value.replace('-', ' 年 ') + ' 月')
const rightLabel = computed(() => {
  const [y, mo] = viewMonth.value.split('-').map(Number)
  const nxt = mo === 12 ? [y + 1, 1] : [y, mo + 1]
  return `${nxt[0]} 年 ${nxt[1]} 月`
})

const viewLabel = computed(() => (centerFrom.value === centerTo.value ? centerFrom.value : `${centerFrom.value} ~ ${centerTo.value}`))

function shiftMonth(n: number) {
  const [y, mo] = viewMonth.value.split('-').map(Number)
  const d = mo + n
  const ny = d < 1 ? y - 1 : d > 12 ? y + 1 : y
  const nm = d < 1 ? d + 12 : d > 12 ? d - 12 : d
  viewMonth.value = `${ny}-${String(nm).padStart(2, '0')}`
}

function openPanel() {
  draftFrom.value = centerFrom.value
  draftTo.value = centerTo.value
  viewMonth.value = centerFrom.value.slice(0, 7)
  open.value = !open.value
}

/** 日历选区：第一击定起点，第二击定终点（早于起点则交换），再击重新开始 */
function pickDay(c: Cell) {
  if (!draftFrom.value || (draftFrom.value && draftTo.value)) {
    draftFrom.value = c.date
    draftTo.value = ''
    return
  }
  if (c.date < draftFrom.value) {
    draftTo.value = draftFrom.value
    draftFrom.value = c.date
  } else {
    draftTo.value = c.date
  }
}
function cellClass(c: Cell) {
  const from = draftFrom.value
  const to = draftTo.value || draftFrom.value
  const lo = from < to ? from : to
  const hi = from < to ? to : from
  return {
    'dtp-day': true,
    'in-month': c.inMonth,
    edge: c.date === from || c.date === (draftTo.value || ''),
    'in-range': !!from && !!draftTo.value && c.date >= lo && c.date <= hi,
  }
}

function pickPreset(id: string) {
  open.value = false
  applyDashPreset(id)
}
function applyCustom() {
  if (!draftFrom.value) return
  centerFrom.value = draftFrom.value
  centerTo.value = draftTo.value || draftFrom.value
  open.value = false
  refreshReportView()
}
</script>

<template>
  <div class="dtp" role="group" aria-label="报表日期区间">
    <button class="dtp-trigger" :aria-expanded="open" aria-haspopup="dialog" @click="openPanel">
      <Icon name="report" :size="ICON_SIZE.sm" />
      <span>{{ viewLabel }}</span>
      <Icon name="chevron-down" :size="ICON_SIZE.sm" :class="{ open }" />
    </button>
    <div v-if="open" class="dtp-panel" role="dialog" aria-label="选择日期区间">
      <div class="dtp-presets">
        <button v-for="p in PRESETS" :key="p.id" class="dtp-chip" @click="pickPreset(p.id)">{{ p.label }}</button>
      </div>
      <div class="dtp-months">
        <div class="dtp-month">
          <div class="dtp-month-head">
            <button class="dtp-nav dtp-nav--prev" aria-label="上一月" @click="shiftMonth(-1)"><Icon name="chevron-down" :size="ICON_SIZE.sm" /></button>
            <span>{{ leftLabel }}</span>
            <span class="dtp-nav-spacer"></span>
          </div>
          <div class="dtp-grid">
            <span v-for="w in WEEK_HEADS" :key="w" class="dtp-week">{{ w }}</span>
            <button
              v-for="c in leftCells"
              :key="c.date"
              :class="cellClass(c)"
              @click="pickDay(c)"
            >{{ c.day }}</button>
          </div>
        </div>
        <div class="dtp-month">
          <div class="dtp-month-head">
            <span class="dtp-nav-spacer"></span>
            <span>{{ rightLabel }}</span>
            <button class="dtp-nav dtp-nav--next" aria-label="下一月" @click="shiftMonth(1)"><Icon name="chevron-down" :size="ICON_SIZE.sm" /></button>
          </div>
          <div class="dtp-grid">
            <span v-for="w in WEEK_HEADS" :key="w" class="dtp-week">{{ w }}</span>
            <button
              v-for="c in rightCells"
              :key="c.date"
              :class="cellClass(c)"
              @click="pickDay(c)"
            >{{ c.day }}</button>
          </div>
        </div>
      </div>
      <div class="dtp-foot">
        <span class="muted small">{{ draftFrom ? (draftTo ? `${draftFrom} ~ ${draftTo}` : `${draftFrom} 起`) : '请选择区间' }}</span>
        <button class="btn primary btn-sm" :disabled="!draftFrom" @click="applyCustom">应用</button>
      </div>
    </div>
  </div>
</template>
