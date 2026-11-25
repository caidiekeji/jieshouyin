<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { api, formatMoney, type AgeProduct, type ProductHit } from '../api'
import SceneBoard from './SceneBoard.vue'

const emit = defineEmits<{ add: [product: ProductHit]; error: [message: string] }>()

const products = ref<ProductHit[]>([])
const group = ref('全部')
const groups = ['全部', '奶粉', '纸尿裤', '辅食', '洗护', '玩具', '童装', '孕产']

/** 后端维护的月龄区间转为商品标签；效期仅作提示，不参与库存扣减 */
const ages = [
  { label: '全部月龄', value: 0 },
  { label: '0-6月', value: 3 },
  { label: '7-12月', value: 9 },
  { label: '13-24月', value: 18 },
  { label: '25-36月', value: 30 },
  { label: '37月+', value: 48 },
]
const month = ref(0)

function toHit(p: AgeProduct): ProductHit {
  const range = p.month_min != null ? `${p.month_min}-${p.month_max ?? '以上'}月` : ''
  const expiry = p.expiry_date ? ` 效期 ${p.expiry_date}` : ''
  return {
    sku_id: p.sku_id,
    spu_id: 0,
    spu_name: p.name,
    barcode: '',
    spec_text: `${range}${expiry}`.trim(),
    price_cents: p.price_cents,
    member_price_cents: p.member_price_cents,
    is_weighed: false,
    category_name: p.category_name,
    category_group: '',
    unit: p.unit,
    image_path: null,
    image_url: null,
  }
}

function add(product: ProductHit) {
  emit('add', product)
}

const filtered = computed(() => products.value.slice(0, 24))

async function load() {
  try {
    if (month.value === 0) {
      products.value = await api.lookup(
        '',
        200,
        group.value === '全部' ? undefined : group.value
      )
    } else {
      const rows = await api.listProductsByAge(month.value, 200)
      products.value = rows
        .filter(
          (p) =>
            group.value === '全部' ||
            p.category_name === group.value ||
            p.category_name.includes(group.value)
        )
        .map(toHit)
    }
  } catch (e) {
    emit('error', String(e))
  }
}

watch([group, month], load)
onMounted(load)
</script>

<template>
  <SceneBoard
    class="scene-baby"
    title="母婴选购"
    hint="按月龄段与品类分档，点卡片直接加购"
    :min-card="132"
    :empty-text="filtered.length ? '' : '当前分类还没有商品。到「商品 → 新增」按奶粉、纸尿裤、辅食等分类建品后即可点选。'"
  >
    <template #action>
      <div class="chips">
        <button
          v-for="g in groups"
          :key="g"
          :class="['chip', { active: group === g }]"
          @click="group = g"
        >{{ g }}</button>
      </div>
      <div class="chips age-chips" aria-label="月龄筛选">
        <button
          v-for="a in ages"
          :key="a.value"
          :class="['chip', { active: month === a.value }]"
          @click="month = a.value"
        >{{ a.label }}</button>
      </div>
    </template>
    <button v-for="p in filtered" :key="p.sku_id" class="scene-card baby-card" @click="add(p)">
      <span class="stage">{{ p.category_name }}</span>
      <strong>{{ p.spu_name }}</strong>
      <span v-if="p.spec_text" class="stage age-tag">{{ p.spec_text }}</span>
      <footer>
        <span class="price">¥{{ formatMoney(p.price_cents) }}</span>
        <span class="unit">/{{ p.unit }}</span>
      </footer>
    </button>
  </SceneBoard>
</template>

<style scoped>
/* 母婴卡片内部排版：月龄标签 + 价格脚注；行业色全部来自 --scene-* 语义层 */
.baby-card { display: flex; flex-direction: column; gap: var(--space-0_5); }
.baby-card strong { font-size: var(--font-sm); color: var(--scene-heading); line-height: var(--line-tight); }
.stage {
  align-self: flex-start;
  padding: 0 var(--space-2);
  border-radius: var(--radius-xs);
  background: var(--scene-tag-bg);
  color: var(--scene-tag-text);
  font-size: var(--font-2xs);
}
.baby-card footer { display: flex; align-items: baseline; gap: var(--space-0_5); margin-top: auto; }
.price { color: var(--scene-accent); font-weight: 700; }
.unit { color: var(--scene-sub); font-size: var(--font-2xs); }
/* 品类筛选 chips：直角标签，选中态用行业强调色 */
.chips { display: flex; gap: var(--space-1); flex-wrap: wrap; }
.age-chips { padding-top: var(--space-1); }
.age-tag { background: var(--scene-tag-bg); }
.chip {
  border: var(--border-width) solid var(--scene-card-line);
  background: var(--scene-card);
  color: var(--scene-sub);
  border-radius: 0;
  padding: var(--space-0_5) var(--space-3);
  font-size: var(--font-xs);
  cursor: pointer;
}
.chip:hover { border-color: var(--scene-accent); color: var(--scene-accent); }
.chip:focus-visible { outline: 2px solid var(--scene-accent); outline-offset: -2px; }
.chip.active {
  background: var(--scene-accent);
  border-color: var(--scene-accent);
  color: var(--text-on-brand);
  font-weight: 600;
}
</style>
