<script setup lang="ts">
import { onMounted, ref, watch } from 'vue'
import { api, formatMoney, today, type OrderDetail, type OrderSummary } from '../api'
import Icon from './Icon.vue'
import {
  ERROR_DURATION_MS,
  ICON_SIZE,
  ORDER_LIST_LIMIT,
  ORDER_STATUS_LABEL,
  TOAST_DURATION_MS,
} from '../constants'

/**
 * 退款页：检索订单 → 选择退货明细 → 试算金额 → 二次授权 → 提交退款。
 * 金额试算走后端 preview_refund，与实际退款同口径，界面不自行折算优惠。
 */
const props = defineProps<{
  initialOrderNo?: string
  /** 二次授权：由 App.vue 提供（弹窗收集批准人账号密码） */
  requestAuth: (permission: string, title: string) => Promise<string>
}>()
const emit = defineEmits<{ (e: 'detail', orderNo: string): void }>()

const err = ref('')
const okMsg = ref('')
const date = ref(today())
const keyword = ref('')
const orders = ref<OrderSummary[]>([])
const detail = ref<OrderDetail | null>(null)
/** 退货数量：item_id → 数量 */
const quantities = ref<Record<number, number>>({})
const refundAmount = ref(0)
const submitting = ref(false)
const loading = ref(false)

function fail(msg: unknown) {
  err.value = String(msg)
  okMsg.value = ''
  setTimeout(() => (err.value = ''), ERROR_DURATION_MS)
}
function done(msg: string) {
  okMsg.value = msg
  err.value = ''
  setTimeout(() => (okMsg.value = ''), TOAST_DURATION_MS)
}

async function loadOrders() {
  loading.value = true
  try {
    orders.value = await api.listOrders(date.value, keyword.value.trim(), ORDER_LIST_LIMIT)
  } catch (e) {
    fail(e)
  } finally {
    loading.value = false
  }
}

async function selectOrder(orderNo: string) {
  try {
    detail.value = await api.getOrderDetail(orderNo)
    quantities.value = {}
    refundAmount.value = 0
    if (props.initialOrderNo) keyword.value = props.initialOrderNo
  } catch (e) {
    fail(e)
  }
}

/** 可退数量 = 购买数量 − 已退数量 */
function refundableQty(itemId: number): number {
  const it = detail.value?.items.find((x) => x.item_id === itemId)
  if (!it) return 0
  return Math.max(0, it.qty - it.returned_qty)
}

function selectedItems() {
  return Object.entries(quantities.value)
    .filter(([, qty]) => qty > 0)
    .map(([id, qty]) => ({ order_item_id: Number(id), qty }))
}

/** 试算：数量变化即向后台要一次准确金额 */
async function preview() {
  const items = selectedItems()
  if (!items.length || !detail.value) {
    refundAmount.value = 0
    return
  }
  try {
    refundAmount.value = await api.previewRefund(detail.value.header.order_no, items)
  } catch (e) {
    fail(e)
  }
}

// 退货数量联动试算：输入停顿后再请求，避免每次按键都打后端
let previewTimer: ReturnType<typeof setTimeout> | null = null
watch(quantities, () => {
  if (previewTimer) clearTimeout(previewTimer)
  previewTimer = setTimeout(() => void preview(), 250)
}, { deep: true })

async function submitRefund() {
  const items = selectedItems()
  if (!items.length) {
    fail('请至少选择一行退货明细')
    return
  }
  submitting.value = true
  try {
    const nonce = await props.requestAuth('pos.refund', '退款授权')
    const amount = await api.refundOrder(detail.value!.header.order_no, items, nonce)
    done(`退款完成，金额 ¥${formatMoney(amount)}`)
    await selectOrder(detail.value!.header.order_no)
    await loadOrders()
  } catch (e) {
    if (String(e) !== '已取消二次授权') fail(e)
  } finally {
    submitting.value = false
  }
}

onMounted(async () => {
  await loadOrders()
  if (props.initialOrderNo) {
    keyword.value = props.initialOrderNo
    await selectOrder(props.initialOrderNo)
  }
})
</script>

<template>
  <main class="main-simple">
    <div class="page-head">
      <h2>退款</h2>
    </div>

    <p v-if="err" class="error" role="alert">{{ err }}</p>
    <p v-if="okMsg" class="ok-msg">{{ okMsg }}</p>

    <div class="card wide">
      <h3>检索订单</h3>
      <div class="toolbar">
        <input v-model="date" class="input" type="date" aria-label="订单日期" />
        <input v-model="keyword" class="input" placeholder="订单号" aria-label="订单号" @keyup.enter="loadOrders" />
        <button class="btn" :disabled="loading" @click="loadOrders">
          <span v-if="loading" class="btn-spinner" aria-label="查询中"></span>
          <Icon v-else name="search" :size="ICON_SIZE.md" />{{ loading ? '查询中…' : '查询' }}
        </button>
      </div>
      <table class="tbl">
        <thead>
          <tr><th>订单号</th><th>时间</th><th class="num-cell">件数</th><th class="num-cell">实付</th><th class="num-cell">已退</th><th>状态</th><th>操作</th></tr>
        </thead>
        <tbody>
          <tr v-for="o in orders" :key="o.order_no">
            <td class="mono">{{ o.order_no }}</td>
            <td>{{ o.created_at }}</td>
            <td class="num-cell">{{ o.item_count }}</td>
            <td class="num-cell">¥{{ formatMoney(o.pay_cents) }}</td>
            <td class="num-cell">¥{{ formatMoney(o.refunded_cents) }}</td>
            <td><span class="badge" :class="o.status === 'PAID' || o.status === 'PARTIAL_REFUNDED' ? 'badge-ok' : 'badge-off'">{{ ORDER_STATUS_LABEL[o.status] ?? o.status }}</span></td>
            <td>
              <button class="btn btn-sm" @click="selectOrder(o.order_no)">选择</button>
              <button class="btn btn-sm ghost" @click="emit('detail', o.order_no)">详情</button>
            </td>
          </tr>
          <tr v-if="!orders.length"><td colspan="7" class="empty">没有符合条件的订单，试试调整日期或订单号</td></tr>
        </tbody>
      </table>
    </div>

    <div v-if="detail" class="card wide">
      <h3>退货明细 · {{ detail.header.order_no }}</h3>
      <table class="tbl">
        <thead>
          <tr><th>商品</th><th class="num-cell">单价</th><th class="num-cell">数量</th><th class="num-cell">已退</th><th class="num-cell">可退</th><th class="num-cell">退货数量</th></tr>
        </thead>
        <tbody>
          <tr v-for="it in detail.items" :key="it.item_id">
            <td>{{ it.product_name }}</td>
            <td class="num-cell">¥{{ formatMoney(it.unit_price_cents) }}</td>
            <td class="num-cell">{{ it.qty }}</td>
            <td class="num-cell">{{ it.returned_qty }}</td>
            <td class="num-cell">{{ refundableQty(it.item_id) }}</td>
            <td class="num-cell">
              <span v-if="refundableQty(it.item_id) === 0" class="muted small" title="该商品已全部退货">已全退</span>
              <input
                v-else
                v-model.number="quantities[it.item_id]"
                class="input input-qty"
                type="number"
                min="0"
                :max="refundableQty(it.item_id)"
                aria-label="退货数量"
              />
            </td>
          </tr>
          <tr v-if="!detail.items.length"><td colspan="6" class="empty">该订单无明细</td></tr>
        </tbody>
      </table>
      <div class="refund-total">
        <span>应退金额</span>
        <strong>¥{{ formatMoney(refundAmount) }}</strong>
      </div>
      <button class="btn primary" :disabled="submitting || refundAmount <= 0" @click="submitRefund">
        <span v-if="submitting" class="btn-spinner" aria-label="退款处理中"></span>
        <Icon v-else name="check" :size="ICON_SIZE.md" />{{ submitting ? '退款处理中…' : '确认退款（需授权）' }}
      </button>
      <p class="muted small">退款按原单行优惠还原金额；扫码支付订单退至原渠道，需店长授权。</p>
    </div>
  </main>
</template>
