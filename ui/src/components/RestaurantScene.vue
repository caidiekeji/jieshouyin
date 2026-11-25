<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'
import { api, type CartLine, type ServiceTable, type TableCreateSpec } from '../api'
import SceneBoard from './SceneBoard.vue'
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

onMounted(() => {
  void load()
  timer = setInterval(() => (nowTick.value = Date.now()), 30000)
})
onUnmounted(() => {
  if (timer) clearInterval(timer)
})
</script>

<template>
  <SceneBoard
    class="scene-dining"
    title="餐饮桌台"
    :min-card="140"
    :empty-text="tables.length ? '' : '还没有桌台。先批量新增 A01 等桌号，再进入点菜。'"
    :notice="actionMode === 'transfer'
      ? '转台：点击空闲桌台作为目标'
      : actionMode === 'merge' ? `并台：点击另一张已开台桌台，当前 ${actionSource?.code} 并入目标` : ''"
    @cancel="cancelAction"
  >
    <template #action>
      <button v-if="canManage" class="btn btn-sm ghost" @click="showCreate = true">
        ＋ 批量新增桌台
      </button>
    </template>
    <article
      v-for="t in tables"
      :key="t.table_id"
      :class="['scene-card', t.status.toLowerCase(), { active: t.table_id === activeTableId }]"
      @click="pick(t)"
    >
      <header>
        <strong>{{ t.code }}</strong>
        <span>{{ t.zone || '未分区' }} · {{ t.seats }}座</span>
      </header>
      <p>{{ t.name }}<span v-if="t.member_phone" class="scene-member-tag">会员 {{ t.member_phone }}</span></p>
      <div class="scene-foot">
        <span v-if="t.status === 'OCCUPIED'" class="occupied-state">用餐中 · {{ elapsed(t) }}</span>
        <span v-else class="free-state">空闲</span>
        <span>{{ itemCount(t) }} 件</span>
      </div>
      <div v-if="t.status === 'OCCUPIED'" class="scene-actions" @click.stop>
        <button class="link" @click="startAction('transfer', t)">转台</button>
        <button class="link" @click="startAction('merge', t)">并台</button>
        <button class="link danger" @click="clear(t)">清台</button>
      </div>
    </article>
  </SceneBoard>
  <TableCreateDialog
    :open="showCreate"
    title="餐饮桌台"
    capacity-label="座位"
    :capacity-default="4"
    :show-fee="false"
    :fee-products="[]"
    default-prefix="A"
    @close="showCreate = false"
    @create="createBatch"
  />
</template>

<style scoped>
/* 卡片内部排版差异：桌号放大 + 用餐时长计时，行业色全部来自 --scene-* 语义层 */
.scene-card header strong { font-size: var(--font-lg); }
</style>
