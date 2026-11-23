<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { api, formatMoney, type ProductHit } from '../api'

const emit = defineEmits<{ add: [product: ProductHit]; error: [message: string] }>()

const services = ref<ProductHit[]>([])
const group = ref('推拿理疗')
const groups = ['推拿理疗', '艾灸', '拔罐刮痧', '足疗', '套餐', '养生产品']

const filtered = computed(() => services.value.slice(0, 30))

function add(p: ProductHit) {
  emit('add', p)
}

async function load() {
  try {
    services.value = await api.lookup('', 200, group.value)
  } catch (e) {
    emit('error', String(e))
  }
}

watch(group, load)
onMounted(load)
</script>

<template>
  <section class="wellness-board scene-wellness">
    <div class="well-head">
      <div><strong>养生项目</strong><span>按项目分类点单，价格与次数以商品档案为准</span></div>
      <nav>
        <button
          v-for="g in groups"
          :key="g"
          :class="{ active: group === g }"
          @click="group = g"
        >{{ g }}</button>
      </nav>
    </div>
    <div class="service-grid">
      <button v-for="p in filtered" :key="p.sku_id" class="service-card" @click="add(p)">
        <span class="line"></span>
        <div>
          <strong>{{ p.spu_name }}</strong>
          <p>{{ p.category_name }} · 按{{ p.unit }}计</p>
        </div>
        <span class="price">¥{{ formatMoney(p.price_cents) }}</span>
      </button>
      <div v-if="!filtered.length" class="empty-service">
        「{{ group }}」下还没有项目。到商品管理按推拿理疗、艾灸、足疗等分类建服务项目。
      </div>
    </div>
  </section>
</template>

<style scoped>
/* 配色来自场景语义层（tokens.css .scene-wellness），暗色只翻语义层。 */
.wellness-board {
  border-bottom: var(--border-width) solid var(--scene-line);
  padding: var(--space-2) var(--space-3) var(--space-3);
  background: var(--scene-surface);
  color: var(--scene-title);
}
.well-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: var(--space-3);
  margin-bottom: var(--space-2);
  flex-wrap: wrap;
}
.well-head strong { color: var(--scene-title); margin-right: var(--space-2); }
.well-head span { color: var(--scene-sub); font-size: var(--font-xs); }
.well-head nav {
  display: flex;
  gap: var(--space-0_5);
  flex-wrap: wrap;
  border-bottom: var(--border-width) solid var(--scene-line);
}
.well-head nav button {
  border: 0;
  background: none;
  padding: var(--space-1) var(--space-3);
  color: var(--scene-sub);
  font-size: var(--font-xs);
  cursor: pointer;
  border-bottom: 2px solid transparent;
}
.well-head nav button:hover { color: var(--scene-accent); }
.well-head nav button:focus-visible { outline: 2px solid var(--scene-accent); outline-offset: -2px; }
.well-head nav button.active {
  color: var(--scene-heading);
  border-bottom-color: var(--scene-soft-line, var(--scene-accent));
  font-weight: 600;
}
.service-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(210px, 1fr));
  gap: var(--space-2);
  max-height: var(--scene-board-max-height);
  overflow: auto;
}
.service-card {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  text-align: left;
  border: var(--border-width) solid var(--scene-card-line);
  border-radius: var(--radius-sm);
  background: var(--scene-card);
  padding: var(--space-2) var(--space-3);
  cursor: pointer;
}
.service-card:hover { border-color: var(--scene-card-hover); background: var(--scene-surface); }
.service-card:focus-visible { outline: 2px solid var(--scene-accent); outline-offset: 2px; }
.line {
  width: var(--hairline);
  height: var(--space-8);
  border-radius: var(--radius-xs);
  background: var(--scene-card-hover);
  flex: none;
}
.service-card > div { flex: 1; min-width: 0; }
.service-card strong {
  font-size: var(--font-sm);
  color: var(--scene-heading);
  display: block;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.service-card p { margin: var(--space-0_5) 0 0; font-size: var(--font-2xs); color: var(--scene-sub); }
.price { color: var(--scene-accent); font-weight: 700; white-space: nowrap; }
.empty-service {
  grid-column: 1 / -1;
  color: var(--scene-sub);
  border: var(--border-width) dashed var(--scene-empty-line);
  border-radius: var(--radius-sm);
  padding: var(--space-4);
}
</style>
