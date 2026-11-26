<script setup lang="ts">
import { formatMoney, type CartLine, type ServiceTable } from '../api'
import { useServiceRooms } from '../composables/useServiceRooms'
import SceneBoard from './SceneBoard.vue'
import TableCreateDialog from './TableCreateDialog.vue'

/** 棋牌包间独立页面：卡片看板 + 计时钟 + 转台，仅复用通用组件。 */
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
  scene: 'CHESS',
  roomNoun: '包间',
  activeTableId: () => props.activeTableId,
  cart: () => props.cart,
  memberId: () => props.memberId,
  onSelect: (t, lines) => emit('select', t, lines),
  onCleared: () => emit('cleared'),
  onError: (message) => emit('error', message),
})
</script>

<template>
  <SceneBoard
    class="scene-chess"
    :title="title"
    :min-card="150"
    :empty-text="rooms.length ? '' : '还没有棋牌包间。先在商品里建“房费”分类和房费商品，再批量新增包间并绑定。'"
    :notice="transferSource ? `转台：点击空闲包间，当前 ${transferSource.code} 的计时与消费会迁过去` : ''"
    @cancel="transferSource = null"
  >
    <template #action>
      <button v-if="canManage" class="btn btn-sm ghost" @click="showCreate = true">＋ 批量新增包间</button>
    </template>
    <article
      v-for="room in rooms"
      :key="room.table_id"
      :class="['scene-card', room.status.toLowerCase(), { active: room.table_id === activeTableId }]"
      @click="pick(room)"
    >
      <header><strong>{{ room.code }}</strong><span>{{ room.zone || '未分区' }} · {{ room.seats }}人</span></header>
      <p>{{ room.name }}<span v-if="room.member_phone" class="scene-member-tag">会员 {{ room.member_phone }}</span></p>
      <div class="clock">{{ room.status === 'OCCUPIED' ? elapsed(room) : '待开台' }}</div>
      <div class="scene-foot">
        <span v-if="feeOf(room)" class="fee">{{ feeOf(room)!.spu_name }} ¥{{ formatMoney(feeOf(room)!.price_cents) }}{{ room.fee_unit === 'HOUR' ? '/小时' : '/分钟' }}</span>
        <span v-else class="fee muted">未绑定房费</span>
        <span>{{ itemCount(room) }} 件</span>
      </div>
      <div v-if="room.status === 'OCCUPIED'" class="scene-actions" @click.stop>
        <button class="link" @click="startTransfer(room)">转台</button>
        <button class="link danger" @click="clear(room)">清台</button>
      </div>
    </article>
  </SceneBoard>
  <TableCreateDialog
    :open="showCreate"
    title="棋牌包间"
    capacity-label="人数"
    :capacity-default="4"
    :show-fee="true"
    :fee-products="feeProducts"
    :default-prefix="defaultPrefix"
    @close="showCreate = false"
    @create="createBatch"
  />
</template>

<style scoped>
/* 棋牌：深绿卡面 + 金色计时钟，卡片密度高于养生服务间 */
.scene-card header strong { font-size: var(--font-md); }
.clock { color: var(--scene-accent, var(--brand)); font-size: var(--font-xl); font-variant-numeric: tabular-nums; }
</style>
