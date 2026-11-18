<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { convertFileSrc } from '@tauri-apps/api/core'
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
  instoreCode: '',
  categoryId: 1,
  unit: '件',
  salePrice: '',
  costPrice: '',
  memberPrice: '',
  isWeighed: false,
  initialQty: 0,
  alertQty: 0,
  imageUrl: '',
})
/** 新增表单选择的图片文件（file input 不能 v-model，单独持有 File） */
const createImageFile = ref<File | null>(null)

// 行内编辑
const editSku = ref<number | null>(null)
const editPrice = ref('')
const editAlert = ref('')
const editMemberPrice = ref('')
const editInstore = ref('')
// 库存调整
const adjustSku = ref<number | null>(null)
const adjustQty = ref('')
const adjustReason = ref('')
// 分类
const showCategory = ref(false)
const newCategoryName = ref('')
const newCategoryParent = ref<number | null>(null)

// 商品图片：上传本地文件或登记在线地址
const imageSku = ref<number | null>(null)
const imageUrlDraft = ref('')

const lowStockCount = computed(
  () => products.value.filter((p) => p.status === 1 && p.qty <= p.alert_qty).length
)

/** 读取文件为 base64 data URL（后端剥前缀后按文件头判定格式） */
function readFileBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader()
    reader.onload = () => resolve(String(reader.result))
    reader.onerror = () => reject(new Error('读取图片失败'))
    reader.readAsDataURL(file)
  })
}

/** 展示地址：本地图片走资产协议，在线图片直接用 */
function imageSrc(p: ProductRow): string | null {
  if (p.image_path) return convertFileSrc(p.image_path)
  if (p.image_url) return p.image_url
  return null
}

function pickCreateImage(e: Event) {
  const input = e.target as HTMLInputElement
  createImageFile.value = input.files?.[0] ?? null
}

function startImage(p: ProductRow) {
  imageSku.value = p.sku_id
  imageUrlDraft.value = p.image_url ?? ''
}

async function uploadImage(e: Event) {
  const input = e.target as HTMLInputElement
  const file = input.files?.[0]
  if (!file || imageSku.value == null) return
  try {
    const base64 = await readFileBase64(file)
    await api.uploadProductImage(imageSku.value, base64)
    done('图片已上传')
    await reload()
  } catch (ex) {
    fail(ex)
  } finally {
    input.value = ''
  }
}

async function saveImageUrl() {
  if (imageSku.value == null) return
  try {
    await api.setProductImageUrl(imageSku.value, imageUrlDraft.value.trim())
    done('图片地址已保存')
    await reload()
  } catch (e) {
    fail(e)
  }
}

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
    const skuId = await api.createProduct({
      name: form.value.name,
      barcode: form.value.barcode.trim() || null,
      instoreCode: form.value.instoreCode.trim() || null,
      categoryId: form.value.categoryId,
      unit: form.value.unit.trim() || '件',
      salePriceCents: sale,
      costPriceCents: cost,
      memberPriceCents: form.value.memberPrice.trim()
        ? Math.round(parseFloat(form.value.memberPrice) * CENTS_PER_YUAN)
        : null,
      isWeighed: form.value.isWeighed,
      initialQty: form.value.initialQty,
      alertQty: form.value.alertQty,
    })
    // 图片：上传的本地文件优先，其次登记在线地址
    if (createImageFile.value) {
      const base64 = await readFileBase64(createImageFile.value)
      await api.uploadProductImage(skuId, base64)
    }
    if (form.value.imageUrl.trim()) {
      await api.setProductImageUrl(skuId, form.value.imageUrl.trim())
    }
    showCreate.value = false
    createImageFile.value = null
    form.value = { name: '', barcode: '', instoreCode: '', categoryId: 1, unit: '件', salePrice: '', costPrice: '', memberPrice: '', isWeighed: false, initialQty: 0, alertQty: 0, imageUrl: '' }
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
  editMemberPrice.value = p.member_price_cents != null
    ? (p.member_price_cents / CENTS_PER_YUAN).toFixed(CURRENCY_DECIMALS)
    : ''
  editInstore.value = p.instore_code ?? ''
}

async function saveEdit() {
  if (editSku.value == null) return
  try {
    await api.updateProduct(
      editSku.value,
      Math.round(parseFloat(editPrice.value || '0') * CENTS_PER_YUAN),
      parseInt(editAlert.value || '0', 10),
      editMemberPrice.value.trim() ? Math.round(parseFloat(editMemberPrice.value) * CENTS_PER_YUAN) : null,
      editInstore.value.trim() || null
    )
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
    await api.createCategory(newCategoryParent.value, name)
    newCategoryName.value = ''
    newCategoryParent.value = null
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
      <div class="form-row">
        <input v-model="newCategoryName" class="input" placeholder="新分类名称" @keyup.enter="addCategory" />
        <select v-model.number="newCategoryParent" class="input">
          <option :value="null">顶级分类</option>
          <option v-for="c in categories" :key="c.category_id" :value="c.category_id">{{ c.name }}</option>
        </select>
        <button class="btn" @click="addCategory"><Icon name="plus" :size="ICON_SIZE.md" />添加分类</button>
      </div>
    </div>

    <div v-if="showCreate" class="card">
      <h3>新增商品</h3>
      <div class="form-grid">
        <label>名称<input v-model="form.name" class="input" placeholder="商品名称" /></label>
        <label>条码<input v-model="form.barcode" class="input" placeholder="条码（称重商品可留空）" /></label>
        <label>店内码（称重必填）<input v-model="form.instoreCode" class="input" placeholder="5 位店内商品代码" /></label>
        <label>分类
          <select v-model="form.categoryId" class="input">
            <option v-for="c in categories" :key="c.category_id" :value="c.category_id">{{ c.name }}</option>
          </select>
        </label>
        <label>单位<input v-model="form.unit" class="input" /></label>
        <label>售价（元）<input v-model="form.salePrice" class="input" placeholder="如 3.50" /></label>
        <label>会员价（元）<input v-model="form.memberPrice" class="input" placeholder="可选" /></label>
        <label>成本（元）<input v-model="form.costPrice" class="input" placeholder="如 2.10" /></label>
        <label>期初库存<input v-model.number="form.initialQty" class="input" type="number" min="0" /></label>
        <label>库存预警值<input v-model.number="form.alertQty" class="input" type="number" min="0" /></label>
        <label>商品图片（本地文件）
          <input class="input" type="file" accept="image/png,image/jpeg,image/webp,image/gif" @change="pickCreateImage" />
        </label>
        <label>商品图片（在线地址）<input v-model="form.imageUrl" class="input" placeholder="https://…（可选）" /></label>
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
            <th>图片</th>
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
            <td>
              <img v-if="imageSrc(p)" :src="imageSrc(p)!" class="thumb" alt="" />
              <span v-else class="muted small">无</span>
            </td>
            <td>{{ p.name }}<span v-if="p.is_weighed" class="muted small">（称重/{{ p.unit }}）</span></td>
            <td class="mono">{{ p.barcode || '—' }}</td>
            <td>{{ p.category_name }}</td>
            <td>
              <span v-if="editSku !== p.sku_id">{{ formatMoney(p.sale_price_cents) }}</span>
              <span v-else class="edit-row">
                <input v-model="editPrice" class="input input-sm" />
                <input v-model="editMemberPrice" class="input input-sm" placeholder="会员价" />
                <input v-model="editInstore" class="input input-sm" placeholder="店内码" />
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
              <button class="btn btn-sm" @click="startImage(p)"><Icon name="box" :size="ICON_SIZE.sm" />图片</button>
              <button class="btn btn-sm ghost" @click="toggleStatus(p)">{{ p.status === 1 ? '停用' : '启用' }}</button>
            </td>
          </tr>
          <tr v-if="products.length === 0">
            <td colspan="8" class="muted">暂无商品，点击右上角"新增商品"创建第一个商品</td>
          </tr>
        </tbody>
      </table>
    </div>

    <div v-if="imageSku != null" class="card">
      <h3>商品图片（SKU {{ imageSku }}）</h3>
      <p class="muted small">两种方式二选一或并存：上传本地文件（存本机），或登记在线图片地址（http/https）。</p>
      <div class="form-grid">
        <label>上传本地图片<input class="input" type="file" accept="image/png,image/jpeg,image/webp,image/gif" @change="uploadImage" /></label>
        <label>在线图片地址<input v-model="imageUrlDraft" class="input" placeholder="https://…" /></label>
      </div>
      <div class="op-form">
        <button class="btn primary" @click="saveImageUrl">保存图片地址</button>
        <button class="btn ghost" @click="imageSku = null">关闭</button>
      </div>
    </div>

    <div v-if="adjustSku != null" class="card">
      <h3>库存调整（SKU {{ adjustSku }}）</h3>
      <p class="muted small">正数为盘盈/入账，负数为盘亏/报损；原因将记入库存台账</p>
      <div class="form-row">
        <input v-model="adjustQty" class="input" type="number" placeholder="变化数量，如 5 或 -3" />
        <input v-model="adjustReason" class="input" placeholder="原因（必填），如 破损报废" />
        <button class="btn primary" @click="saveAdjust">确认调整</button>
        <button class="btn ghost" @click="adjustSku = null">取消</button>
      </div>
    </div>
  </main>
</template>

<style scoped>
/* 组件特有样式；通用类（page-head/toolbar/tbl/form-grid/chip 等）已上移全局 styles.css */
/* 表格操作列：区别于全局 .actions（POS 支付双列网格），此处为行内按钮组 */
.actions { display: flex; gap: var(--space-2); }
.edit-row { display: inline-flex; gap: var(--space-1); align-items: center; }
/* 商品缩略图：固定方形，缺图不撑表格 */
.thumb {
  width: var(--control-thumb-size); height: var(--control-thumb-size);
  object-fit: cover; border-radius: var(--radius-sm);
  border: var(--border-width) solid var(--border);
  background: var(--surface-subtle);
}
</style>
