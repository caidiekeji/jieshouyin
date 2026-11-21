<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'
import { api, formatMoney, type CartLine, type ProductHit, type ServiceTable } from '../api'

const props = defineProps<{
  activeTableId: number | null
  cart: CartLine[]
  memberId: number | null
  /** 建/删包间需 product.edit：店员只开台与续费。 */
  canManage: boolean
}>()
const emit = defineEmits<{
  select: [table: ServiceTable, lines: CartLine[]]
  add: [product: ProductHit]
  cleared: []
  error: [message: string]
}>()

const rooms = ref<ServiceTable[]>([])
const feeProducts = ref<ProductHit[]>([])
const busy = ref(false)
const showCreate = ref(false)
const form = ref({
  code: '',
  name: '',
  zone: '大厅',
  seats: 4,
  feeSkuId: null as number | null,
  feeUnit: '' as '' | 'MINUTE' | 'HOUR',
  minMinutes: '' as number | '',
  roundMinutes: '' as number | '',
})
const transferSource = ref<ServiceTable | null>(null)
const tick = ref(Date.now())
let timer: ReturnType<typeof setInterval> | null = null

async function load() {
  try {
    rooms.value = await api.listServiceTables('CHESS')
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

async function create() {
  if (busy.value) return
  if (!form.value.feeSkuId) {
    emit('error', '请选择房费商品，并按“每分钟/每小时”维护好单价')
    return
  }
  if (form.value.minMinutes !== '' && form.value.minMinutes < 0) {
    emit('error', '最低计费分钟不能为负')
    return
  }
  if (form.value.roundMinutes !== '' && form.value.roundMinutes < 0) {
    emit('error', '取整分钟不能为负')
    return
  }
  busy.value = true
  try {
    await api.createServiceTable(
      'CHESS',
      form.value.code,
      form.value.name,
      form.value.zone,
      form.value.seats,
      form.value.feeSkuId,
      form.value.feeUnit || null,
      form.value.minMinutes === '' ? null : Number(form.value.minMinutes),
      form.value.roundMinutes === '' ? null : Number(form.value.roundMinutes)
    )
    form.value = {
      code: '',
      name: '',
      zone: form.value.zone,
      seats: 4,
      feeSkuId: null,
      feeUnit: form.value.feeUnit,
      minMinutes: form.value.minMinutes,
      roundMinutes: form.value.roundMinutes,
    }
    showCreate.value = false
    await load()
  } catch (e) {
    emit('error', String(e))
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
  <section class="chess-board">
    <div class="board-head">
      <div><strong>棋牌包间</strong><span>按分钟计时，最低 60 分钟；开台不再预收整小时房费</span></div>
      <button v-if="canManage" class="btn btn-sm ghost" @click="showCreate = !showCreate">
        {{ showCreate ? '收起' : '＋ 新增包间' }}
      </button>
    </div>
    <div v-if="transferSource" class="transfer-hint">
      <span>转台：点击空闲包间，当前 {{ transferSource.code }} 的计时与消费会迁过去</span>
      <button class="link" @click="transferSource = null">取消</button>
    </div>
    <div v-if="showCreate" class="create-row">
      <input v-model="form.code" class="input input-sm" placeholder="包间号 B01" maxlength="12" />
      <input v-model="form.name" class="input input-sm" placeholder="名称 二楼包间" maxlength="20" />
      <input v-model="form.zone" class="input input-sm" placeholder="区域 二楼" maxlength="12" />
      <input v-model.number="form.seats" class="input input-sm" type="number" min="1" max="50" />
      <select v-model.number="form.feeSkuId" class="input input-sm">
        <option :value="null" disabled>选择房费商品</option>
        <option v-for="p in feeProducts" :key="p.sku_id" :value="p.sku_id">
          {{ p.spu_name }} ¥{{ formatMoney(p.price_cents) }}
        </option>
      </select>
      <select v-model="form.feeUnit" class="input input-sm" aria-label="房费计价单位">
        <option value="">跟随门店配置</option>
        <option value="MINUTE">单价/分钟</option>
        <option value="HOUR">单价/小时</option>
      </select>
      <label class="muted small">最低
        <input v-model.number="form.minMinutes" class="input input-xs" type="number" min="0" max="1440" placeholder="跟随" />
        分钟
      </label>
      <label class="muted small">取整
        <input v-model.number="form.roundMinutes" class="input input-xs" type="number" min="0" max="120" placeholder="跟随" />
        分钟
      </label>
      <button class="btn btn-sm primary" :disabled="busy" @click="create">创建</button>
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
        还没有包间。先在商品里建“房费”分类和“每分钟房费”，再新增包间并绑定。
      </div>
    </div>
  </section>
</template>

<style scoped>
.chess-board {
  border-bottom: 1px solid #123c2f;
  padding: 10px 12px 12px;
  background: radial-gradient(circle at 20% 0%, #1d5741, #0d2a21 70%);
  color: #edf7f1;
}
.board-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 8px;
}
.board-head strong {
  color: #f1d38b;
  margin-right: 8px;
}
.board-head span {
  color: #a9c9b8;
  font-size: 12px;
}
.board-head .btn.ghost {
  color: #dfeee5;
  border-color: #3a6f59;
}
.transfer-hint {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 8px;
  border: 1px solid #7d6a3a;
  background: rgba(241, 211, 139, 0.12);
  color: #f1d38b;
  border-radius: 8px;
  padding: 6px 10px;
  margin-bottom: 8px;
  font-size: 12px;
}
.member-tag {
  color: #bff0d8;
  background: rgba(76, 175, 125, 0.18);
  border-radius: 999px;
  padding: 1px 6px;
  margin-left: 6px;
  font-size: 11px;
}
.create-row .input-xs {
  width: 56px;
}
.create-row {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr)) minmax(0, 1.4fr) auto;
  gap: 6px;
  margin-bottom: 8px;
}
.room-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
  gap: 8px;
  max-height: 215px;
  overflow: auto;
}
.room-card {
  border: 1px solid #2c5f4b;
  border-radius: 10px;
  background: rgba(255, 255, 255, 0.05);
  padding: 9px 10px;
  cursor: pointer;
}
.room-card.active {
  border-color: #f1d38b;
  box-shadow: 0 0 0 2px rgba(241, 211, 139, 0.35) inset;
}
.room-card.occupied {
  background: rgba(241, 211, 139, 0.08);
}
.room-card header {
  display: flex;
  justify-content: space-between;
  font-size: 12px;
  color: #c9ded2;
}
.room-card header strong {
  color: #f1d38b;
  font-size: 16px;
}
.room-card p {
  margin: 2px 0;
  font-size: 12px;
  color: #9fc0ae;
}
.clock {
  font-family: var(--font-mono, ui-monospace, monospace);
  font-size: 20px;
  letter-spacing: 1px;
  color: #fff3cf;
}
.card-foot {
  display: flex;
  justify-content: space-between;
  font-size: 11px;
  color: #a9c9b8;
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
  color: #f1d38b;
  font-size: 11px;
  cursor: pointer;
}
.link.danger {
  color: #ffb4a8;
}
.empty-board {
  grid-column: 1 / -1;
  color: #a9c9b8;
  border: 1px dashed #3a6f59;
  border-radius: 10px;
  padding: 14px;
}
</style>
