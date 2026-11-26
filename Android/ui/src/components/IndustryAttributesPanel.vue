<script setup lang="ts">
/**
 * 行业商品属性面板（可复用）：按行业显示真实后端模型，不做假界面。
 * 服装=款色码规格矩阵；母婴=月龄与批次效期；餐饮/养生=套餐与加料。
 */
import { computed, onMounted, ref, watch } from 'vue'
import {
  api,
  formatMoney,
  type Combo,
  type OptionGroup,
  type SkuBatch,
  type SkuVariant,
  type SpecGroup,
} from '../api'

const props = defineProps<{
  skuId: number
  spuId: number
  scene: string
  basePriceCents: number
}>()

const error = ref('')
const okMsg = ref('')
const groups = ref<SpecGroup[]>([])
const variants = ref<SkuVariant[]>([])
const batches = ref<SkuBatch[]>([])
const combos = ref<Combo[]>([])
const optionGroups = ref<OptionGroup[]>([])

const newGroup = ref('')
const valueDraft = ref<Record<number, string>>({})
const ageForm = ref({ min: '', max: '', shelf: '' })
const batchForm = ref({ no: '', production: '', expiry: '', qty: 0 })
const comboForm = ref({ note: '', itemSku: '', itemQty: 1 })
const optionForm = ref({ name: '' })

const isApparel = computed(() => props.scene === 'CLOTHING')
const isBaby = computed(() => props.scene === 'BABY')
const isDining = computed(() => props.scene === 'RESTAURANT' || props.scene === 'WELLNESS')

async function load() {
  error.value = ''
  try {
    if (isApparel.value) {
      groups.value = await api.listSpecGroups(props.spuId)
      variants.value = await api.listSkuVariants(props.spuId)
    }
    if (isBaby.value) batches.value = await api.listSkuBatches(props.skuId)
    if (isDining.value) {
      combos.value = await api.listCombos()
      optionGroups.value = await api.listOptionGroups(props.scene)
    }
  } catch (e) {
    error.value = String(e)
  }
}
watch(() => props.skuId, load)
onMounted(load)

async function act(fn: () => Promise<unknown>, message: string) {
  error.value = ''
  okMsg.value = ''
  try {
    await fn()
    await load()
    okMsg.value = message
  } catch (e) {
    error.value = String(e)
  }
}

/** 按现有规格组/值生成笛卡尔积组合，价格统一取当前商品售价生成后再单独调价。 */
async function generateVariants() {
  const pools = groups.value.map((g) => g.values.map((v) => v.spec_value_id))
  if (!pools.length || pools.some((p) => !p.length)) {
    error.value = '请先为每个规格组添加至少一个规格值'
    return
  }
  let combinations: number[][] = [[]]
  for (const pool of pools) {
    const next: number[][] = []
    for (const c of combinations) for (const v of pool) next.push([...c, v])
    combinations = next
  }
  const inputs = combinations.map((ids) => ({
    spec_value_ids: ids,
    sale_price_cents: props.basePriceCents,
    cost_price_cents: 0,
    member_price_cents: null,
    barcode: null,
    instore_code: null,
    initial_qty: 0,
  }))
  await act(
    () => api.createSkuVariants(props.spuId, inputs),
    `已生成 ${combinations.length} 个规格组合`
  )
}
</script>

<template>
  <div class="card industry-attrs">
    <strong>行业属性 · 当前商品</strong>

    <template v-if="isApparel">
      <div class="op-form">
        <input v-model="newGroup" class="input" placeholder="规格组名，如 颜色 / 尺码" />
        <button
          class="btn"
          :disabled="!newGroup.trim()"
          @click="act(() => api.createSpecGroup(spuId, newGroup.trim()), '已添加规格组'); newGroup = ''"
        >添加规格组</button>
      </div>
      <div v-for="g in groups" :key="g.spec_group_id" class="attr-row">
        <span class="attr-label">{{ g.name }}</span>
        <span v-for="v in g.values" :key="v.spec_value_id" class="tag">{{ v.value }}</span>
        <input v-model="valueDraft[g.spec_group_id]" class="input input-sm" placeholder="新规格值" />
        <button
          class="btn btn-sm ghost"
          :disabled="!valueDraft[g.spec_group_id]?.trim()"
          @click="act(() => api.addSpecValue(g.spec_group_id, (valueDraft[g.spec_group_id] ?? '').trim()), '已添加规格值'); valueDraft[g.spec_group_id] = ''"
        >添加</button>
      </div>
      <div class="op-form">
        <button class="btn primary" @click="generateVariants">按组合生成 SKU</button>
      </div>
      <div v-if="variants.length > 1" class="attr-list">
        <div v-for="v in variants" :key="v.sku_id" class="attr-row">
          <span>{{ v.spec_text || '基础款' }}</span>
          <span class="muted small">¥{{ formatMoney(v.sale_price_cents) }}</span>
          <span class="muted small">库存 {{ v.qty }}</span>
        </div>
      </div>
    </template>

    <template v-else-if="isBaby">
      <div class="op-form">
        <label>月龄起<input v-model="ageForm.min" class="input" type="number" min="0" max="240" placeholder="如 6" /></label>
        <label>月龄止<input v-model="ageForm.max" class="input" type="number" min="0" max="240" placeholder="如 12" /></label>
        <label>保质期(天)<input v-model="ageForm.shelf" class="input" type="number" min="0" placeholder="如 540" /></label>
        <button
          class="btn"
          @click="act(() => api.setProductAgeRange(
            skuId,
            ageForm.min === '' ? null : Number(ageForm.min),
            ageForm.max === '' ? null : Number(ageForm.max),
            ageForm.shelf === '' ? 0 : Number(ageForm.shelf)
          ), '已保存月龄与保质期')"
        >保存月龄</button>
      </div>
      <div class="op-form">
        <input v-model="batchForm.no" class="input" placeholder="批号" />
        <input v-model="batchForm.production" class="input" type="date" aria-label="生产日期" />
        <input v-model="batchForm.expiry" class="input" type="date" aria-label="失效日期" />
        <input v-model.number="batchForm.qty" class="input" type="number" min="0" placeholder="数量" />
        <button
          class="btn"
          :disabled="!batchForm.no.trim()"
          @click="act(() => api.createSkuBatch(
            skuId, batchForm.no.trim(), batchForm.production || null, batchForm.expiry || null, batchForm.qty
          ), '已登记批次'); batchForm.no = ''"
        >登记批次</button>
      </div>
      <div v-if="batches.length" class="attr-list">
        <div v-for="b in batches" :key="b.batch_id" class="attr-row">
          <span>{{ b.batch_no }}</span>
          <span class="muted small">生产 {{ b.production_date || '—' }} · 效期 {{ b.expiry_date || '—' }}</span>
          <span class="muted small">{{ b.qty }} 件</span>
        </div>
      </div>
    </template>

    <template v-else-if="isDining">
      <div class="op-form">
        <input v-model="comboForm.note" class="input" placeholder="套餐说明（可空）" />
        <button
          class="btn"
          @click="act(() => api.createCombo(skuId, comboForm.note.trim() || null), '已登记为套餐；请在下方添加明细')"
        >把当前商品设为套餐</button>
      </div>
      <div v-for="c in combos.filter((c) => c.sku_id === skuId)" :key="c.combo_id" class="attr-row">
        <span>套餐 {{ c.name }} · ¥{{ formatMoney(c.price_cents) }}</span>
        <input v-model="comboForm.itemSku" class="input input-sm" type="number" min="1" placeholder="明细 SKU" />
        <input v-model.number="comboForm.itemQty" class="input input-sm" type="number" min="1" max="99" placeholder="份数" />
        <button
          class="btn btn-sm ghost"
          :disabled="!comboForm.itemSku"
          @click="act(() => api.setComboItems(c.combo_id, [{ sku_id: Number(comboForm.itemSku), qty: Number(comboForm.itemQty) }]), '已保存套餐明细')"
        >设置明细</button>
        <span class="muted small">{{ c.items.map((i) => `${i.name}×${i.qty}`).join(' + ') || '未配置明细' }}</span>
      </div>
      <div class="op-form">
        <input v-model="optionForm.name" class="input" placeholder="加料选项组名，如 加料" />
        <button
          class="btn"
          :disabled="!optionForm.name.trim()"
          @click="act(async () => { const id = await api.createOptionGroup(optionForm.name.trim(), scene); await api.setOptionItems(id, [skuId]); optionForm.name = '' }, '已创建选项组并加入当前商品')"
        >建加料组并加入当前商品</button>
      </div>
      <div v-if="optionGroups.length" class="attr-list">
        <div v-for="g in optionGroups" :key="g.group_id" class="attr-row">
          <span>{{ g.name }}</span>
          <span class="muted small">{{ g.items.map((i) => `${i.name} ¥${formatMoney(i.price_cents)}`).join(' · ') || '暂无选项' }}</span>
        </div>
      </div>
    </template>

    <p v-if="error" class="tc-error">{{ error }}</p>
    <p v-else-if="okMsg" class="muted small">{{ okMsg }}</p>
  </div>
</template>

<style scoped>
.industry-attrs { margin-top: var(--space-2); }
.attr-row {
  display: flex; align-items: center; gap: var(--space-2); flex-wrap: wrap;
  padding: var(--space-1) 0; border-bottom: var(--border-width) solid var(--scene-line);
  font-size: var(--font-xs);
}
.attr-label { min-width: var(--control-input-xs-width); color: var(--scene-title); }
.attr-list { margin-top: var(--space-1); }
</style>
