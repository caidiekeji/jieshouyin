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

type CategoryNode = Category & { children: CategoryNode[] }

/** 扁平分类按 parent_id 组装成树；孤儿节点（父不存在）归为顶层，避免丢失 */
const tree = computed<CategoryNode[]>(() => {
  const nodes = new Map<number, CategoryNode>()
  categories.value.forEach((c) => nodes.set(c.category_id, { ...c, children: [] }))
  const roots: CategoryNode[] = []
  nodes.forEach((node) => {
    const parent = node.parent_id != null ? nodes.get(node.parent_id) : null
    if (parent) parent.children.push(node)
    else roots.push(node)
  })
  return roots
})

/** 展平成带层级的行：分类树不限层数，第三层及以下也要能看见、改名、删除 */
const flatTree = computed(() => {
  const out: Array<{ node: CategoryNode; depth: number }> = []
  const walk = (list: CategoryNode[], depth: number) => {
    for (const node of list) {
      out.push({ node, depth })
      if (node.children.length) walk(node.children, depth + 1)
    }
  }
  walk(tree.value, 0)
  return out
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

async function rename(c: Category) {
  const next = window.prompt('新的分类名称', c.name)?.trim()
  if (!next || next === c.name) return
  try {
    await api.updateCategory(c.category_id, c.parent_id, next)
    okMsg.value = '分类已改名'
    setTimeout(() => (okMsg.value = ''), TOAST_DURATION_MS)
    await reload()
  } catch (e) {
    err.value = String(e)
  }
}

async function remove(c: Category) {
  if (!window.confirm(`删除分类「${c.name}」？分类下有商品或子分类时会被拒绝。`)) return
  try {
    await api.deleteCategory(c.category_id)
    okMsg.value = '分类已删除'
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

      <div v-if="flatTree.length" class="category-tree">
        <div
          v-for="row in flatTree"
          :key="row.node.category_id"
          class="category-row"
          :style="{ paddingLeft: `${row.depth * 20}px` }"
        >
          <span class="category-node" :class="row.depth === 0 ? 'root-node' : 'child-node'">
            {{ row.node.name }}
          </span>
          <button class="btn btn-sm ghost" @click="rename(row.node)">改名</button>
          <button class="btn btn-sm ghost danger-text" @click="remove(row.node)">删除</button>
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
.category-tree { display: flex; flex-direction: column; gap: var(--space-2); }
.category-row { display: flex; align-items: center; gap: var(--space-2); border-left: var(--border-width) solid var(--brand-border); }
.category-node { padding: var(--space-1) var(--space-3); border-radius: var(--radius-sm); width: fit-content; }
.root-node { background: var(--chrome-subtle); color: var(--chrome); font-weight: 600; }
.child-node { background: var(--surface-subtle); color: var(--text); font-size: var(--font-sm); }
</style>
