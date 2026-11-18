<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import {
  api,
  formatMoney,
  type ProductRow,
  type SupplierRow,
  type PurchaseOrderRow,
  type PurchaseItemRow,
  type StocktakeRow,
  type StocktakeItemRow
} from '../api'
import Icon from './Icon.vue'
import {
  CENTS_PER_YUAN,
  CURRENCY_DECIMALS,
  ERROR_DURATION_MS,
  ICON_SIZE,
  PAGE_SIZE,
  TOAST_DURATION_MS,
} from '../constants'

const tab = ref<'purchase' | 'stocktake'>('purchase')
const err = ref('')
const okMsg = ref('')

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

// ===== 供应商 =====
const suppliers = ref<SupplierRow[]>([])
const showSupplier = ref(false)
const supForm = ref({ name: '', contact: '', phone: '' })

async function reloadSuppliers() {
  try {
    suppliers.value = await api.listSuppliers()
  } catch (e) {
    fail(e)
  }
}

async function addSupplier() {
  try {
    await api.createSupplier(
      supForm.value.name.trim(),
      supForm.value.contact.trim() || null,
      supForm.value.phone.trim() || null
    )
    supForm.value = { name: '', contact: '', phone: '' }
    done('供应商已创建')
    await reloadSuppliers()
  } catch (e) {
    fail(e)
  }
}

// ===== 采购单 =====
const pos = ref<PurchaseOrderRow[]>([])
const showCreatePo = ref(false)
const poSupplierId = ref<number | null>(null)
const poLines = ref<{ sku_id: number; name: string; qty: number; cost: string }[]>([])
const poSearch = ref('')
const poCandidates = ref<ProductRow[]>([])

async function reloadPos() {
  try {
    pos.value = await api.listPurchaseOrders()
  } catch (e) {
    fail(e)
  }
}

async function searchForPo() {
  try {
    poCandidates.value = await api.listProducts(poSearch.value.trim(), PAGE_SIZE.LOOKUP)
  } catch (e) {
    fail(e)
  }
}

function addPoLine(p: ProductRow) {
  if (poLines.value.some((l) => l.sku_id === p.sku_id)) return
  poLines.value.push({ sku_id: p.sku_id, name: p.name, qty: 1, cost: (p.cost_price_cents / CENTS_PER_YUAN).toFixed(CURRENCY_DECIMALS) })
  poCandidates.value = []
  poSearch.value = ''
}

function removePoLine(i: number) {
  poLines.value.splice(i, 1)
}

const poTotalCents = computed(() =>
  poLines.value.reduce((s, l) => s + l.qty * Math.round(parseFloat(l.cost || '0') * CENTS_PER_YUAN), 0)
)

async function submitPo() {
  if (poSupplierId.value == null) return fail('请选择供应商')
  if (!poLines.value.length) return fail('请添加采购明细')
  try {
    const poNo = await api.createPurchaseOrder(
      poSupplierId.value,
      poLines.value.map((l) => ({
        sku_id: l.sku_id,
        ordered_qty: l.qty,
        unit_cost_cents: Math.round(parseFloat(l.cost || '0') * CENTS_PER_YUAN)
      }))
    )
    showCreatePo.value = false
    poLines.value = []
    done(`采购单 ${poNo} 已创建`)
    await reloadPos()
  } catch (e) {
    fail(e)
  }
}

// 验收
const receivePo = ref<string | null>(null)
const receiveRows = ref<PurchaseItemRow[]>([])
const receiveQty = ref<Record<number, string>>({})

async function openReceive(poNo: string) {
  try {
    receiveRows.value = await api.getPurchaseItems(poNo)
    receiveQty.value = {}
    for (const r of receiveRows.value) {
      receiveQty.value[r.item_id] = String(r.received_qty ?? r.ordered_qty)
    }
    receivePo.value = poNo
  } catch (e) {
    fail(e)
  }
}

async function submitReceive() {
  if (receivePo.value == null) return
  try {
    await api.receivePurchase(
      receivePo.value,
      receiveRows.value.map((r) => ({
        item_id: r.item_id,
        received_qty: parseInt(receiveQty.value[r.item_id] || '0', 10)
      }))
    )
    receivePo.value = null
    done('验收完成，库存已入账')
    await reloadPos()
  } catch (e) {
    fail(e)
  }
}

// ===== 盘点 =====
const stocktakes = ref<StocktakeRow[]>([])
const showCreateSt = ref(false)
const stSearch = ref('')
const stCandidates = ref<ProductRow[]>([])
const stLines = ref<{ sku_id: number; name: string }[]>([])

async function reloadStocktakes() {
  try {
    stocktakes.value = await api.listStocktakes()
  } catch (e) {
    fail(e)
  }
}

async function searchForSt() {
  try {
    stCandidates.value = await api.listProducts(stSearch.value.trim(), PAGE_SIZE.LOOKUP)
  } catch (e) {
    fail(e)
  }
}

function addStLine(p: ProductRow) {
  if (stLines.value.some((l) => l.sku_id === p.sku_id)) return
  stLines.value.push({ sku_id: p.sku_id, name: p.name })
  stCandidates.value = []
  stSearch.value = ''
}

function removeStLine(i: number) {
  stLines.value.splice(i, 1)
}

async function submitSt() {
  if (!stLines.value.length) return fail('请添加盘点商品')
  try {
    const taskNo = await api.createStocktake(stLines.value.map((l) => l.sku_id))
    showCreateSt.value = false
    stLines.value = []
    done(`盘点任务 ${taskNo} 已创建`)
    await reloadStocktakes()
  } catch (e) {
    fail(e)
  }
}

// 录数 / 审批
const countTask = ref<string | null>(null)
const countRows = ref<StocktakeItemRow[]>([])
const countQty = ref<Record<number, string>>({})
const approveTask = ref<string | null>(null)

async function openCount(taskNo: string) {
  try {
    countRows.value = await api.getStocktakeItems(taskNo)
    countQty.value = {}
    countTask.value = taskNo
  } catch (e) {
    fail(e)
  }
}

async function submitCount() {
  if (countTask.value == null) return
  try {
    await api.recordCount(
      countTask.value,
      countRows.value
        .filter((r) => countQty.value[r.item_id] !== undefined && countQty.value[r.item_id] !== '')
        .map((r) => ({ item_id: r.item_id, received_qty: parseInt(countQty.value[r.item_id], 10) }))
    )
    countTask.value = null
    done('实盘已录入，待审批')
    await reloadStocktakes()
  } catch (e) {
    fail(e)
  }
}

async function openApprove(taskNo: string) {
  try {
    approveTask.value = taskNo
    countRows.value = await api.getStocktakeItems(taskNo)
  } catch (e) {
    fail(e)
  }
}

async function submitApprove() {
  if (approveTask.value == null) return
  try {
    await api.approveStocktake(approveTask.value)
    approveTask.value = null
    done('盘点已审批，库存差异已调整')
    await reloadStocktakes()
  } catch (e) {
    fail(e)
  }
}

onMounted(() => {
  reloadSuppliers()
  reloadPos()
  reloadStocktakes()
})
</script>

<template>
  <main class="main-simple">
    <div class="page-head">
      <h2>采购与盘点</h2>
      <div class="tab-row">
        <button :class="['btn', { primary: tab === 'purchase' }]" @click="tab = 'purchase'">采购入库</button>
        <button :class="['btn', { primary: tab === 'stocktake' }]" @click="tab = 'stocktake'">库存盘点</button>
      </div>
    </div>

    <p v-if="err" class="error">{{ err }}</p>
    <p v-if="okMsg" class="ok-msg">{{ okMsg }}</p>

    <!-- 采购入库 -->
    <template v-if="tab === 'purchase'">
      <div class="toolbar">
        <button class="btn" @click="showSupplier = !showSupplier"><Icon name="users" :size="ICON_SIZE.md" />供应商</button>
        <button class="btn primary" @click="showCreatePo = !showCreatePo"><Icon name="plus" :size="ICON_SIZE.md" />新建采购单</button>
      </div>

      <div v-if="showSupplier" class="card">
        <h3>供应商</h3>
        <div class="chip-row">
          <span v-for="s in suppliers" :key="s.supplier_id" class="chip">
            {{ s.name }}<span v-if="s.contact" class="muted small">（{{ s.contact }}）</span>
          </span>
        </div>
        <div class="form-row">
          <input v-model="supForm.name" class="input" placeholder="供应商名称" />
          <input v-model="supForm.contact" class="input" placeholder="联系人" />
          <input v-model="supForm.phone" class="input" placeholder="电话" />
          <button class="btn" @click="addSupplier">添加</button>
        </div>
      </div>

      <div v-if="showCreatePo" class="card">
        <h3>新建采购单</h3>
        <div class="form-row mb-2">
          <select v-model="poSupplierId" class="input">
            <option :value="null" disabled>选择供应商</option>
            <option v-for="s in suppliers" :key="s.supplier_id" :value="s.supplier_id">{{ s.name }}</option>
          </select>
        </div>
        <div class="form-row mb-2">
          <input v-model="poSearch" class="input" placeholder="搜索商品加入采购单" @keyup.enter="searchForPo" />
          <button class="btn" @click="searchForPo"><Icon name="scan" :size="ICON_SIZE.md" />搜索</button>
        </div>
        <div v-for="p in poCandidates" :key="p.sku_id" class="candidate" @click="addPoLine(p)">
          {{ p.name }}（库存 {{ p.qty }}）
        </div>
        <table class="tbl" v-if="poLines.length">
          <thead>
            <tr><th>商品</th><th>数量</th><th>单价（元）</th><th></th></tr>
          </thead>
          <tbody>
            <tr v-for="(l, i) in poLines" :key="l.sku_id">
              <td>{{ l.name }}</td>
              <td><input v-model.number="l.qty" class="input input-sm" type="number" min="1" /></td>
              <td><input v-model="l.cost" class="input input-sm" /></td>
              <td><button class="btn btn-sm ghost" @click="removePoLine(i)"><Icon name="close" :size="ICON_SIZE.sm" /></button></td>
            </tr>
          </tbody>
        </table>
        <p class="muted small">采购金额合计：{{ formatMoney(poTotalCents) }}</p>
        <button class="btn primary" @click="submitPo">创建采购单</button>
      </div>

      <div class="card">
        <h3>采购单</h3>
        <table class="tbl">
          <thead>
            <tr><th>单号</th><th>供应商</th><th>明细</th><th>金额</th><th>状态</th><th>操作</th></tr>
          </thead>
          <tbody>
            <tr v-for="o in pos" :key="o.po_no">
              <td class="mono">{{ o.po_no }}</td>
              <td>{{ o.supplier_name }}</td>
              <td>{{ o.item_count }} 行</td>
              <td>{{ formatMoney(o.total_cents) }}</td>
              <td>{{ o.status === 'RECEIVED' ? '已验收' : '待验收' }}</td>
              <td>
                <button v-if="o.status === 'DRAFT'" class="btn btn-sm" @click="openReceive(o.po_no)">验收</button>
              </td>
            </tr>
            <tr v-if="!pos.length"><td colspan="6" class="muted">暂无采购单</td></tr>
          </tbody>
        </table>
      </div>

      <div v-if="receivePo" class="card">
        <h3>验收：{{ receivePo }}</h3>
        <p class="muted small">逐行录入实收数量，与订购数的差异将留痕记录；验收通过后库存入账</p>
        <table class="tbl">
          <thead><tr><th>商品</th><th>订购</th><th>实收</th></tr></thead>
          <tbody>
            <tr v-for="r in receiveRows" :key="r.item_id">
              <td>{{ r.product_name }}</td>
              <td>{{ r.ordered_qty }}</td>
              <td><input v-model="receiveQty[r.item_id]" class="input input-sm" type="number" min="0" /></td>
            </tr>
          </tbody>
        </table>
        <div class="form-row">
          <button class="btn primary" @click="submitReceive">确认验收</button>
          <button class="btn ghost" @click="receivePo = null">取消</button>
        </div>
      </div>
    </template>

    <!-- 库存盘点 -->
    <template v-else>
      <div class="toolbar">
        <button class="btn primary" @click="showCreateSt = !showCreateSt"><Icon name="plus" :size="ICON_SIZE.md" />新建盘点任务</button>
      </div>

      <div v-if="showCreateSt" class="card">
        <h3>新建盘点任务</h3>
        <div class="form-row mb-2">
          <input v-model="stSearch" class="input" placeholder="搜索商品加入盘点" @keyup.enter="searchForSt" />
          <button class="btn" @click="searchForSt"><Icon name="scan" :size="ICON_SIZE.md" />搜索</button>
        </div>
        <div v-for="p in stCandidates" :key="p.sku_id" class="candidate" @click="addStLine(p)">
          {{ p.name }}（账面 {{ p.qty }}）
        </div>
        <div class="chip-row" v-if="stLines.length">
          <span v-for="(l, i) in stLines" :key="l.sku_id" class="chip">
            {{ l.name }}
            <button class="link" @click="removeStLine(i)"><Icon name="close" :size="ICON_SIZE.xs" /></button>
          </span>
        </div>
        <button class="btn primary" @click="submitSt">创建任务</button>
      </div>

      <div class="card">
        <h3>盘点任务</h3>
        <table class="tbl">
          <thead>
            <tr><th>任务号</th><th>商品数</th><th>状态</th><th>操作</th></tr>
          </thead>
          <tbody>
            <tr v-for="t in stocktakes" :key="t.task_no">
              <td class="mono">{{ t.task_no }}</td>
              <td>{{ t.item_count }}</td>
              <td>{{ t.status === 'DRAFT' ? '待录数' : t.status === 'DIFF_REVIEW' ? '待审批' : '已调整' }}</td>
              <td>
                <button v-if="t.status === 'DRAFT'" class="btn btn-sm" @click="openCount(t.task_no)">录实盘</button>
                <button v-if="t.status === 'DIFF_REVIEW'" class="btn btn-sm" @click="openApprove(t.task_no)">审批调账</button>
              </td>
            </tr>
            <tr v-if="!stocktakes.length"><td colspan="4" class="muted">暂无盘点任务</td></tr>
          </tbody>
        </table>
      </div>

      <div v-if="countTask" class="card">
        <h3>录实盘：{{ countTask }}</h3>
        <table class="tbl">
          <thead><tr><th>商品</th><th>账面</th><th>实盘</th></tr></thead>
          <tbody>
            <tr v-for="r in countRows" :key="r.item_id">
              <td>{{ r.product_name }}</td>
              <td>{{ r.book_qty }}</td>
              <td><input v-model="countQty[r.item_id]" class="input input-sm" type="number" placeholder="实盘数" /></td>
            </tr>
          </tbody>
        </table>
        <div class="form-row">
          <button class="btn primary" @click="submitCount">提交实盘</button>
          <button class="btn ghost" @click="countTask = null">取消</button>
        </div>
      </div>

      <div v-if="approveTask" class="card">
        <h3>审批调账：{{ approveTask }}</h3>
        <table class="tbl">
          <thead><tr><th>商品</th><th>账面</th><th>实盘</th><th>差异</th></tr></thead>
          <tbody>
            <tr v-for="r in countRows" :key="r.item_id">
              <td>{{ r.product_name }}</td>
              <td>{{ r.book_qty }}</td>
              <td>{{ r.counted_qty ?? '—' }}</td>
              <td :class="{ 'warning-text': (r.diff_qty ?? 0) !== 0 }">{{ r.diff_qty ?? 0 }}</td>
            </tr>
          </tbody>
        </table>
        <div class="form-row">
          <button class="btn primary" @click="submitApprove">确认审批</button>
          <button class="btn ghost" @click="approveTask = null">取消</button>
        </div>
      </div>
    </template>
  </main>
</template>
