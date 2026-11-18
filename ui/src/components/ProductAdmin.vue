<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { api, formatMoney, type ProductRow, type Category } from '../api'
import Icon from './Icon.vue'
import {
  CENTS_PER_YUAN,
  CURRENCY_DECIMALS,
  ERROR_DURATION_MS,
  ICON_SIZE,
  PAGE_SIZE,
  TOAST_DURATION_MS,
} from '../constants'

const products = ref<ProductRow[]>([])
const categories = ref<Category[]>([])
const keyword = ref('')
const loading = ref(false)
const err = ref('')
const okMsg = ref('')

// 新增商品表单
const showCreate = ref(false)
const form = ref({
  name: '',
  barcode: '',
  categoryId: 1,
  unit: '件',
  salePrice: '',
  costPrice: '',
  isWeighed: false,
  initialQty: 0,
  alertQty: 0,
})

// 行内编辑
const editSku = ref<number | null>(null)
const editPrice = ref('')
const editAlert = ref('')
// 库存调整
const adjustSku = ref<number | null>(null)
const adjustQty = ref('')
const adjustReason = ref('')
// 分类
const showCategory = ref(false)
const newCategoryName = ref('')

const lowStockCount = computed(
  () => products.value.filter((p) => p.status === 1 && p.qty <= p.alert_qty).length
)

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

async function reload() {
  loading.value = true
  try {
    products.value = await api.listProducts(keyword.value.trim(), PAGE_SIZE.PRODUCTS)
    categories.value = await api.listCategories()
  } catch (e) {
    fail(e)
  } finally {
    loading.value = false
  }
}

async function createProduct() {
  const sale = Math.round(parseFloat(form.value.salePrice || '0') * CENTS_PER_YUAN)
  const cost = Math.round(parseFloat(form.value.costPrice || '0') * CENTS_PER_YUAN)
  try {
    await api.createProduct({
      name: form.value.name,
      barcode: form.value.barcode.trim() || null,
      categoryId: form.value.categoryId,
      unit: form.value.unit.trim() || '件',
      salePriceCents: sale,
      costPriceCents: cost,
      isWeighed: form.value.isWeighed,
      initialQty: form.value.initialQty,
      alertQty: form.value.alertQty,
    })
    showCreate.value = false
    form.value = { name: '', barcode: '', categoryId: 1, unit: '件', salePrice: '', costPrice: '', isWeighed: false, initialQty: 0, alertQty: 0 }
    done('商品已创建')
    await reload()
  } catch (e) {
    fail(e)
  }
}

function startEdit(p: ProductRow) {
  editSku.value = p.sku_id
  editPrice.value = (p.sale_price_cents / CENTS_PER_YUAN).toFixed(CURRENCY_DECIMALS)
  editAlert.value = String(p.alert_qty)
}

async function saveEdit() {
  if (editSku.value == null) return
  try {
    await api.updateProduct(editSku.value, Math.round(parseFloat(editPrice.value || '0') * CENTS_PER_YUAN), parseInt(editAlert.value || '0', 10))
    editSku.value = null
    done('已保存')
    await reload()
  } catch (e) {
    fail(e)
  }
}

async function toggleStatus(p: ProductRow) {
  try {
    await api.setProductStatus(p.spu_id, p.status === 1 ? 0 : 1)
    done(p.status === 1 ? '已停用' : '已启用')
    await reload()
  } catch (e) {
    fail(e)
  }
}

function startAdjust(p: ProductRow) {
  adjustSku.value = p.sku_id
  adjustQty.value = ''
  adjustReason.value = ''
}

async function saveAdjust() {
  if (adjustSku.value == null) return
  const qty = parseInt(adjustQty.value || '0', 10)
  try {
    await api.adjustStock(adjustSku.value, qty, adjustReason.value.trim())
    adjustSku.value = null
    done('库存已调整')
    await reload()
  } catch (e) {
    fail(e)
  }
}

async function addCategory() {
  const name = newCategoryName.value.trim()
  if (!name) return
  try {
    await api.createCategory(null, name)
    newCategoryName.value = ''
    done('分类已创建')
    await reload()
  } catch (e) {
    fail(e)
  }
}

onMounted(reload)
</script>

<template>
  <main class="main-simple">
    <div class="page-head">
      <h2>商品管理</h2>
      <p v-if="lowStockCount > 0" class="muted small warning-text">
        <Icon name="alert" :size="ICON_SIZE.sm" /> {{ lowStockCount }} 个商品库存低于预警值
      </p>
    </div>

    <div class="toolbar">
      <input v-model="keyword" class="input" placeholder="搜索商品名 / 条码" @keyup.enter="reload" />
      <button class="btn" @click="reload"><Icon name="refresh" :size="ICON_SIZE.md" />查询</button>
      <button class="btn" @click="showCategory = !showCategory"><Icon name="box" :size="ICON_SIZE.md" />分类</button>
      <button class="btn primary" @click="showCreate = !showCreate"><Icon name="plus" :size="ICON_SIZE.md" />新增商品</button>
    </div>

    <p v-if="err" class="error">{{ err }}</p>
    <p v-if="okMsg" class="ok-msg">{{ okMsg }}</p>

    <div v-if="showCategory" class="card">
      <h3>商品分类</h3>
      <div class="chip-row">
        <span v-for="c in categories" :key="c.category_id" class="chip">{{ c.name }}</span>
      </div>
      <div class="inline-form">
        <input v-model="newCategoryName" class="input" placeholder="新分类名称" @keyup.enter="addCategory" />
        <button class="btn" @click="addCategory"><Icon name="plus" :size="ICON_SIZE.md" />添加分类</button>
      </div>
    </div>

    <div v-if="showCreate" class="card">
      <h3>新增商品</h3>
      <div class="form-grid">
        <label>名称<input v-model="form.name" class="input" placeholder="商品名称" /></label>
        <label>条码<input v-model="form.barcode" class="input" placeholder="条码（称重商品可留空）" /></label>
        <label>分类
          <select v-model="form.categoryId" class="input">
            <option v-for="c in categories" :key="c.category_id" :value="c.category_id">{{ c.name }}</option>
          </select>
        </label>
        <label>单位<input v-model="form.unit" class="input" /></label>
        <label>售价（元）<input v-model="form.salePrice" class="input" placeholder="如 3.50" /></label>
        <label>成本（元）<input v-model="form.costPrice" class="input" placeholder="如 2.10" /></label>
        <label>期初库存<input v-model.number="form.initialQty" class="input" type="number" min="0" /></label>
        <label>库存预警值<input v-model.number="form.alertQty" class="input" type="number" min="0" /></label>
        <label class="checkbox-label">
          <input v-model="form.isWeighed" type="checkbox" /> 称重商品（按克计价）
        </label>
      </div>
      <button class="btn primary" @click="createProduct">创建商品</button>
    </div>

    <div class="card">
      <table class="tbl">
        <thead>
          <tr>
            <th>名称</th>
            <th>条码</th>
            <th>分类</th>
            <th>售价</th>
            <th>库存</th>
            <th>状态</th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="p in products" :key="p.sku_id" :class="{ 'row-disabled': p.status === 0 }">
            <td>{{ p.name }}<span v-if="p.is_weighed" class="muted small">（称重/{{ p.unit }}）</span></td>
            <td class="mono">{{ p.barcode || '—' }}</td>
            <td>{{ p.category_name }}</td>
            <td>
              <span v-if="editSku !== p.sku_id">{{ formatMoney(p.sale_price_cents) }}</span>
              <span v-else class="edit-row">
                <input v-model="editPrice" class="input input-sm" />
                <input v-model="editAlert" class="input input-sm" placeholder="预警" />
                <button class="btn btn-sm" @click="saveEdit"><Icon name="check" :size="ICON_SIZE.sm" /></button>
              </span>
            </td>
            <td :class="{ 'warning-text': p.qty <= p.alert_qty }">
              {{ p.qty }} {{ p.unit }}<span v-if="p.qty <= p.alert_qty" class="muted small">（低于预警 {{ p.alert_qty }}）</span>
            </td>
            <td>{{ p.status === 1 ? '在售' : '停用' }}</td>
            <td class="actions">
              <button class="btn btn-sm" @click="startEdit(p)"><Icon name="edit" :size="ICON_SIZE.sm" />改价</button>
              <button class="btn btn-sm" @click="startAdjust(p)"><Icon name="box" :size="ICON_SIZE.sm" />库存</button>
              <button class="btn btn-sm ghost" @click="toggleStatus(p)">{{ p.status === 1 ? '停用' : '启用' }}</button>
            </td>
          </tr>
          <tr v-if="products.length === 0">
            <td colspan="7" class="muted">暂无商品，点击右上角"新增商品"创建第一个商品</td>
          </tr>
        </tbody>
      </table>
    </div>

    <div v-if="adjustSku != null" class="card">
      <h3>库存调整（SKU {{ adjustSku }}）</h3>
      <p class="muted small">正数为盘盈/入账，负数为盘亏/报损；原因将记入库存台账</p>
      <div class="inline-form">
        <input v-model="adjustQty" class="input" type="number" placeholder="变化数量，如 5 或 -3" />
        <input v-model="adjustReason" class="input" placeholder="原因（必填），如 破损报废" />
        <button class="btn primary" @click="saveAdjust">确认调整</button>
        <button class="btn ghost" @click="adjustSku = null">取消</button>
      </div>
    </div>
  </main>
</template>

<style scoped>
/* 组件层：只引用语义层 token，不出现裸值 */
.page-head { display: flex; align-items: baseline; gap: var(--space-3); margin-bottom: var(--space-3); }
.toolbar { display: flex; gap: var(--space-2); margin-bottom: var(--space-3); }
.toolbar .input { flex: 1; }
.tbl { width: 100%; border-collapse: collapse; font-size: var(--font-sm); }
.tbl th, .tbl td {
  padding: var(--space-2) var(--space-3);
  text-align: left;
  border-bottom: var(--border-width) solid var(--border);
}
.tbl th { color: var(--muted); font-weight: 600; font-size: var(--font-xs); }
.row-disabled { opacity: var(--opacity-disabled); }
.warning-text { color: var(--danger); }
.actions { display: flex; gap: var(--space-2); }
.edit-row { display: inline-flex; gap: var(--space-1); align-items: center; }
.btn-sm { padding: var(--space-0_5) var(--space-2); font-size: var(--font-xs); }
.input-sm {
  width: var(--control-input-sm-width);
  padding: var(--space-0_5) var(--space-2);
  font-size: var(--font-xs);
}
.form-grid {
  display: grid; grid-template-columns: repeat(4, 1fr);
  gap: var(--space-2); margin-bottom: var(--space-2);
}
.form-grid label {
  display: flex; flex-direction: column; gap: var(--space-1);
  font-size: var(--font-xs); color: var(--muted);
}
.checkbox-label { flex-direction: row !important; align-items: center; gap: var(--space-2) !important; }
.chip-row { display: flex; flex-wrap: wrap; gap: var(--space-2); margin-bottom: var(--space-2); }
.chip {
  background: var(--border); border-radius: var(--radius-pill);
  padding: var(--space-1) var(--space-3); font-size: var(--font-xs);
}
.ok-msg { color: var(--success-text); font-size: var(--font-sm); margin-bottom: var(--space-2); }
.inline-form { display: flex; gap: var(--space-2); align-items: center; }
.card { margin-bottom: var(--space-3); }
.mono { font-family: var(--font-family-mono); }
</style>
