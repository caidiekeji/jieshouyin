<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { api, type Category } from '../api'
import Icon from './Icon.vue'
import { ICON_SIZE, TOAST_DURATION_MS } from '../constants'

const categories = ref<Category[]>([])
const name = ref('')
const parentId = ref<number | null>(null)
const err = ref('')
const okMsg = ref('')

/** 扁平分类按 parent_id 组装成树；孤儿节点（父不存在）归为顶层，避免丢失 */
const tree = computed(() => {
  const nodes = new Map<number, Category & { children: Category[] }>()
  categories.value.forEach((c) => nodes.set(c.category_id, { ...c, children: [] }))
  const roots: Array<Category & { children: Category[] }> = []
  nodes.forEach((node) => {
    const parent = node.parent_id != null ? nodes.get(node.parent_id) : null
    if (parent) parent.children.push(node)
    else roots.push(node)
  })
  return roots
})

const allParents = computed(() =>
  categories.value.map((c) => ({ value: c.category_id, label: c.name }))
)

async function reload() {
  try {
    categories.value = await api.listCategories()
  } catch (e) {
    err.value = String(e)
  }
}

async function add() {
  err.value = ''
  const trimmed = name.value.trim()
  if (!trimmed) {
    err.value = '请输入分类名称'
    return
  }
  try {
    await api.createCategory(parentId.value, trimmed)
    name.value = ''
    parentId.value = null
    okMsg.value = '分类已创建'
    setTimeout(() => (okMsg.value = ''), TOAST_DURATION_MS)
    await reload()
  } catch (e) {
    err.value = String(e)
  }
}

onMounted(reload)
</script>

<template>
  <div class="page-body">
    <p class="muted small">按行业预置的分类模板在此维护；新增分类不影响已有商品。</p>

    <div class="card wide">
      <h3>分类结构</h3>
      <p v-if="err" class="error">{{ err }}</p>
      <p v-if="okMsg" class="ok-msg">{{ okMsg }}</p>

      <div v-if="tree.length" class="category-tree">
        <div v-for="root in tree" :key="root.category_id" class="category-branch">
          <div class="category-node root-node">{{ root.name }}</div>
          <div v-if="root.children.length" class="category-children">
            <span v-for="child in root.children" :key="child.category_id" class="category-node child-node">
              {{ child.name }}
            </span>
          </div>
        </div>
      </div>
      <p v-else class="empty">暂无分类</p>
    </div>

    <div class="card wide">
      <h3>新增分类</h3>
      <div class="form-grid">
        <label>分类名称
          <input v-model="name" class="input" placeholder="如 休闲食品" @keyup.enter="add" />
        </label>
        <label>上级分类
          <select v-model.number="parentId" class="input">
            <option :value="null">顶级分类</option>
            <option v-for="p in allParents" :key="p.value" :value="p.value">{{ p.label }}</option>
          </select>
        </label>
      </div>
      <div class="op-form">
        <button class="btn primary" @click="add"><Icon name="plus" :size="ICON_SIZE.md" />添加分类</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.category-tree { display: flex; flex-direction: column; gap: var(--space-3); }
.category-branch { display: flex; flex-direction: column; gap: var(--space-2); padding-left: var(--space-2); border-left: var(--border-width) solid var(--brand-border); }
.category-node { padding: var(--space-1) var(--space-3); border-radius: var(--radius-sm); width: fit-content; }
.root-node { background: var(--chrome-subtle); color: var(--chrome); font-weight: 600; }
.category-children { display: flex; flex-wrap: wrap; gap: var(--space-2); padding-left: var(--space-4); }
.child-node { background: var(--surface-subtle); color: var(--text); font-size: var(--font-sm); }
</style>
