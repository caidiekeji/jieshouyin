<script setup lang="ts">
import { onMounted, ref, watch } from 'vue'
import { api, formatMoney, type OrderDetail } from '../api'
import Icon from './Icon.vue'
import {
  ERROR_DURATION_MS,
  ICON_SIZE,
  ORDER_STATUS_LABEL,
  PAY_CHANNEL_LABEL,
  PAY_STATUS_LABEL,
  TOAST_DURATION_MS,
} from '../constants'

/** 订单详情面板：单号由父级传入；退款与重打小票从面板直接触发。 */
const props = defineProps<{ orderNo: string }>()
const emit = defineEmits<{
  (e: 'refund', orderNo: string): void
  (e: 'search', orderNo: string): void
}>()

const detail = ref<OrderDetail | null>(null)
const err = ref('')
const okMsg = ref('')
const loading = ref(false)
/** 查询框：回车或点「查询」后交给父级切换当前单号 */
const query = ref('')

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

async function load() {
  if (!props.orderNo.trim()) {
    detail.value = null
    return
  }
  loading.value = true
  try {
    detail.value = await api.getOrderDetail(props.orderNo.trim())
  } catch (e) {
    fail(e)
  } finally {
    loading.value = false
  }
}

async function reprint() {
  try {
    const r = await api.printReceipt(props.orderNo.trim())
    done(r)
  } catch (e) {
    fail(e)
  }
}

watch(() => props.orderNo, load)
onMounted(() => {
  query.value = props.orderNo
  load()
})

/** 按单号查询：单号为空时提示，避免误查全表 */
function search() {
  const no = query.value.trim()
  if (!no) {
    fail('请输入订单号')
    return
  }
  emit('search', no)
}
</script>

<template>
  <main class="main-simple">
    <div class="page-head">
      <h2>订单详情</h2>
      <div class="tab-row">
        <button class="btn" :disabled="loading" @click="load">
          <Icon name="refresh" :size="ICON_SIZE.md" />刷新
        </button>
        <button class="btn" @click="reprint"><Icon name="print" :size="ICON_SIZE.md" />重打小票</button>
        <button class="btn danger-text" @click="emit('refund', props.orderNo)"><Icon name="undo" :size="ICON_SIZE.md" />发起退款</button>
      </div>
    </div>

    <div class="card wide">
      <div class="toolbar">
        <input
          v-model="query"
          class="input"
          placeholder="输入订单号查询"
          aria-label="订单号"
          @keyup.enter="search"
        />
        <button class="btn" @click="search">查询</button>
      </div>
    </div>

    <p v-if="err" class="error">{{ err }}</p>
    <p v-if="okMsg" class="ok-msg">{{ okMsg }}</p>

    <div v-if="detail" class="card wide">
      <h3>
        {{ detail.header.order_no }}
        <span class="badge">{{ ORDER_STATUS_LABEL[detail.header.status] ?? detail.header.status }}</span>
      </h3>
      <div class="stats">
        <div class="stat"><span>订单金额</span><strong>¥{{ formatMoney(detail.header.total_cents) }}</strong></div>
        <div class="stat"><span>优惠金额</span><strong>¥{{ formatMoney(detail.header.discount_cents) }}</strong></div>
        <div class="stat"><span>实付金额</span><strong>¥{{ formatMoney(detail.header.pay_cents) }}</strong></div>
        <div class="stat"><span>已退金额</span><strong>¥{{ formatMoney(detail.header.refunded_cents) }}</strong></div>
      </div>
      <div class="form-grid">
        <div class="field"><span class="field-label">业务日期</span><span class="field-value mono">{{ detail.header.business_date }}</span></div>
        <div class="field"><span class="field-label">下单时间</span><span class="field-value mono">{{ detail.header.created_at }}</span></div>
        <div class="field"><span class="field-label">收银员编号</span><span class="field-value mono">{{ detail.header.cashier_id }}</span></div>
        <div class="field"><span class="field-label">会员</span><span class="field-value">{{ detail.header.member_name || '散客' }}</span></div>
      </div>
    </div>

    <div v-if="detail" class="card wide">
      <h3>商品明细</h3>
      <table class="tbl">
        <thead>
          <tr><th>商品</th><th class="num-cell">单价</th><th class="num-cell">数量</th><th class="num-cell">小计</th><th class="num-cell">优惠</th><th class="num-cell">已退</th></tr>
        </thead>
        <tbody>
          <tr v-for="it in detail.items" :key="it.item_id">
            <td>{{ it.product_name }}<span v-if="it.is_gift" class="badge">赠品</span></td>
            <td class="num-cell">¥{{ formatMoney(it.unit_price_cents) }}</td>
            <td class="num-cell">{{ it.qty }}</td>
            <td class="num-cell">¥{{ formatMoney(it.line_total_cents) }}</td>
            <td class="num-cell">¥{{ formatMoney(it.discount_cents) }}</td>
            <td class="num-cell">{{ it.returned_qty }}</td>
          </tr>
          <tr v-if="!detail.items.length"><td colspan="6" class="empty">该订单无商品明细</td></tr>
        </tbody>
      </table>
    </div>

    <div v-if="detail?.payments.length" class="card wide">
      <h3>支付流水</h3>
      <table class="tbl">
        <thead>
          <tr><th>渠道</th><th>方式</th><th class="num-cell">金额</th><th class="num-cell">实收</th><th class="num-cell">找零</th><th>渠道单号</th><th>状态</th></tr>
        </thead>
        <tbody>
          <tr v-for="p in detail.payments" :key="p.pay_id">
            <td>{{ PAY_CHANNEL_LABEL[p.pay_channel] ?? p.pay_channel }}</td>
            <td>{{ p.pay_mode || '—' }}</td>
            <td class="num-cell">¥{{ formatMoney(p.amount_cents) }}</td>
            <td class="num-cell">{{ p.cash_tendered_cents == null ? '—' : formatMoney(p.cash_tendered_cents) }}</td>
            <td class="num-cell">{{ p.change_cents == null ? '—' : formatMoney(p.change_cents) }}</td>
            <td class="mono">{{ p.trade_no || p.out_trade_no || '—' }}</td>
            <td><span class="badge" :class="p.status === 'PAID' || p.status === 'COMPLETED' ? 'badge-ok' : 'badge-off'">{{ PAY_STATUS_LABEL[p.status] ?? p.status }}</span></td>
          </tr>
        </tbody>
      </table>
    </div>

    <div v-if="detail?.promos.length" class="card wide">
      <h3>优惠明细</h3>
      <table class="tbl">
        <thead><tr><th>规则编号</th><th class="num-cell">优惠金额</th><th>说明</th></tr></thead>
        <tbody>
          <tr v-for="(p, i) in detail.promos" :key="i">
            <td class="mono">#{{ p.promotion_id }}</td>
            <td class="num-cell">¥{{ formatMoney(p.discount_cents) }}</td>
            <td class="muted">{{ p.detail || '—' }}</td>
          </tr>
        </tbody>
      </table>
    </div>

    <div v-if="detail?.refunds.length" class="card wide">
      <h3>退货记录</h3>
      <table class="tbl">
        <thead><tr><th>退货单</th><th>退款单号</th><th class="num-cell">金额</th><th>状态</th><th>时间</th></tr></thead>
        <tbody>
          <tr v-for="r in detail.refunds" :key="r.refund_id">
            <td>{{ r.return_no || '—' }}</td>
            <td class="mono">{{ r.refund_no }}</td>
            <td class="num-cell">¥{{ formatMoney(r.amount_cents) }}</td>
            <td><span class="badge" :class="r.status === 'PAID' || r.status === 'COMPLETED' ? 'badge-ok' : 'badge-off'">{{ PAY_STATUS_LABEL[r.status] ?? r.status }}</span></td>
            <td>{{ r.created_at }}</td>
          </tr>
        </tbody>
      </table>
    </div>

    <div v-if="!detail && !loading" class="empty-state-card">
      <Icon name="search" :size="ICON_SIZE.lg" />
      <p>输入上方订单号，查看商品明细、支付流水与退货记录</p>
    </div>
  </main>
</template>
