<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { api, formatMoney, type ProductHit } from '../api'

const emit = defineEmits<{ add: [product: ProductHit]; error: [message: string] }>()

const services = ref<ProductHit[]>([])
const group = ref('推拿理疗')
const groups = ['推拿理疗', '艾灸', '拔罐刮痧', '足疗', '套餐', '养生产品']

const filtered = computed(() =>
  services.value.filter((p) => p.category_group.includes(group.value)).slice(0, 30)
)

function add(p: ProductHit) {
  emit('add', p)
}

async function load() {
  try {
    services.value = await api.lookup('', 200)
  } catch (e) {
    emit('error', String(e))
  }
}

onMounted(load)
</script>

<template>
  <section class="wellness-board">
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
.wellness-board {
  border-bottom: 1px solid #dfe7dc;
  padding: 10px 12px 12px;
  background: linear-gradient(180deg, #f6f9f3, #fcfbf7);
}
.well-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 10px;
  margin-bottom: 8px;
  flex-wrap: wrap;
}
.well-head strong {
  color: #3f6148;
  margin-right: 8px;
}
.well-head span {
  color: #849083;
  font-size: 12px;
}
.well-head nav {
  display: flex;
  gap: 3px;
  flex-wrap: wrap;
  border-bottom: 1px solid #dfe7dc;
}
.well-head nav button {
  border: 0;
  background: none;
  padding: 5px 9px;
  color: #6f7d6e;
  font-size: 12px;
  cursor: pointer;
  border-bottom: 2px solid transparent;
}
.well-head nav button.active {
  color: #31543b;
  border-bottom-color: #6d9b76;
  font-weight: 600;
}
.service-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(210px, 1fr));
  gap: 7px;
  max-height: 205px;
  overflow: auto;
}
.service-card {
  display: flex;
  align-items: center;
  gap: 9px;
  text-align: left;
  border: 1px solid #e3e9df;
  border-radius: 8px;
  background: #fff;
  padding: 9px 10px;
  cursor: pointer;
}
.service-card:hover {
  border-color: #a9c0a8;
  background: #fbfdf9;
}
.line {
  width: 3px;
  height: 26px;
  border-radius: 2px;
  background: #9ab79c;
  flex: none;
}
.service-card > div {
  flex: 1;
  min-width: 0;
}
.service-card strong {
  font-size: 13px;
  color: #374b3a;
  display: block;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.service-card p {
  margin: 2px 0 0;
  font-size: 11px;
  color: #8b978a;
}
.price {
  color: #4f7357;
  font-weight: 700;
  white-space: nowrap;
}
.empty-service {
  grid-column: 1 / -1;
  color: #8b978a;
  border: 1px dashed #d9e3d7;
  border-radius: 8px;
  padding: 14px;
}
</style>
