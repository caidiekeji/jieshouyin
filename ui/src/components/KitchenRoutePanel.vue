<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { api, type Category, type KitchenRoute } from '../api'

const props = defineProps<{ canManage: boolean }>()
const emit = defineEmits<{ changed: [] }>()

const open = ref(false)
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
    emit('changed')
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
  <section v-if="canManage" class="route-panel scene-kitchen">
    <div class="route-head">
      <button class="link" @click="open = !open">
        {{ open ? '收起' : '厨打档口路由' }}
        <span class="muted">（已配 {{ routes.length }} 个档口）</span>
      </button>
      <span v-if="!open" class="muted small">未配置时全部走打印设置里的主打印机</span>
    </div>
    <div v-if="open" class="route-body">
      <div class="route-add">
        <select v-model.number="form.categoryId" class="input input-sm">
          <option v-for="c in categories" :key="c.category_id" :value="c.category_id">{{ c.name }}</option>
        </select>
        <input v-model="form.label" class="input input-sm" placeholder="档口名 热菜" maxlength="20" />
        <input v-model="form.host" class="input input-sm" placeholder="打印机 IP/主机名" maxlength="64" />
        <input v-model.number="form.port" class="input input-xs" type="number" min="1" max="65535" />
        <button class="btn btn-sm primary" :disabled="busy" @click="upsert">保存档口</button>
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
  </section>
</template>

<style scoped>
/* 配色来自场景语义层（tokens.css .scene-kitchen），暗色只翻语义层。 */
.route-panel {
  margin: 0 var(--space-3) var(--space-2);
  border: var(--border-width) solid var(--scene-line);
  border-radius: var(--radius-sm);
  background: var(--scene-surface);
  padding: var(--space-2) var(--space-3);
}
.route-head { display: flex; justify-content: space-between; gap: var(--space-2); }
.route-head .link { font-size: var(--font-xs); color: var(--scene-title); }
.route-body { margin-top: var(--space-2); }
.route-add { display: flex; flex-wrap: wrap; gap: var(--space-2); align-items: center; }
.input-xs { width: var(--control-input-xs-width); }
.route-list {
  list-style: none; padding: 0; margin: var(--space-2) 0 0;
  display: flex; flex-wrap: wrap; gap: var(--space-1) var(--space-4);
  font-size: var(--font-xs);
}
</style>
