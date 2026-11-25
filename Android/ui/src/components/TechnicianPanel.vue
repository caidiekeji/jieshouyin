<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import {
  api,
  formatMoney,
  type Category,
  type Technician,
  type TechnicianBell,
  type TechnicianCommission,
  type TechnicianRule,
} from '../api'

const props = defineProps<{
  /** 技师档案与排班需 product.edit；店长维护，收银员只在下拉里选在班技师。 */
  canManage: boolean
}>()
const emit = defineEmits<{ changed: [] }>()

const open = ref(false)
const busy = ref(false)
const technicians = ref<Technician[]>([])
const report = ref<TechnicianCommission[]>([])
const rules = ref<TechnicianRule[]>([])
const categories = ref<Category[]>([])
const bells = ref<TechnicianBell[]>([])
const form = ref({
  name: '',
  percent: 10,
  mode: 'PERCENT' as 'PERCENT' | 'FIXED',
  fixedYuan: 5,
})
const ruleForm = ref({
  technicianId: 0,
  categoryId: 0,
  mode: 'PERCENT' as 'PERCENT' | 'FIXED',
  percent: 10,
  fixedYuan: 5,
})
const today = ref(localDate(new Date()))
const month = computed(() => reportMonth())

function localDate(d: Date): string {
  const y = d.getFullYear()
  const m = String(d.getMonth() + 1).padStart(2, '0')
  const day = String(d.getDate()).padStart(2, '0')
  return `${y}-${m}-${day}`
}

/** 提成报表按自然月，月初到月末，不参与营业日切换。 */
function reportMonth(): { start: string; end: string } {
  const d = new Date()
  const start = new Date(d.getFullYear(), d.getMonth(), 1)
  const end = new Date(d.getFullYear(), d.getMonth() + 1, 0)
  return { start: localDate(start), end: localDate(end) }
}

async function load() {
  try {
    technicians.value = await api.listTechnicians(today.value)
    const range = month.value
    report.value = await api.technicianCommission(range.start, range.end)
    rules.value = await api.listTechnicianRules(null)
    categories.value = await api.listCategories()
    bells.value = await api.listBells(today.value)
    if (!ruleForm.value.technicianId && technicians.value.length) {
      ruleForm.value.technicianId = technicians.value[0].technician_id
    }
    if (!ruleForm.value.categoryId && categories.value.length) {
      ruleForm.value.categoryId = categories.value[0].category_id
    }
  } catch {
    technicians.value = []
    report.value = []
    rules.value = []
    categories.value = []
    bells.value = []
  }
}

async function add() {
  const name = form.value.name.trim()
  if (!name) return
  busy.value = true
  try {
    await api.createTechnician(
      name,
      null,
      Math.round(form.value.percent * 100),
      form.value.mode,
      Math.round(form.value.fixedYuan * 100)
    )
    form.value = { name: '', percent: 10, mode: form.value.mode, fixedYuan: form.value.fixedYuan }
    await load()
    emit('changed')
  } finally {
    busy.value = false
  }
}

async function toggle(t: Technician) {
  busy.value = true
  try {
    await api.setTechnicianShift(t.technician_id, today.value, !t.on_duty)
    await load()
    emit('changed')
  } finally {
    busy.value = false
  }
}

async function saveRule() {
  const r = ruleForm.value
  if (!r.technicianId || !r.categoryId) return
  busy.value = true
  try {
    await api.setTechnicianRule(
      r.technicianId,
      r.categoryId,
      r.mode,
      Math.round(r.percent * 100),
      Math.round(r.fixedYuan * 100)
    )
    await load()
  } finally {
    busy.value = false
  }
}

async function removeRule(rule: TechnicianRule) {
  busy.value = true
  try {
    await api.deleteTechnicianRule(rule.technician_id, rule.category_id)
    await load()
  } finally {
    busy.value = false
  }
}

function categoryName(id: number): string {
  return categories.value.find((c) => c.category_id === id)?.name ?? `分类 ${id}`
}

function technicianName(id: number): string {
  return technicians.value.find((t) => t.technician_id === id)?.name ?? `技师 ${id}`
}

function ruleText(r: TechnicianRule): string {
  return r.mode === 'FIXED'
    ? `每份 ¥${formatMoney(r.fixed_cents)}`
    : `${(r.bps / 100).toFixed(1)}%`
}

onMounted(() => {
  if (props.canManage) void load()
})
</script>

<template>
  <section v-if="canManage" class="tech-panel scene-tech">
    <div class="tech-head">
      <button class="link tech-toggle" @click="open = !open">
        {{ open ? '收起' : '技师排班 / 提成' }}
        <span class="muted">（今日在班 {{ technicians.filter((t) => t.on_duty).length }} 人）</span>
      </button>
      <span v-if="!open" class="muted small">服务行可在购物车里指定在班技师</span>
    </div>
    <div v-if="open" class="tech-body">
      <div class="tech-add">
        <input v-model="form.name" class="input input-sm" placeholder="技师姓名" maxlength="20" />
        <select v-model="form.mode" class="input input-sm" aria-label="提成方式">
          <option value="PERCENT">按比例</option>
          <option value="FIXED">每份固定</option>
        </select>
        <label v-if="form.mode === 'PERCENT'" class="muted small">
          提成
          <input v-model.number="form.percent" class="input input-xs" type="number" min="0" max="100" step="0.5" />
          %
        </label>
        <label v-else class="muted small">
          每份
          <input v-model.number="form.fixedYuan" class="input input-xs" type="number" min="0" step="0.5" />
          元
        </label>
        <button class="btn btn-sm primary" :disabled="busy || !form.name.trim()" @click="add">新增技师</button>
      </div>
      <ul class="tech-list">
        <li v-for="t in technicians" :key="t.technician_id">
          <strong>{{ t.name }}</strong>
          <span class="muted">
            {{ t.commission_mode === 'FIXED' ? `每份 ¥${formatMoney(t.fixed_cents)}` : `${(t.commission_bps / 100).toFixed(1)}%` }}
          </span>
          <button class="link" :class="{ on: t.on_duty }" :disabled="busy" @click="toggle(t)">
            {{ t.on_duty ? '在班 · 点此休息' : '休息 · 点此上班' }}
          </button>
        </li>
        <li v-if="!technicians.length" class="muted small">还没有技师，先新增档案。</li>
      </ul>
      <div class="tech-rules">
        <div class="muted small">按项目覆盖提成（同技师同分类只有一条，未设置则用技师默认）</div>
        <div class="tech-add">
          <select v-model.number="ruleForm.technicianId" class="input input-sm">
            <option v-for="t in technicians" :key="t.technician_id" :value="t.technician_id">{{ t.name }}</option>
          </select>
          <select v-model.number="ruleForm.categoryId" class="input input-sm">
            <option v-for="c in categories" :key="c.category_id" :value="c.category_id">{{ c.name }}</option>
          </select>
          <select v-model="ruleForm.mode" class="input input-sm">
            <option value="PERCENT">按比例</option>
            <option value="FIXED">每份固定</option>
          </select>
          <label v-if="ruleForm.mode === 'PERCENT'" class="muted small">
            <input v-model.number="ruleForm.percent" class="input input-xs" type="number" min="0" max="100" step="0.5" />%
          </label>
          <label v-else class="muted small">
            <input v-model.number="ruleForm.fixedYuan" class="input input-xs" type="number" min="0" step="0.5" />元/份
          </label>
          <button class="btn btn-sm" :disabled="busy" @click="saveRule">保存规则</button>
        </div>
        <ul class="tech-rule-list">
          <li v-for="r in rules" :key="`${r.technician_id}-${r.category_id}`">
            {{ technicianName(r.technician_id) }} · {{ categoryName(r.category_id) }} · {{ ruleText(r) }}
            <button class="link danger" :disabled="busy" @click="removeRule(r)">删除</button>
          </li>
          <li v-if="!rules.length" class="muted small">还没有项目级规则。</li>
        </ul>
      </div>
      <div class="tech-report">
        <div class="muted small">本月提成（{{ month.start }} ~ {{ month.end }}）</div>
        <table v-if="report.length">
          <thead><tr><th>技师</th><th>服务行</th><th>数量</th><th>提成</th></tr></thead>
          <tbody>
            <tr v-for="r in report" :key="r.technician_id">
              <td>{{ r.name }}</td>
              <td>{{ r.service_lines }}</td>
              <td>{{ r.service_qty }}</td>
              <td>¥{{ formatMoney(r.commission_cents) }}</td>
            </tr>
          </tbody>
        </table>
        <p v-else class="muted small">本月还没有技师提成记录。</p>
      </div>
      <div class="tech-bells">
        <div class="muted small">今日钟态（{{ today }}）</div>
        <ul v-if="bells.length" class="tech-rule-list">
          <li v-for="b in bells" :key="b.bell_id">
            {{ b.technician_name }} · {{ b.table_code || '未绑定服务间' }}
            · {{ b.is_pointed ? '点钟' : '排钟' }} · {{ b.minutes }} 分钟
            · {{ b.status === 'ACTIVE' ? `上钟 ${b.elapsed_minutes} 分钟` : '已收钟' }}
          </li>
        </ul>
        <p v-else class="muted small">今日还没有排钟记录。</p>
      </div>
    </div>
  </section>
</template>

<style scoped>
/* 配色来自场景语义层（tokens.css .scene-tech），暗色只翻语义层。 */
.tech-panel {
  margin: 0 var(--space-3) var(--space-2);
  border: var(--border-width) solid var(--scene-line);
  border-radius: var(--radius-sm);
  background: var(--scene-surface);
  padding: var(--space-2) var(--space-3);
}
.tech-head { display: flex; justify-content: space-between; gap: var(--space-2); }
.tech-toggle { font-size: var(--font-xs); color: var(--scene-title); }
.tech-body { margin-top: var(--space-2); }
.tech-add { display: flex; gap: var(--space-2); align-items: center; flex-wrap: wrap; }
.input-xs { width: var(--control-input-xs-width); }
.tech-list {
  list-style: none; padding: 0; margin: var(--space-2) 0;
  display: flex; flex-wrap: wrap; gap: var(--space-2) var(--space-4);
}
.tech-list li {
  display: flex; align-items: center; gap: var(--space-2); font-size: var(--font-xs);
}
.tech-list .on { color: var(--scene-state-ok); }
.tech-rules {
  margin: var(--space-2) 0;
  border-top: var(--border-width) dashed var(--scene-line);
  padding-top: var(--space-2);
}
.tech-rule-list {
  list-style: none; padding: 0; margin: var(--space-2) 0;
  display: flex; flex-wrap: wrap; gap: var(--space-1) var(--space-4);
  font-size: var(--font-xs);
}
.tech-bells {
  margin: var(--space-2) 0;
  border-top: var(--border-width) dashed var(--scene-line);
  padding-top: var(--space-2);
}
.tech-report table { width: 100%; border-collapse: collapse; font-size: var(--font-xs); }
.tech-report th,
.tech-report td {
  text-align: left;
  padding: var(--space-0_5) var(--space-2);
  border-bottom: var(--border-width) solid var(--scene-line);
}
</style>
