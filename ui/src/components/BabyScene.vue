<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { api, formatMoney, type ProductHit } from '../api'

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
  <section class="baby-board scene-baby">
    <div class="baby-head">
      <div><strong>母婴选购</strong><span>按月龄段与品类分档，点卡片直接加购</span></div>
      <div class="chips">
        <button
          v-for="g in groups"
          :key="g"
          :class="['chip', { active: group === g }]"
          @click="group = g"
        >{{ g }}</button>
      </div>
    </div>
    <div class="baby-grid">
      <button v-for="p in filtered" :key="p.sku_id" class="baby-card" @click="add(p)">
        <span class="stage">{{ p.category_name }}</span>
        <strong>{{ p.spu_name }}</strong>
        <footer>
          <span class="price">¥{{ formatMoney(p.price_cents) }}</span>
          <span class="unit">/{{ p.unit }}</span>
        </footer>
      </button>
      <div v-if="!filtered.length" class="empty-baby">
        当前分类还没有商品。到「商品 → 新增」按奶粉、纸尿裤、辅食等分类建品后即可点选。
      </div>
    </div>
  </section>
</template>

<style scoped>
/* 配色来自场景语义层（tokens.css .scene-baby），暗色只翻语义层；分类标签保持直角。 */
.baby-board {
  border-bottom: var(--border-width) solid var(--scene-line);
  padding: var(--space-2) var(--space-3) var(--space-3);
  background: var(--scene-surface);
  color: var(--scene-title);
}
.baby-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: var(--space-3);
  margin-bottom: var(--space-2);
  flex-wrap: wrap;
}
.baby-head strong { color: var(--scene-title); margin-right: var(--space-2); }
.baby-head span { color: var(--scene-sub); font-size: var(--font-xs); }
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
.baby-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(132px, 1fr));
  gap: var(--space-2);
  max-height: var(--scene-board-max-height);
  overflow: auto;
}
.baby-card {
  text-align: left;
  border: var(--border-width) solid var(--scene-card-line);
  border-radius: var(--radius-sm);
  background: var(--scene-card);
  padding: var(--space-2) var(--space-3);
  cursor: pointer;
  display: flex;
  flex-direction: column;
  gap: var(--space-0_5);
}
.baby-card:hover { border-color: var(--scene-card-hover); }
.baby-card:focus-visible { outline: 2px solid var(--scene-accent); outline-offset: 2px; }
.stage {
  align-self: flex-start;
  padding: 0 var(--space-2);
  border-radius: var(--radius-xs);
  background: var(--scene-tag-bg);
  color: var(--scene-tag-text);
  font-size: var(--font-2xs);
}
.baby-card strong { font-size: var(--font-sm); color: var(--scene-heading); line-height: var(--line-tight); }
.baby-card footer {
  display: flex;
  align-items: baseline;
  gap: var(--space-0_5);
  margin-top: auto;
}
.price { color: var(--scene-accent); font-weight: 700; }
.unit { color: var(--scene-sub); font-size: var(--font-2xs); }
.empty-baby {
  grid-column: 1 / -1;
  color: var(--scene-sub);
  border: var(--border-width) dashed var(--scene-empty-line);
  border-radius: var(--radius-sm);
  padding: var(--space-4);
}
</style>
