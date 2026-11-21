<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'
import { api, type CartLine, type ServiceTable } from '../api'

const props = defineProps<{
  activeTableId: number | null
  cart: CartLine[]
  /** 当前已识别的会员，开台时一并绑定，恢复桌台沿用会员价。 */
  memberId: number | null
  /** 建/删桌台需 product.edit：店员只开台点菜，不见管理入口。 */
  canManage: boolean
}>()
const emit = defineEmits<{
  select: [table: ServiceTable, lines: CartLine[]]
  cleared: []
  error: [message: string]
}>()

const tables = ref<ServiceTable[]>([])
const busy = ref(false)
const showCreate = ref(false)
const form = ref({ code: '', name: '', zone: '大厅', seats: 4 })
/** 转台/并台的待选目标模式；设置后下一次点桌台即执行迁移。 */
const actionMode = ref<'transfer' | 'merge' | null>(null)
const actionSource = ref<ServiceTable | null>(null)
const nowTick = ref(Date.now())
let timer: ReturnType<typeof setInterval> | null = null

async function load() {
  try {
    tables.value = await api.listServiceTables('DINING')
  } catch (e) {
    emit('error', String(e))
  }
}

function linesOf(t: ServiceTable): CartLine[] {
  if (t.status !== 'OCCUPIED') return []
  try {
    const parsed = JSON.parse(t.cart_json)
    return Array.isArray(parsed) ? parsed : []
  } catch {
    return []
  }
}

function itemCount(t: ServiceTable): number {
  if (t.table_id === props.activeTableId) return props.cart.reduce((n, l) => n + l.qty, 0)
  return linesOf(t).reduce((n, l) => n + (l.qty || 0), 0)
}

function elapsed(t: ServiceTable): string {
  if (!t.opened_at) return ''
  const ms = nowTick.value - new Date(t.opened_at).getTime()
  const min = Math.max(0, Math.floor(ms / 60000))
  return min >= 60 ? `${Math.floor(min / 60)}时${min % 60}分` : `${min}分钟`
}

async function pick(t: ServiceTable) {
  if (busy.value) return
  if (actionMode.value && actionSource.value) {
    await runAction(t)
    return
  }
  busy.value = true
  try {
    if (t.status === 'FREE') {
      const opened = await api.openServiceTable(t.table_id, '[]', props.memberId)
      emit('select', opened, [])
    } else {
      emit('select', t, linesOf(t))
    }
    await load()
  } catch (e) {
    emit('error', String(e))
  } finally {
    busy.value = false
  }
}

function startAction(mode: 'transfer' | 'merge', t: ServiceTable) {
  actionMode.value = mode
  actionSource.value = t
}

function cancelAction() {
  actionMode.value = null
  actionSource.value = null
}

/** 转台只选空闲桌；并台选另一张占用桌，当前桌作为源桌并入目标。 */
async function runAction(target: ServiceTable) {
  const source = actionSource.value
  const mode = actionMode.value
  if (!source || !mode) return
  if (mode === 'transfer' && target.status !== 'FREE') {
    emit('error', '转台请选择空闲桌台')
    return
  }
  if (mode === 'merge' && (target.status !== 'OCCUPIED' || target.table_id === source.table_id)) {
    emit('error', '并台请选择另一张已开台桌')
    return
  }
  const label = mode === 'transfer' ? '转台' : '并台'
  if (!window.confirm(`${label}：${source.code} → ${target.code}？`)) return
  busy.value = true
  try {
    const updated =
      mode === 'transfer'
        ? await api.transferServiceTable(source.table_id, target.table_id)
        : await api.mergeServiceTables(target.table_id, source.table_id)
    cancelAction()
    emit('select', updated, linesOf(updated))
    await load()
  } catch (e) {
    emit('error', String(e))
  } finally {
    busy.value = false
  }
}

async function create() {
  if (busy.value) return
  busy.value = true
  try {
    await api.createServiceTable('DINING', form.value.code, form.value.name, form.value.zone, form.value.seats, null)
    form.value = { code: '', name: '', zone: form.value.zone, seats: 4 }
    showCreate.value = false
    await load()
  } catch (e) {
    emit('error', String(e))
  } finally {
    busy.value = false
  }
}

async function clear(t: ServiceTable) {
  if (!window.confirm(`未结算清台 ${t.code}？购物车会丢弃，本操作留审计。`)) return
  busy.value = true
  try {
    await api.closeServiceTable(t.table_id, true)
    if (t.table_id === props.activeTableId) emit('cleared')
    await load()
  } catch (e) {
    emit('error', String(e))
  } finally {
    busy.value = false
  }
}

async function remove(t: ServiceTable) {
  if (!window.confirm(`删除空桌台 ${t.code}？`)) return
  busy.value = true
  try {
    await api.deleteServiceTable(t.table_id)
    await load()
  } catch (e) {
    emit('error', String(e))
  } finally {
    busy.value = false
  }
}

onMounted(() => {
  void load()
  timer = setInterval(() => (nowTick.value = Date.now()), 30000)
})
onUnmounted(() => {
  if (timer) clearInterval(timer)
})
</script>

<template>
  <section class="dining-board">
    <div class="board-head">
      <div>
        <strong>餐饮桌台</strong>
        <span class="muted">选中桌台后点菜，结算成功自动清台</span>
      </div>
      <button v-if="canManage" class="btn btn-sm ghost" @click="showCreate = !showCreate">
        {{ showCreate ? '收起' : '＋ 新增桌台' }}
      </button>
    </div>
    <div v-if="actionMode" class="action-hint">
      <span v-if="actionMode === 'transfer'">转台：点击空闲桌台作为目标</span>
      <span v-else>并台：点击另一张已开台桌台，当前 {{ actionSource?.code }} 并入目标</span>
      <button class="link" @click="cancelAction">取消</button>
    </div>
    <div v-if="showCreate" class="create-row">
      <input v-model="form.code" class="input input-sm" placeholder="桌号 A01" maxlength="12" />
      <input v-model="form.name" class="input input-sm" placeholder="名称 大厅1号桌" maxlength="20" />
      <input v-model="form.zone" class="input input-sm" placeholder="区域 大厅" maxlength="12" />
      <input v-model.number="form.seats" class="input input-sm" type="number" min="1" max="50" />
      <button class="btn btn-sm primary" :disabled="busy" @click="create">创建</button>
    </div>
    <div class="table-grid">
      <article
        v-for="t in tables"
        :key="t.table_id"
        :class="['table-card', t.status.toLowerCase(), { active: t.table_id === activeTableId }]"
        @click="pick(t)"
      >
        <header>
          <strong>{{ t.code }}</strong>
          <span>{{ t.zone || '未分区' }} · {{ t.seats }}座</span>
        </header>
        <p>{{ t.name }}<span v-if="t.member_phone" class="member-tag">会员 {{ t.member_phone }}</span></p>
        <div class="card-foot">
          <span v-if="t.status === 'OCCUPIED'" class="occupied">用餐中 · {{ elapsed(t) }}</span>
          <span v-else class="free">空闲</span>
          <span class="count">{{ itemCount(t) }} 件</span>
        </div>
        <div class="card-actions" @click.stop>
          <template v-if="t.status === 'OCCUPIED'">
            <button class="link" @click="startAction('transfer', t)">转台</button>
            <button class="link" @click="startAction('merge', t)">并台</button>
            <button class="link danger" @click="clear(t)">清台</button>
          </template>
          <button v-else-if="canManage" class="link" @click="remove(t)">删除</button>
        </div>
      </article>
      <div v-if="!tables.length" class="empty-board">
        还没有桌台。先新增 A01 等桌号，再开台点菜。
      </div>
    </div>
  </section>
</template>

<style scoped>
.dining-board {
  border-bottom: 1px solid var(--border, #e5e0d8);
  padding: 10px 12px 12px;
  background: linear-gradient(180deg, #fffaf0, #fff);
}
.board-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin-bottom: 8px;
}
.board-head strong {
  color: #8a4b12;
  margin-right: 8px;
}
.create-row {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr)) auto;
  gap: 6px;
  margin-bottom: 8px;
}
.table-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
  gap: 8px;
  max-height: 210px;
  overflow: auto;
}
.table-card {
  border: 1px solid #eadfce;
  border-radius: 12px;
  padding: 9px 10px;
  cursor: pointer;
  background: #fff;
  transition: border-color 0.15s, transform 0.15s;
}
.table-card:hover {
  transform: translateY(-1px);
}
.table-card.active {
  border-color: #c2701d;
  box-shadow: 0 0 0 2px #f5dcc0 inset;
}
.table-card.occupied {
  background: #fff7ea;
}
.table-card header {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  font-size: 13px;
}
.table-card header strong {
  font-size: 17px;
  color: #7a3f0c;
}
.table-card header span,
.table-card p {
  color: #9a8a76;
  font-size: 12px;
}
.table-card p {
  margin: 3px 0 5px;
}
.card-foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 12px;
}
.occupied {
  color: #b45309;
  font-weight: 600;
}
.free {
  color: #4b7a4b;
}
.count {
  color: #6b6258;
}
.card-actions {
  margin-top: 4px;
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}
.action-hint {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 8px;
  background: #fff3d8;
  border: 1px solid #f0d39a;
  border-radius: 8px;
  padding: 6px 10px;
  margin-bottom: 8px;
  font-size: 12px;
  color: #8a4b12;
}
.member-tag {
  color: #2f6b4f;
  background: #e8f4ec;
  border-radius: 999px;
  padding: 1px 6px;
  margin-left: 6px;
  font-size: 11px;
}
.link {
  border: 0;
  background: none;
  padding: 0;
  font-size: 11px;
  color: #9a8a76;
  cursor: pointer;
}
.link.danger {
  color: #b42318;
}
.empty-board {
  grid-column: 1 / -1;
  color: #9a8a76;
  padding: 14px;
  border: 1px dashed #e2d7c8;
  border-radius: 10px;
}
</style>
