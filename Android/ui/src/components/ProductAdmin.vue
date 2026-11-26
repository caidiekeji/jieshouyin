<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { convertFileSrc } from '@tauri-apps/api/core'
import { api, formatMoney, type ProductRow, type Category } from '../api'
import Icon from './Icon.vue'
import Checkbox from './Checkbox.vue'
import {
  CENTS_PER_YUAN,
  CURRENCY_DECIMALS,
  ERROR_DURATION_MS,
  ICON_SIZE,
  PAGE_SIZE,
  PRODUCT_NAME_MAX,
  TOAST_DURATION_MS,
} from '../constants'
import { useAppContext } from '../composables/useAppContext'
import IndustryAttributesPanel from './IndustryAttributesPanel.vue'

const ctx = useAppContext()
const { industry } = ctx

/** 计时行业（棋牌/养生）：商品是按时间计费的服务项目，表单区别于零售 */
const timedIndustry = computed(() => industry.value === 'BOARD_GAME' || industry.value === 'WELLNESS')
/** 计时行业的计价单位选项（存入商品单位字段，POS 展示与房费计费共用） */
const TIMED_UNITS = ['小时', '分钟', '次']

const products = ref<ProductRow[]>([])
const categories = ref<Category[]>([])
const keyword = ref('')
const loading = ref(false)
const err = ref('')
const okMsg = ref('')

// 商品表单：新增与编辑共用同一张完整表单，editSku 为 null 表示新增
const showCreate = ref(false)
const editSku = ref<number | null>(null)
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
  isService: false,
  initialQty: 0,
  alertQty: 0,
  imageUrl: '',
})
/** 表单选择的图片文件（file input 不能 v-model，单独持有 File） */
const createImageFile = ref<File | null>(null)

// 库存调整
const adjustSku = ref<number | null>(null)
const adjustQty = ref('')
const adjustReason = ref('')
// 商品图片：上传本地文件或登记在线地址
const imageSku = ref<number | null>(null)
const imageUrlDraft = ref('')

const lowStockCount = computed(
  () => products.value.filter((p) => p.status === 1 && !p.is_service && p.qty <= p.alert_qty).length
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
    products.value = await api.listProducts(keyword.value.trim(), PAGE_SIZE.PRODUCTS, true)
    categories.value = await api.listCategories()
  } catch (e) {
    fail(e)
  } finally {
    loading.value = false
  }
}

/** 金额文本 → 分：空/非数字/负数直接报错，避免 parseFloat 得到 NaN 后静默存 0。 */
function yuanToCents(raw: string, label: string): number {
  const t = raw.trim()
  const n = Number(t)
  if (!t || !Number.isFinite(n) || n < 0) {
    throw new Error(`${label}必须是不小于 0 的数字`)
  }
  return Math.round(n * CENTS_PER_YUAN)
}

function intOrZero(raw: string, label: string): number {
  const n = Number(raw.trim() || '0')
  if (!Number.isInteger(n) || n < 0) throw new Error(`${label}必须是不小于 0 的整数`)
  return n
}

/** 服务项目不计库存：切换时清掉称重与库存字段，避免提交互斥组合。 */
function onServiceToggle() {
  if (form.value.isService) {
    form.value.isWeighed = false
    form.value.initialQty = 0
    form.value.alertQty = 0
  }
}

const EMPTY_FORM = () => ({
  name: '', barcode: '', instoreCode: '', categoryId: 1, unit: '件',
  salePrice: '', costPrice: '', memberPrice: '', isWeighed: false,
  isService: false, initialQty: 0, alertQty: 0, imageUrl: '',
})

/** 打开新增表单：计时行业默认为按时间计费的服务项目 */
function openCreate() {
  editSku.value = null
  form.value = EMPTY_FORM()
  if (timedIndustry.value) {
    form.value.isService = true
    form.value.unit = '小时'
  }
  createImageFile.value = null
  showCreate.value = true
}

/** 打开编辑表单：用商品当前数据回填与新增相同的完整字段 */
function startEdit(p: ProductRow) {
  editSku.value = p.sku_id
  form.value = {
    name: p.name,
    barcode: p.barcode ?? '',
    instoreCode: p.instore_code ?? '',
    categoryId: p.category_id,
    unit: p.unit,
    salePrice: (p.sale_price_cents / CENTS_PER_YUAN).toFixed(CURRENCY_DECIMALS),
    costPrice: (p.cost_price_cents / CENTS_PER_YUAN).toFixed(CURRENCY_DECIMALS),
    memberPrice: p.member_price_cents != null
      ? (p.member_price_cents / CENTS_PER_YUAN).toFixed(CURRENCY_DECIMALS)
      : '',
    isWeighed: p.is_weighed,
    isService: p.is_service,
    // 期初库存是新建台账概念，编辑时仅回填展示、不允许修改（库存请用「库存」按钮调整）
    initialQty: p.qty,
    alertQty: p.alert_qty,
    imageUrl: p.image_url ?? '',
  }
  createImageFile.value = null
  showCreate.value = true
}

/** 当前正在编辑的商品（表单标题用） */
const editingProduct = computed(() => products.value.find((p) => p.sku_id === editSku.value) ?? null)

async function submitProduct() {
  try {
    if (!form.value.name.trim()) throw new Error('商品名不能为空')
    if (form.value.name.trim().length > PRODUCT_NAME_MAX) {
      throw new Error(`商品名不能超过 ${PRODUCT_NAME_MAX} 个字符`)
    }
    const timed = timedIndustry.value
    const payload = {
      name: form.value.name.trim(),
      // 计时行业不涉及条码/店内码/库存，统一置空避免残留零售语义
      barcode: timed ? null : (form.value.barcode.trim() || null),
      instoreCode: timed ? null : (form.value.instoreCode.trim() || null),
      categoryId: form.value.categoryId,
      unit: timed ? (form.value.unit.trim() || '小时') : (form.value.unit.trim() || '件'),
      salePriceCents: yuanToCents(form.value.salePrice || '0', '售价'),
      costPriceCents: timed ? 0 : yuanToCents(form.value.costPrice || '0', '成本'),
      memberPriceCents: form.value.memberPrice.trim()
        ? yuanToCents(form.value.memberPrice, '会员价')
        : null,
      isWeighed: timed ? false : form.value.isWeighed,
      alertQty: timed ? 0 : intOrZero(String(form.value.alertQty), '预警值'),
    }
    const currentSku = editSku.value
    let skuId: number
    if (currentSku != null) {
      await api.updateProductProfile(currentSku, payload)
      skuId = currentSku
    } else {
      const asService = timed || form.value.isService
      skuId = await api.createProduct({
        ...payload,
        isService: asService,
        initialQty: asService ? 0 : intOrZero(String(form.value.initialQty), '初始库存'),
      })
    }
    // 图片：上传的本地文件优先，其次登记在线地址。
    // 商品已落库后再失败不会回滚档案，但必须明确告知并保留编辑入口。
    try {
      if (createImageFile.value) {
        await api.uploadProductImage(skuId, await readFileBase64(createImageFile.value))
      }
      if (form.value.imageUrl.trim()) {
        await api.setProductImageUrl(skuId, form.value.imageUrl.trim())
      }
    } catch (imgErr) {
      closeForm()
      await reload()
      fail(`档案已保存，但图片设置失败：${String(imgErr)}。可再次编辑重试图片`)
      return
    }
    closeForm()
    done(currentSku != null ? '商品档案已保存' : '商品已创建')
    await reload()
  } catch (e) {
    fail(e)
  }
}

/** 关闭表单并复位编辑态 */
function closeForm() {
  showCreate.value = false
  editSku.value = null
  createImageFile.value = null
  form.value = EMPTY_FORM()
}

/** 实体/服务互转：服务项目不计库存，转换前需库存为 0 且不能是称重商品。 */
async function toggleService(p: ProductRow) {
  const target = !p.is_service
  if (target && !window.confirm(`把「${p.name}」设为服务项目？服务项目不计库存，账面库存必须为 0。`)) return
  try {
    await api.setProductService(p.sku_id, target)
    done(target ? '已设为服务项目' : '已转回实体商品')
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

onMounted(reload)
</script>

<template>
  <div class="page-body">
    <p v-if="lowStockCount > 0" class="notice-warning">
      <Icon name="alert" :size="ICON_SIZE.sm" /> 有 {{ lowStockCount }} 个商品库存低于预警值，建议及时补货（下表以警示色标出）
    </p>

    <div class="toolbar">
      <input v-model="keyword" class="input" placeholder="搜索商品名 / 条码" @keyup.enter="reload" />
      <button class="btn" :disabled="loading" @click="reload">
        <span v-if="loading" class="btn-spinner" aria-label="查询中"></span>
        {{ loading ? '查询中…' : '查询' }}
      </button>
      <button class="btn primary" :aria-expanded="showCreate" @click="showCreate ? closeForm() : openCreate()">{{ showCreate ? '收起表单' : '新增商品' }}</button>
    </div>

    <p v-if="err" class="error">{{ err }}</p>
    <p v-if="okMsg" class="ok-msg">{{ okMsg }}</p>

    <!-- 新增/编辑共用同一张完整表单；计时行业（棋牌/养生）显示按时间计费的精简表单 -->
    <div v-if="showCreate" class="card">
      <h3>{{ editSku != null ? `编辑商品 · ${editingProduct?.name ?? ''}` : '新增商品' }}</h3>
      <!-- 计时行业：项目按时间计费，不计库存、无条码 -->
      <template v-if="timedIndustry">
        <div class="form-grid">
          <label>名称<input v-model="form.name" class="input" placeholder="如 棋牌房费 / 按摩理疗" /></label>
          <label>分类
            <select v-model="form.categoryId" class="input">
              <option v-for="c in categories" :key="c.category_id" :value="c.category_id">{{ c.name }}</option>
            </select>
          </label>
          <label>单价（元）<input v-model="form.salePrice" class="input" placeholder="如 20.00" /></label>
          <label>计价单位
            <select v-model="form.unit" class="input">
              <option v-for="u in TIMED_UNITS" :key="u" :value="u">{{ u }}</option>
            </select>
          </label>
          <label>会员价（元）<input v-model="form.memberPrice" class="input" placeholder="可选" /></label>
          <Checkbox :model-value="true" disabled label="计时/服务项目（不计库存）" />
        </div>
      </template>
      <!-- 零售行业：完整商品档案 -->
      <div v-else class="form-grid">
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
        <label>期初库存<input v-model.number="form.initialQty" class="input" type="number" min="0" :disabled="form.isService || editSku != null" /></label>
        <label>库存预警值<input v-model.number="form.alertQty" class="input" type="number" min="0" :disabled="form.isService" /></label>
        <Checkbox
          v-model="form.isService"
          :disabled="editSku != null"
          label="服务项目（不计库存）"
          @change="onServiceToggle"
        />
        <Checkbox v-model="form.isWeighed" :disabled="form.isService" label="称重商品（按克计价）" />
      </div>
      <div class="form-grid">
        <label>商品图片（本地文件）
          <input class="input" type="file" accept="image/png,image/jpeg,image/webp,image/gif" @change="pickCreateImage" />
        </label>
        <label>商品图片（在线地址）<input v-model="form.imageUrl" class="input" placeholder="https://…（可选）" /></label>
      </div>
      <div class="op-form">
        <button class="btn primary" @click="submitProduct">{{ editSku != null ? '保存修改' : '创建商品' }}</button>
        <button class="btn ghost" @click="closeForm">取消</button>
      </div>
      <IndustryAttributesPanel
        v-if="editSku != null"
        :sku-id="editSku"
        :spu-id="editingProduct?.spu_id ?? 0"
        :scene="industry"
        :base-price-cents="Math.round(Number(form.salePrice || 0) * 100)"
      />
    </div>

    <!-- 触屏卡片化：8 列表格改商品卡，五个行内操作改为卡底整宽按钮 -->
    <div class="tcard-list">
      <div v-for="p in products" :key="p.sku_id" class="tcard" :class="{ 'row-disabled': p.status === 0 }">
        <div class="tcard-head">
          <img v-if="imageSrc(p)" :src="imageSrc(p)!" class="thumb" alt="" />
          <span v-else class="thumb"></span>
          <span class="tcard-code">
            {{ p.name }}
            <span v-if="p.is_weighed" class="muted small">（称重/{{ p.unit }}）</span>
            <span v-else-if="p.is_service" class="muted small">（服务/{{ p.unit }}）</span>
          </span>
          <span class="badge" :class="p.status === 1 ? 'badge-ok' : 'badge-off'">{{ p.status === 1 ? '在售' : '停用' }}</span>
        </div>
        <div class="tcard-line"><span class="tcard-key">条码</span><span class="tcard-value mono">{{ p.barcode || '—' }}</span></div>
        <div class="tcard-line"><span class="tcard-key">分类</span><span class="tcard-value">{{ p.category_name }}</span></div>
        <div class="tcard-line"><span class="tcard-key">售价</span><span class="tcard-value">{{ formatMoney(p.sale_price_cents) }}</span></div>
        <div class="tcard-line"><span class="tcard-key">库存</span><span class="tcard-value" :class="{ 'warning-text': !p.is_service && p.qty <= p.alert_qty }">
          <span v-if="p.is_service">—</span>
          <template v-else>
            {{ p.qty }} {{ p.unit }}<span v-if="p.qty <= p.alert_qty" class="muted small">（低于预警 {{ p.alert_qty }}）</span>
          </template>
        </span></div>
        <div class="tcard-actions">
          <button class="btn" @click="startEdit(p)">编辑</button>
          <button v-if="!p.is_service" class="btn" @click="startAdjust(p)">库存</button>
          <button class="btn ghost" @click="toggleService(p)">{{ p.is_service ? '转实体' : '设为服务' }}</button>
          <button class="btn" @click="startImage(p)">图片</button>
          <button class="btn ghost" @click="toggleStatus(p)">{{ p.status === 1 ? '停用' : '启用' }}</button>
        </div>
      </div>
      <div v-if="products.length === 0" class="empty">暂无商品，点击上方「新增商品」创建第一个商品</div>
    </div>

    <div v-if="imageSku != null" class="card">
      <h3>商品图片（SKU {{ imageSku }}）</h3>
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
      <div class="form-row">
        <input v-model="adjustQty" class="input" type="number" placeholder="变化数量（正数盘盈，负数盘亏），如 5 或 -3" />
        <input v-model="adjustReason" class="input" placeholder="原因（必填），如 破损报废" />
        <button class="btn primary" @click="saveAdjust">确认调整</button>
        <button class="btn ghost" @click="adjustSku = null">取消</button>
      </div>
    </div>

  </div>
</template>

<style scoped>
/* 组件特有样式；通用类（page-head/toolbar/tbl/form-grid/chip 等）已上移全局 styles.css */
/* 商品缩略图：固定方形，缺图不撑表格 */
.thumb {
  width: var(--control-thumb-size); height: var(--control-thumb-size);
  object-fit: cover; border-radius: var(--radius-sm);
  border: var(--border-width) solid var(--border);
  background: var(--surface-subtle);
}
</style>
