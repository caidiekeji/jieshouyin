<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { api, formatMoney, type ProductHit } from '../api'
import SceneBoard from './SceneBoard.vue'

const emit = defineEmits<{ add: [product: ProductHit]; error: [message: string] }>()

const products = ref<ProductHit[]>([])
const group = ref('全部')
const groups = ['全部', '奶粉', '纸尿裤', '辅食', '洗护', '玩具', '童装', '孕产']

function add(product: ProductHit) {
  emit('add', product)
}

const filtered = computed(() => products.value.slice(0, 24))

async function load() {
  try {
    products.value = await api.lookup(
      '',
      200,
      group.value === '全部' ? undefined : group.value
    )
  } catch (e) {
    emit('error', String(e))
  }
}

watch(group, load)
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
    </template>
    <button v-for="p in filtered" :key="p.sku_id" class="scene-card baby-card" @click="add(p)">
      <span class="stage">{{ p.category_name }}</span>
      <strong>{{ p.spu_name }}</strong>
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
