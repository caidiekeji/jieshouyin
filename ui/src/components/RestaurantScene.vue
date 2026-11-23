<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'
import { api, type CartLine, type ServiceTable, type TableCreateSpec } from '../api'
import TableCreateDialog from './TableCreateDialog.vue'

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
  // 空闲桌台只进入点菜界面，不立即开台；加入第一道菜时才真正占用（见 pos.addToCart）
  emit('select', t, linesOf(t))
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

/** 批量创建桌台：逐个写入，遇到重名等错误即停止并保留已创建的部分 */
async function createBatch(items: TableCreateSpec[]) {
  if (busy.value) return
  busy.value = true
  try {
    let done = 0
    for (const it of items) {
      try {
        await api.createServiceTable('DINING', it.code, it.name, it.zone, it.seats, null)
        done++
      } catch (e) {
        emit('error', done ? `已创建 ${done}/${items.length}，${it.code} 失败：${String(e)}` : String(e))
        break
      }
    }
    showCreate.value = false
    await load()
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
  <section class="dining-board scene-dining">
    <div class="board-head">
      <div>
        <strong>餐饮桌台</strong>
        <span class="muted">点击空闲桌台进入点菜，加入菜品后自动开台；结算成功自动清台</span>
      </div>
      <button v-if="canManage" class="btn btn-sm ghost" @click="showCreate = true">
        ＋ 批量新增桌台
      </button>
    </div>
    <div v-if="actionMode" class="action-hint">
      <span v-if="actionMode === 'transfer'">转台：点击空闲桌台作为目标</span>
      <span v-else>并台：点击另一张已开台桌台，当前 {{ actionSource?.code }} 并入目标</span>
      <button class="link" @click="cancelAction">取消</button>
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
        还没有桌台。先批量新增 A01 等桌号，再进入点菜。
      </div>
    </div>
    <TableCreateDialog
      :open="showCreate"
      title="餐饮桌台"
      :show-fee="false"
      :fee-products="[]"
      default-prefix="A"
      @close="showCreate = false"
      @create="createBatch"
    />
  </section>
</template>

<style scoped>
/* 配色来自场景语义层（tokens.css .scene-dining），暗色只翻语义层。 */
.dining-board {
  border-bottom: var(--border-width) solid var(--scene-line);
  padding: var(--space-2) var(--space-3) var(--space-3);
  background: var(--scene-surface);
  color: var(--scene-title);
}
.board-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
  margin-bottom: var(--space-2);
}
.board-head strong { color: var(--scene-title); margin-right: var(--space-2); }
.table-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
  gap: var(--space-2);
  max-height: var(--scene-board-max-height);
  overflow: auto;
}
.table-card {
  border: var(--border-width) solid var(--scene-card-line);
  border-radius: var(--radius-sm);
  padding: var(--space-2) var(--space-3);
  cursor: pointer;
  background: var(--scene-card);
  transition: border-color var(--duration-fast) var(--ease-out),
              background var(--duration-fast) var(--ease-out);
}
.table-card:hover { background: var(--scene-card-hover); }
.table-card:focus-visible { outline: 2px solid var(--scene-card-active-line); outline-offset: 2px; }
.table-card.active {
  border-color: var(--scene-card-active-line);
  box-shadow: 0 0 0 2px var(--scene-card-active-ring) inset;
}
.table-card.occupied { background: var(--scene-card-occupied); }
.table-card header {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  font-size: var(--font-sm);
}
.table-card header strong { font-size: var(--font-lg); color: var(--scene-heading); }
.table-card header span,
.table-card p { color: var(--scene-sub); font-size: var(--font-xs); }
.table-card p { margin: var(--space-0_5) 0 var(--space-1); }
.card-foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: var(--font-xs);
}
.occupied { color: var(--scene-state-warn); font-weight: 600; }
.free { color: var(--scene-state-ok); }
.count { color: var(--scene-sub); }
.card-actions {
  margin-top: var(--space-1);
  display: flex;
  gap: var(--space-2);
  flex-wrap: wrap;
}
.action-hint {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: var(--space-2);
  background: var(--scene-hint-bg);
  border: var(--border-width) solid var(--scene-hint-line);
  border-radius: var(--radius-xs);
  padding: var(--space-1) var(--space-3);
  margin-bottom: var(--space-2);
  font-size: var(--font-xs);
  color: var(--scene-hint-text);
}
.member-tag {
  color: var(--scene-tag-text);
  background: var(--scene-tag-bg);
  border-radius: var(--radius-xs);
  padding: 0 var(--space-2);
  margin-left: var(--space-1);
  font-size: var(--font-2xs);
}
.link {
  border: 0; background: none; padding: 0;
  font-size: var(--font-2xs); color: var(--scene-link); cursor: pointer;
}
.link:hover { text-decoration: underline; }
.link.danger { color: var(--scene-danger); }
.empty-board {
  grid-column: 1 / -1;
  color: var(--scene-sub);
  padding: var(--space-4);
  border: var(--border-width) dashed var(--scene-empty-line);
  border-radius: var(--radius-sm);
}
</style>
