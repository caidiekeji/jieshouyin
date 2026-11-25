<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { api, formatMoney, type ProductHit } from '../api'
import SceneBoard from './SceneBoard.vue'

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
  <SceneBoard
    class="scene-wellness"
    title="养生项目"
    :min-card="210"
    :empty-text="filtered.length ? '' : `「${group}」下还没有项目。到商品管理按推拿理疗、艾灸、足疗等分类建服务项目。`"
  >
    <template #action>
      <nav class="group-tabs">
        <button
          v-for="g in groups"
          :key="g"
          :class="{ active: group === g }"
          @click="group = g"
        >{{ g }}</button>
      </nav>
    </template>
    <button v-for="p in filtered" :key="p.sku_id" class="scene-card service-card" @click="add(p)">
      <span class="line"></span>
      <div>
        <strong>{{ p.spu_name }}</strong>
        <p>{{ p.category_name }} · 按{{ p.unit }}计</p>
      </div>
      <span class="price">¥{{ formatMoney(p.price_cents) }}</span>
    </button>
  </SceneBoard>
</template>

<style scoped>
/* 养生项目卡片：左侧装饰竖条 + 名称/分类 + 价格；行业色全部来自 --scene-* 语义层 */
.service-card { display: flex; align-items: center; gap: var(--space-3); }
.service-card strong {
  font-size: var(--font-sm);
  color: var(--scene-heading);
  display: block;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.service-card p { margin: var(--space-0_5) 0 0; font-size: var(--font-2xs); color: var(--scene-sub); }
.service-card > div { flex: 1; min-width: 0; }
.line {
  width: var(--hairline);
  height: var(--space-8);
  border-radius: var(--radius-xs);
  background: var(--scene-card-hover);
  flex: none;
}
.price { color: var(--scene-accent); font-weight: 700; white-space: nowrap; }
/* 项目分组：下划线 tab（与全局 segmented 同语义，行业强调色激活） */
.group-tabs {
  display: flex;
  gap: var(--space-0_5);
  flex-wrap: wrap;
  border-bottom: var(--border-width) solid var(--scene-line);
}
.group-tabs button {
  border: 0;
  background: none;
  padding: var(--space-1) var(--space-3);
  color: var(--scene-sub);
  font-size: var(--font-xs);
  cursor: pointer;
  border-bottom: 2px solid transparent;
}
.group-tabs button:hover { color: var(--scene-accent); }
.group-tabs button:focus-visible { outline: 2px solid var(--scene-accent); outline-offset: -2px; }
.group-tabs button.active {
  color: var(--scene-heading);
  border-bottom-color: var(--scene-soft-line, var(--scene-accent));
  font-weight: 600;
}
</style>
