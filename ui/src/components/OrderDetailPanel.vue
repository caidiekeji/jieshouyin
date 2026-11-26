<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { api, formatMoney, today, type OrderDetail, type OrderSummary } from '../api'
import { useAppStore } from '../stores/app'
import {
  ERROR_DURATION_MS,
  ORDER_LIST_LIMIT,
  ORDER_STATUS_LABEL,
  PAY_CHANNEL_LABEL,
  PAY_STATUS_LABEL,
  TOAST_DURATION_MS,
} from '../constants'

/** 订单面板：orderNo 为空 = 列表模式（日期+关键词检索），非空 = 详情模式；退款与重打小票从面板直接触发。 */
const app = useAppStore()
const props = defineProps<{ orderNo: string }>()
const emit = defineEmits<{
  (e: 'refund', orderNo: string): void
  (e: 'search', orderNo: string): void
  (e: 'back'): void
}>()

const isDetail = computed(() => !!props.orderNo.trim())
const detail = ref<OrderDetail | null>(null)
const err = ref('')
const okMsg = ref('')
const loading = ref(false)
/** 查询框：回车或点「查询」后交给父级切换当前单号 */
const query = ref('')

/** 列表模式状态：日期默认取报表跳转带入日（app.orderListDate），否则当天 */
const listDate = ref(app.orderListDate || today())
const listKeyword = ref('')
const listRows = ref<OrderSummary[]>([])
const listLoading = ref(false)

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

async function loadList() {
  listLoading.value = true
  try {
    listRows.value = await api.listOrders(listDate.value, listKeyword.value.trim(), ORDER_LIST_LIMIT)
  } catch (e) {
    fail(e)
  } finally {
    listLoading.value = false
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
  if (!isDetail.value) void loadList()
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

/** ISO 时间转本地「年-月-日 时:分」，避免原始 ISO 串过长折行 */
function fmtTime(s: string): string {
  const d = new Date(s)
  if (Number.isNaN(d.getTime())) return s
  const p = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`
}
</script>

<template>
  <main class="main-simple">
    <div class="page-head">
      <h2>{{ isDetail ? '订单详情' : '订单列表' }}</h2>
      <div class="tab-row">
        <template v-if="isDetail">
          <button class="btn" @click="emit('back')">返回列表</button>
          <button class="btn" :disabled="loading" @click="load">刷新</button>
          <button v-if="app.printAuto" class="btn" @click="reprint">重打小票</button>
          <button class="btn danger-text" @click="emit('refund', props.orderNo)">发起退款</button>
        </template>
        <button v-else class="btn" :disabled="listLoading" @click="loadList">刷新</button>
      </div>
    </div>

    <p v-if="err" class="error">{{ err }}</p>
    <p v-if="okMsg" class="ok-msg">{{ okMsg }}</p>

    <!-- 列表模式：日期 + 关键词检索，行点击进详情 -->
    <div v-if="!isDetail" class="card wide">
      <div class="toolbar">
        <input v-model="listDate" class="input" type="date" aria-label="订单日期" @change="loadList" />
        <input v-model="listKeyword" class="input" placeholder="订单号" aria-label="订单号" @keyup.enter="loadList" />
        <button class="btn" :disabled="listLoading" @click="loadList">
          <span v-if="listLoading" class="btn-spinner" aria-label="查询中"></span>
          {{ listLoading ? '查询中…' : '查询' }}
        </button>
      </div>
      <table class="tbl">
        <thead>
          <tr><th>订单号</th><th>时间</th><th class="num-cell">件数</th><th class="num-cell">实付</th><th class="num-cell">已退</th><th>状态</th><th>操作</th></tr>
        </thead>
        <tbody>
          <tr v-for="o in listRows" :key="o.order_no" class="order-row" @click="emit('search', o.order_no)">
            <td class="mono">{{ o.order_no }}</td>
            <td>{{ o.created_at }}</td>
            <td class="num-cell">{{ o.item_count }}</td>
            <td class="num-cell">¥{{ formatMoney(o.pay_cents) }}</td>
            <td class="num-cell">¥{{ formatMoney(o.refunded_cents) }}</td>
            <td><span class="badge" :class="o.status === 'COMPLETED' || o.status === 'PARTIAL_REFUND' ? 'badge-ok' : 'badge-off'">{{ ORDER_STATUS_LABEL[o.status] ?? o.status }}</span></td>
            <td><button class="btn btn-sm" @click.stop="emit('search', o.order_no)">详情</button></td>
          </tr>
          <tr v-if="!listRows.length"><td colspan="7" class="empty">没有符合条件的订单，试试调整日期或订单号</td></tr>
        </tbody>
      </table>
      <p v-if="listRows.length === ORDER_LIST_LIMIT" class="muted small">已显示最近 {{ ORDER_LIST_LIMIT }} 条，更早的订单请缩小日期范围</p>
    </div>

    <template v-else>
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
        <div class="field"><span class="field-label">下单时间</span><span class="field-value mono">{{ fmtTime(detail.header.created_at) }}</span></div>
        <div class="field"><span class="field-label">收银员编号</span><span class="field-value mono">{{ detail.header.cashier_id }}</span></div>
        <div class="field"><span class="field-label">会员</span><span class="field-value">{{ detail.header.member_name || '散客' }}</span></div>
        <div v-if="detail.header.table_code" class="field"><span class="field-label">桌台/包间</span><span class="field-value mono">{{ detail.header.table_code }}</span></div>
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
      <p>输入上方订单号，查看商品明细、支付流水与退货记录</p>
    </div>
    </template>
  </main>
</template>
