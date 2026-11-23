<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'
import { api, formatMoney, type CartLine, type ProductHit, type ServiceTable, type TableCreateSpec } from '../api'
import TableCreateDialog from './TableCreateDialog.vue'

/**
 * 包间看板：棋牌（CHESS）/养生（WELLNESS）共用。
 * 点击空闲包间即开台计时，房费按分钟由收银台自动生成，最低 60 分钟。
 */
const props = defineProps<{
  scene: 'CHESS' | 'WELLNESS'
  /** 看板标题与提示语 */
  title: string
  hint: string
  /** 配色主题：chess 深绿金 / wellness 浅绿 */
  theme: 'chess' | 'wellness'
  /** 批量创建默认前缀 */
  defaultPrefix: string
  activeTableId: number | null
  cart: CartLine[]
  memberId: number | null
  /** 建/删包间需 product.edit：店员只开台与续费。 */
  canManage: boolean
}>()
const emit = defineEmits<{
  select: [table: ServiceTable, lines: CartLine[]]
  cleared: []
  error: [message: string]
}>()

const rooms = ref<ServiceTable[]>([])
const feeProducts = ref<ProductHit[]>([])
const busy = ref(false)
const showCreate = ref(false)
const transferSource = ref<ServiceTable | null>(null)
const tick = ref(Date.now())
let timer: ReturnType<typeof setInterval> | null = null

async function load() {
  try {
    rooms.value = await api.listServiceTables(props.scene)
  } catch (e) {
    emit('error', String(e))
  }
}

async function loadFees() {
  try {
    feeProducts.value = await api.lookup('', 200, '房费')
  } catch (e) {
    emit('error', String(e))
  }
}

function feeOf(t: ServiceTable): ProductHit | null {
  return feeProducts.value.find((p) => p.sku_id === t.fee_sku_id) ?? null
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

function elapsed(t: ServiceTable): string {
  if (!t.opened_at) return '00:00'
  const sec = Math.max(0, Math.floor((tick.value - new Date(t.opened_at).getTime()) / 1000))
  const h = String(Math.floor(sec / 3600)).padStart(2, '0')
  const m = String(Math.floor((sec % 3600) / 60)).padStart(2, '0')
  const s = String(sec % 60).padStart(2, '0')
  return `${h}:${m}:${s}`
}

function itemCount(t: ServiceTable): number {
  if (t.table_id === props.activeTableId) return props.cart.reduce((n, l) => n + l.qty, 0)
  return linesOf(t).reduce((n, l) => n + (l.qty || 0), 0)
}

async function pick(t: ServiceTable) {
  if (busy.value) return
  if (transferSource.value) {
    await runTransfer(t)
    return
  }
  busy.value = true
  try {
    if (t.status === 'FREE') {
      const fee = feeOf(t)
      if (!fee) {
        emit('error', '该包间未绑定房费商品，请先删除后重建并选择房费项目')
        return
      }
      // 房费不再预收 1 小时；开台由收银台按分钟计时生成房费行，最低 60 分钟。
      const opened = await api.openServiceTable(t.table_id, '[]', props.memberId)
      emit('select', opened, linesOf(opened))
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

function startTransfer(t: ServiceTable) {
  transferSource.value = t
}

async function runTransfer(target: ServiceTable) {
  const source = transferSource.value
  if (!source) return
  if (target.status !== 'FREE') {
    emit('error', '转台请选择空闲包间')
    return
  }
  if (!window.confirm(`转台：${source.code} → ${target.code}？计时与消费继续保留。`)) return
  busy.value = true
  try {
    const updated = await api.transferServiceTable(source.table_id, target.table_id)
    transferSource.value = null
    emit('select', updated, linesOf(updated))
    await load()
  } catch (e) {
    emit('error', String(e))
  } finally {
    busy.value = false
  }
}

/** 批量创建：逐个写入，遇到重名等错误即停止并保留已创建的部分 */
async function createBatch(items: TableCreateSpec[]) {
  if (busy.value) return
  busy.value = true
  try {
    let done = 0
    for (const it of items) {
      try {
        await api.createServiceTable(
          props.scene, it.code, it.name, it.zone, it.seats,
          it.feeSkuId, it.feeUnit, it.minMinutes, it.roundMinutes
        )
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
  if (!window.confirm(`未结算清台 ${t.code}？本操作留审计。`)) return
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
  if (!window.confirm(`删除空包间 ${t.code}？`)) return
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
  void loadFees()
  timer = setInterval(() => (tick.value = Date.now()), 1000)
})
onUnmounted(() => {
  if (timer) clearInterval(timer)
})
</script>

<template>
  <section class="room-board" :class="`room-board--${theme}`">
    <div class="board-head">
      <div><strong>{{ title }}</strong><span>{{ hint }}</span></div>
      <button v-if="canManage" class="btn btn-sm ghost" @click="showCreate = true">
        ＋ 批量新增包间
      </button>
    </div>
    <div v-if="transferSource" class="transfer-hint">
      <span>转台：点击空闲包间，当前 {{ transferSource.code }} 的计时与消费会迁过去</span>
      <button class="link" @click="transferSource = null">取消</button>
    </div>
    <div class="room-grid">
      <article
        v-for="r in rooms"
        :key="r.table_id"
        :class="['room-card', r.status.toLowerCase(), { active: r.table_id === activeTableId }]"
        @click="pick(r)"
      >
        <header><strong>{{ r.code }}</strong><span>{{ r.zone || '未分区' }} · {{ r.seats }}人</span></header>
        <p>{{ r.name }}<span v-if="r.member_phone" class="member-tag">会员 {{ r.member_phone }}</span></p>
        <div class="clock">{{ r.status === 'OCCUPIED' ? elapsed(r) : '待开台' }}</div>
        <div class="card-foot">
          <span v-if="feeOf(r)" class="fee">{{ feeOf(r)?.spu_name }} ¥{{ formatMoney(feeOf(r)!.price_cents) }}{{ r.fee_unit === 'HOUR' ? '/小时' : r.fee_unit === 'MINUTE' ? '/分钟' : '（单位按门店配置）' }}</span>
          <span v-else class="fee muted">未绑定房费</span>
          <span class="count">{{ itemCount(r) }} 件</span>
        </div>
        <div class="card-actions" @click.stop>
          <button v-if="r.status === 'OCCUPIED'" class="link" @click="startTransfer(r)">转台</button>
          <button v-if="r.status === 'OCCUPIED'" class="link danger" @click="clear(r)">清台</button>
          <button v-else-if="canManage" class="link" @click="remove(r)">删除</button>
        </div>
      </article>
      <div v-if="!rooms.length" class="empty-board">
        还没有包间。先在商品里建“房费”分类和房费商品，再批量新增包间并绑定。
      </div>
    </div>
    <TableCreateDialog
      :open="showCreate"
      :title="title"
      :show-fee="true"
      :fee-products="feeProducts"
      :default-prefix="defaultPrefix"
      @close="showCreate = false"
      @create="createBatch"
    />
  </section>
</template>

<style scoped>
.room-board {
  border-bottom: 1px solid var(--room-border, #123c2f);
  padding: 10px 12px 12px;
  background: var(--room-bg, radial-gradient(circle at 20% 0%, #1d5741, #0d2a21 70%));
  color: var(--room-text, #edf7f1);
}
/* 养生主题：浅绿底深字 */
.room-board--wellness {
  --room-border: #dfe7dc;
  --room-bg: linear-gradient(180deg, #f6f9f3, #fcfbf7);
  --room-text: #33503c;
  --room-strong: #3f6148;
  --room-sub: #849083;
  --room-card-bg: #fff;
  --room-card-border: #e3e9df;
  --room-card-hover: #fbfdf9;
  --room-active: #6d9b76;
  --room-active-ring: rgba(109, 155, 118, 0.35);
  --room-occupied-bg: #f2f7ef;
  --room-head-sub: #849083;
  --room-clock: #3f6148;
  --room-fee: #4f7357;
  --room-link: #4f7357;
  --room-link-danger: #b42318;
  --room-hint-bg: #eef5ea;
  --room-hint-border: #c9dcc4;
  --room-hint-text: #3f6148;
  --room-empty-border: #c9dcc4;
  --room-empty-text: #849083;
  --room-btn: #3f6148;
  --room-btn-border: #a9c0a8;
}
.room-board--chess {
  --room-strong: #f1d38b;
  --room-sub: #a9c9b8;
  --room-head-sub: #a9c9b8;
  --room-card-bg: rgba(255, 255, 255, 0.05);
  --room-card-border: #2c5f4b;
  --room-card-hover: rgba(255, 255, 255, 0.09);
  --room-active: #f1d38b;
  --room-active-ring: rgba(241, 211, 139, 0.35);
  --room-occupied-bg: rgba(241, 211, 139, 0.08);
  --room-clock: #fff3cf;
  --room-fee: #a9c9b8;
  --room-link: #f1d38b;
  --room-link-danger: #ffb4a8;
  --room-hint-bg: rgba(241, 211, 139, 0.12);
  --room-hint-border: #7d6a3a;
  --room-hint-text: #f1d38b;
  --room-empty-border: #3a6f59;
  --room-empty-text: #a9c9b8;
  --room-btn: #dfeee5;
  --room-btn-border: #3a6f59;
}
.board-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 8px;
}
.board-head strong {
  color: var(--room-strong);
  margin-right: 8px;
}
.board-head span {
  color: var(--room-head-sub);
  font-size: 12px;
}
.board-head .btn.ghost {
  color: var(--room-btn);
  border-color: var(--room-btn-border);
}
.transfer-hint {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 8px;
  border: 1px solid var(--room-hint-border);
  background: var(--room-hint-bg);
  color: var(--room-hint-text);
  border-radius: 8px;
  padding: 6px 10px;
  margin-bottom: 8px;
  font-size: 12px;
}
.member-tag {
  color: var(--room-strong);
  background: var(--room-hint-bg);
  border-radius: 999px;
  padding: 1px 6px;
  margin-left: 6px;
  font-size: 11px;
}
.room-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
  gap: 8px;
  max-height: 215px;
  overflow: auto;
}
.room-card {
  border: 1px solid var(--room-card-border);
  border-radius: 10px;
  background: var(--room-card-bg);
  padding: 9px 10px;
  cursor: pointer;
}
.room-card:hover {
  background: var(--room-card-hover);
}
.room-card.active {
  border-color: var(--room-active);
  box-shadow: 0 0 0 2px var(--room-active-ring) inset;
}
.room-card.occupied {
  background: var(--room-occupied-bg);
}
.room-card header {
  display: flex;
  justify-content: space-between;
  font-size: 12px;
  color: var(--room-sub);
}
.room-card header strong {
  color: var(--room-strong);
  font-size: 16px;
}
.room-card p {
  margin: 2px 0;
  font-size: 12px;
  color: var(--room-sub);
}
.clock {
  font-family: var(--font-mono, ui-monospace, monospace);
  font-size: 20px;
  letter-spacing: 1px;
  color: var(--room-clock);
}
.card-foot {
  display: flex;
  justify-content: space-between;
  font-size: 11px;
  color: var(--room-fee);
  gap: 6px;
}
.fee {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.card-actions {
  display: flex;
  gap: 10px;
  margin-top: 4px;
}
.link {
  border: 0;
  background: none;
  padding: 0;
  color: var(--room-link);
  font-size: 11px;
  cursor: pointer;
}
.link.danger {
  color: var(--room-link-danger);
}
.empty-board {
  grid-column: 1 / -1;
  color: var(--room-empty-text);
  border: 1px dashed var(--room-empty-border);
  border-radius: 10px;
  padding: 14px;
}
</style>
