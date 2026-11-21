<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { api, formatMoney, type ProductHit } from '../api'

const emit = defineEmits<{ add: [product: ProductHit]; error: [message: string] }>()

const products = ref<ProductHit[]>([])
const group = ref('全部')
const groups = ['全部', '奶粉', '纸尿裤', '辅食', '洗护', '玩具', '童装', '孕产']

function add(product: ProductHit) {
  emit('add', product)
}

const filtered = computed(() => {
  if (group.value === '全部') return products.value.slice(0, 24)
  return products.value.filter((p) => p.category_group.includes(group.value)).slice(0, 24)
})

async function load() {
  try {
    products.value = await api.lookup('', 200)
  } catch (e) {
    emit('error', String(e))
  }
}

onMounted(load)
</script>

<template>
  <section class="baby-board">
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
.baby-board {
  border-bottom: 1px solid #f1dfe4;
  padding: 10px 12px 12px;
  background: linear-gradient(135deg, #fff7fa, #f2fbf7 65%);
}
.baby-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 10px;
  margin-bottom: 8px;
  flex-wrap: wrap;
}
.baby-head strong {
  color: #b35472;
  margin-right: 8px;
}
.baby-head span {
  color: #8f7f85;
  font-size: 12px;
}
.chips {
  display: flex;
  gap: 5px;
  flex-wrap: wrap;
}
.chip {
  border: 1px solid #efd6de;
  background: #fff;
  color: #8a6572;
  border-radius: 999px;
  padding: 3px 11px;
  font-size: 12px;
  cursor: pointer;
}
.chip.active {
  background: #f7dbe4;
  border-color: #e8b3c6;
  color: #8e3e5d;
  font-weight: 600;
}
.baby-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(132px, 1fr));
  gap: 8px;
  max-height: 205px;
  overflow: auto;
}
.baby-card {
  text-align: left;
  border: 1px solid #f0e1e6;
  border-radius: 14px;
  background: #fff;
  padding: 9px 10px;
  cursor: pointer;
  display: flex;
  flex-direction: column;
  gap: 3px;
}
.baby-card:hover {
  border-color: #e0a8bb;
}
.stage {
  align-self: flex-start;
  padding: 1px 7px;
  border-radius: 999px;
  background: #eff8f2;
  color: #4f7a63;
  font-size: 11px;
}
.baby-card strong {
  font-size: 13px;
  color: #4b3f44;
  line-height: 1.3;
}
.baby-card footer {
  display: flex;
  align-items: baseline;
  gap: 2px;
  margin-top: auto;
}
.price {
  color: #b35472;
  font-weight: 700;
}
.unit {
  color: #a99ba0;
  font-size: 11px;
}
.empty-baby {
  grid-column: 1 / -1;
  color: #9b8b91;
  border: 1px dashed #efd3dc;
  border-radius: 12px;
  padding: 14px;
}
</style>
