<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { api, type Category, type KitchenRoute } from '../api'
import HintTip from './HintTip.vue'

/** 厨打档口路由小节：嵌入设置页「打印机设置」卡片内，按分类绑定档口打印机。 */
const props = defineProps<{ canManage: boolean }>()

const busy = ref(false)
const routes = ref<KitchenRoute[]>([])
const categories = ref<Category[]>([])
const form = ref({ categoryId: 0, label: '', host: '', port: 9100 })

async function load() {
  try {
    routes.value = await api.getKitchenRoutes()
    categories.value = await api.listCategories()
    if (!form.value.categoryId && categories.value.length) {
      form.value.categoryId = categories.value[0].category_id
    }
  } catch {
    routes.value = []
    categories.value = []
  }
}

async function save(next: KitchenRoute[]) {
  busy.value = true
  try {
    await api.setKitchenRoutes(next)
    routes.value = next
  } finally {
    busy.value = false
  }
}

async function upsert() {
  const f = form.value
  if (!f.categoryId || !f.label.trim() || !f.host.trim()) return
  const route: KitchenRoute = {
    category_id: f.categoryId,
    label: f.label.trim(),
    host: f.host.trim(),
    port: f.port,
  }
  await save([...routes.value.filter((r) => r.category_id !== route.category_id), route])
}

async function remove(r: KitchenRoute) {
  await save(routes.value.filter((x) => x.category_id !== r.category_id))
}

function categoryName(id: number): string {
  return categories.value.find((c) => c.category_id === id)?.name ?? `分类 ${id}`
}

onMounted(() => {
  if (props.canManage) void load()
})
</script>

<template>
  <div v-if="canManage" class="route-section">
    <div class="route-add">
      <select v-model.number="form.categoryId" class="input input-sm" aria-label="选择分类">
        <option v-for="c in categories" :key="c.category_id" :value="c.category_id">{{ c.name }}</option>
      </select>
      <input v-model="form.label" class="input input-sm route-label" placeholder="档口名，如 热菜" maxlength="20" />
      <input v-model="form.host" class="input input-sm route-host" placeholder="打印机 IP/主机名" maxlength="64" />
      <input v-model.number="form.port" class="input input-sm route-port" type="number" min="1" max="65535" aria-label="端口" />
      <button class="btn btn-sm primary" :disabled="busy" @click="upsert">添加档口</button>
    </div>
    <ul class="route-list">
      <li v-for="r in routes" :key="r.category_id">
        <strong>{{ r.label }}</strong>
        <span class="muted">{{ categoryName(r.category_id) }} → {{ r.host }}:{{ r.port }}</span>
        <button class="link danger" :disabled="busy" @click="remove(r)">删除</button>
      </li>
      <li v-if="!routes.length" class="muted small">还没有档口，未配置的分类会打到主打印机。</li>
    </ul>
  </div>
  <HintTip v-else text="仅店长及以上可修改档口路由。" />
</template>

<style scoped>
/* 组件局部布局变量：档口路由行的列宽基准（色彩一律走全局 token，这里只管尺寸） */
.route-section {
  /* 标签列：容纳「打印分类」四字标签不换行 */
  --route-label-w: 120px;
  /* 主机列：IP/主机名可伸展的最小可用宽 */
  --route-host-min-w: 160px;

  display: flex; flex-direction: column; gap: var(--space-2);
}
.route-add { display: flex; flex-wrap: wrap; gap: var(--space-2); align-items: center; }
.route-label { width: var(--route-label-w); }
.route-host { flex: 1; min-width: var(--route-host-min-w); }
.route-port { width: var(--control-input-xs-width); }
.route-list {
  list-style: none; padding: 0; margin: 0;
  display: flex; flex-direction: column; gap: var(--space-1);
  font-size: var(--font-sm);
}
.route-list li { display: flex; align-items: center; gap: var(--space-2); }
.route-list .link { margin-left: auto; font-size: var(--font-xs); }
</style>
