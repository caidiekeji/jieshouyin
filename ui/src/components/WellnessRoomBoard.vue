<script setup lang="ts">
import { onMounted, reactive, ref } from 'vue'
import {
  api,
  formatMoney,
  type CartLine,
  type ServiceTable,
  type Technician,
  type TechnicianBell,
} from '../api'
import { useServiceRooms } from '../composables/useServiceRooms'
import SceneBoard from './SceneBoard.vue'
import TableCreateDialog from './TableCreateDialog.vue'

/** 养生服务间独立页面：床位列表 + 服务项目计数，不复用棋牌包间页面。 */
const props = defineProps<{
  title: string
  defaultPrefix: string
  activeTableId: number | null
  cart: CartLine[]
  memberId: number | null
  canManage: boolean
}>()
const emit = defineEmits<{
  select: [table: ServiceTable, lines: CartLine[]]
  cleared: []
  error: [message: string]
}>()

const {
  rooms, feeProducts, showCreate, transferSource,
  feeOf, elapsed, itemCount, pick, startTransfer, createBatch, clear,
} = useServiceRooms({
  scene: 'WELLNESS',
  roomNoun: '服务间',
  activeTableId: () => props.activeTableId,
  cart: () => props.cart,
  memberId: () => props.memberId,
  onSelect: (t, lines) => emit('select', t, lines),
  onCleared: () => emit('cleared'),
  onError: (message) => emit('error', message),
})

/** 钟态：排钟/点钟/加钟/收钟只改变技师占用与服务时长，房费仍按桌台计时 */
function today(): string {
  const d = new Date()
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
}
const businessDate = today()
const bells = ref<TechnicianBell[]>([])
const onDuty = ref<Technician[]>([])
const pointed = reactive<Record<number, number>>({})

async function reloadBells() {
  try {
    ;[bells.value, onDuty.value] = await Promise.all([
      api.listBells(businessDate, 'ACTIVE'),
      api.listTechnicians(businessDate),
    ])
  } catch (e) {
    emit('error', String(e))
  }
}
onMounted(reloadBells)

function bellOf(room: ServiceTable): TechnicianBell | undefined {
  return bells.value.find((b) => b.table_id === room.table_id)
}

async function runAction(fn: () => Promise<unknown>) {
  try {
    await fn()
    await reloadBells()
  } catch (e) {
    emit('error', String(e))
  }
}
</script>

<template>
  <SceneBoard
    class="scene-wellness-room"
    :title="title"
    :min-card="260"
    :empty-text="rooms.length ? '' : '还没有服务间。先在商品里建“房费”项目，再批量新增服务间并绑定床位。'"
    :notice="transferSource ? `转间：点击空闲服务间，当前 ${transferSource.code} 的计时与消费会迁过去` : ''"
    @cancel="transferSource = null"
  >
    <template #action>
      <button v-if="canManage" class="btn btn-sm ghost" @click="showCreate = true">＋ 批量新增服务间</button>
    </template>
    <article
      v-for="room in rooms"
      :key="room.table_id"
      :class="['room-row', room.status.toLowerCase(), { active: room.table_id === activeTableId }]"
      @click="pick(room)"
    >
      <span class="bed-badge">{{ room.seats }}床</span>
      <div class="room-main">
        <strong>{{ room.code }}</strong>
        <span>{{ room.name }} · {{ room.zone || '未分区' }}</span>
        <span class="fee">{{ feeOf(room) ? `${feeOf(room)!.spu_name} ¥${formatMoney(feeOf(room)!.price_cents)}` : '未绑定房费' }}</span>
      </div>
      <div class="room-side">
        <strong class="clock">{{ room.status === 'OCCUPIED' ? elapsed(room) : '待开台' }}</strong>
        <span>{{ itemCount(room) }} 项</span>
        <span v-if="bellOf(room)" class="bell-now">{{ bellOf(room)!.technician_name }} · 上钟</span>
      </div>
      <div v-if="room.status === 'OCCUPIED'" class="bell-actions" @click.stop>
        <template v-if="bellOf(room)">
          <span class="bell-now">{{ bellOf(room)!.minutes }} 分钟</span>
          <button class="link" @click="runAction(() => api.addBellMinutes(bellOf(room)!.bell_id, 30))">加钟30</button>
          <button class="link" @click="runAction(() => api.finishBell(bellOf(room)!.bell_id))">收钟</button>
        </template>
        <template v-else>
          <button class="link" @click="runAction(() => api.autoAssignBell(room.table_id, businessDate, 60))">排钟</button>
          <select
            v-model.number="pointed[room.table_id]"
            class="input input-sm tech-select"
            :aria-label="`${room.code} 点钟技师`"
          >
            <option :value="0">点钟技师</option>
            <option v-for="t in onDuty.filter((t) => t.on_duty)" :key="t.technician_id" :value="t.technician_id">
              {{ t.name }}
            </option>
          </select>
          <button
            class="link"
            :disabled="!pointed[room.table_id]"
            @click="runAction(() => api.assignBell(pointed[room.table_id], room.table_id, businessDate, 60, true))"
          >点钟</button>
        </template>
      </div>
      <div v-if="room.status === 'OCCUPIED'" class="scene-actions" @click.stop>
        <button class="link" @click="startTransfer(room)">转间</button>
        <button class="link danger" @click="clear(room)">清台</button>
      </div>
    </article>
  </SceneBoard>
  <TableCreateDialog
    :open="showCreate"
    title="养生服务间"
    capacity-label="床位"
    :capacity-default="1"
    :show-fee="true"
    :fee-products="feeProducts"
    :default-prefix="defaultPrefix"
    @close="showCreate = false"
    @create="createBatch"
  />
</template>

<style scoped>
/* 养生：横向床位行，触控面积大；颜色只取场景语义层 token */
.room-row {
  display: flex; align-items: center; gap: var(--space-3);
  padding: var(--space-3); border: var(--border-width) solid var(--scene-line);
  border-radius: var(--radius-xs); background: var(--scene-surface); cursor: pointer;
}
.room-row.occupied { border-color: var(--scene-accent, var(--brand-border)); }
.bed-badge {
  min-width: var(--control-input-xs-width); padding: var(--space-1) var(--space-2);
  border-radius: var(--radius-xs); background: var(--scene-soft-bg, var(--brand-subtle));
  color: var(--scene-soft-text, var(--brand)); text-align: center; font-size: var(--font-sm);
}
.room-main { display: flex; flex: 1; flex-direction: column; gap: var(--space-0_5); min-width: 0; }
.room-main .fee { color: var(--scene-sub); font-size: var(--font-xs); }
.room-side { display: flex; flex-direction: column; align-items: flex-end; gap: var(--space-0_5); }
.room-side .clock { color: var(--scene-title); font-size: var(--font-md); font-variant-numeric: tabular-nums; }
.room-side span { color: var(--scene-sub); font-size: var(--font-xs); }
.bell-now { color: var(--scene-accent, var(--brand)); font-size: var(--font-xs); }
.bell-actions {
  display: flex; align-items: center; gap: var(--space-1); flex-wrap: wrap;
  padding-top: var(--space-1);
}
.bell-actions .tech-select { max-width: var(--control-input-xs-width); }
</style>
