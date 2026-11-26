<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import Icon from './Icon.vue'
import { ICON_SIZE } from '../constants'
import { api, type Category } from '../api'
import { TOAST_DURATION_MS } from '../constants'

const categories = ref<Category[]>([])
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

/** 区块标签群：直接子级平铺；孙级及以下以「父/子」路径并入同一标签群，保证深层分类可见可改名 */
interface TagItem { label: string; node: CategoryNode }
function tagsOf(root: CategoryNode): TagItem[] {
  const out: TagItem[] = []
  const walk = (list: CategoryNode[], prefix: string) => {
    for (const node of list) {
      out.push({ label: prefix ? `${prefix}/${node.name}` : node.name, node })
      if (node.children.length) walk(node.children, node.name)
    }
  }
  walk(root.children, '')
  return out
}

async function reload() {
  try {
    categories.value = await api.listCategories()
  } catch (e) {
    err.value = String(e)
  }
}

function flash(msg: string) {
  okMsg.value = msg
  setTimeout(() => (okMsg.value = ''), TOAST_DURATION_MS)
}

/** 新建一级分类：轻输入用系统 prompt，避免为单字段建弹窗 */
async function createRoot() {
  const next = window.prompt('分类名称')?.trim()
  if (!next) return
  try {
    await api.createCategory(null, next)
    flash('分类已创建')
    await reload()
  } catch (e) {
    err.value = String(e)
  }
}

/** 在一级分类下添加子分类 */
async function addChild(root: CategoryNode) {
  const next = window.prompt('子分类名称')?.trim()
  if (!next) return
  try {
    await api.createCategory(root.category_id, next)
    flash('子分类已创建')
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
    flash('分类已改名')
    await reload()
  } catch (e) {
    err.value = String(e)
  }
}

async function remove(c: Category) {
  if (!window.confirm('确认删除该分类吗？')) return
  try {
    await api.deleteCategory(c.category_id)
    flash('分类已删除')
    await reload()
  } catch (e) {
    err.value = String(e)
  }
}

onMounted(reload)
</script>

<template>
  <div class="page-body">
    <div class="card wide cat-board">
      <div class="cat-head">
        <h3>分类结构</h3>
        <button class="btn primary" @click="createRoot">
          <Icon name="plus" :size="ICON_SIZE.sm" />新建分类
        </button>
      </div>
      <p v-if="err" class="error">{{ err }}</p>
      <p v-if="okMsg" class="ok-msg">{{ okMsg }}</p>

      <!-- 未分类：系统保留区块，锁定不可改删，标签用透明虚线特殊样式 -->
      <div class="cat-block cat-block--locked">
        <div class="cat-block-head">
          <strong>未分类</strong>
          <span class="muted small">系统默认，不可修改</span>
        </div>
        <div class="cat-tags">
          <span class="cat-tag cat-tag--ghost">未归类商品</span>
        </div>
      </div>

      <!-- 一级分类区块：纵向排列，区块间浅灰细线分隔 -->
      <div v-for="root in tree" :key="root.category_id" class="cat-block">
        <div class="cat-block-head">
          <strong>{{ root.name }}</strong>
          <span class="cat-ops">
            <button class="icon-ghost" aria-label="改名" @click="rename(root)"><Icon name="edit" :size="ICON_SIZE.sm" /></button>
            <button class="icon-ghost cat-danger" aria-label="删除" @click="remove(root)"><Icon name="trash" :size="ICON_SIZE.sm" /></button>
          </span>
        </div>
        <div class="cat-tags">
          <span v-for="t in tagsOf(root)" :key="t.node.category_id" class="cat-tag">
            {{ t.label }}
            <button class="icon-ghost cat-op" aria-label="改名" @click="rename(t.node)"><Icon name="edit" :size="ICON_SIZE.xs" /></button>
            <button class="icon-ghost cat-op cat-danger" aria-label="删除" @click="remove(t.node)"><Icon name="trash" :size="ICON_SIZE.xs" /></button>
          </span>
          <button class="cat-add" @click="addChild(root)">+ 添加</button>
        </div>
      </div>

      <p v-if="!tree.length" class="empty">还没有自建分类，点右上角「新建分类」开始</p>
    </div>
  </div>
</template>

<style scoped>
/* 区块卡片 + 标签群平铺（用户规格映射：680px 容器 / 16px 区块标题 / 13px 标签 / 6px 圆角 / 主题橙） */
.cat-board { max-width: 680px; }
.cat-head { display: flex; align-items: center; justify-content: space-between; gap: var(--space-3); margin-bottom: var(--space-4); }
.cat-head h3 { margin: 0; }
.cat-block { padding: var(--space-4) 0; border-bottom: var(--border-width) solid var(--border); }
.cat-block:last-of-type { border-bottom: none; }
.cat-block-head { display: flex; align-items: center; gap: var(--space-3); margin-bottom: var(--space-2); }
.cat-block-head strong { font-size: var(--font-md); }
.cat-block-head .muted { margin-left: auto; }
.cat-ops { display: inline-flex; gap: var(--space-1); opacity: 0; transition: opacity 120ms ease; }
.cat-block:hover .cat-ops, .cat-block:focus-within .cat-ops { opacity: 1; }
.cat-tags { display: flex; flex-wrap: wrap; gap: var(--space-2); align-items: center; }
.cat-tag {
  display: inline-flex; align-items: center; gap: var(--space-0_5);
  padding: var(--space-1) var(--space-3);
  border-radius: var(--radius-sm);
  background: var(--surface-subtle); color: var(--text);
  font-size: var(--font-sm);
  transition: background 120ms ease;
}
.cat-tag:hover { background: var(--chrome-subtle); }
.cat-tag .cat-op { opacity: 0; transition: opacity 120ms ease; }
.cat-tag:hover .cat-op, .cat-tag:focus-within .cat-op { opacity: 1; }
.cat-tag--ghost {
  background: transparent; color: var(--text-muted);
  border: var(--border-width) dashed var(--border);
}
.cat-add {
  padding: var(--space-1) var(--space-3);
  border: var(--border-width) dashed var(--brand-border);
  border-radius: var(--radius-sm);
  background: transparent; color: var(--brand);
  font-size: var(--font-sm);
}
.cat-add:hover { background: var(--brand-subtle); border-color: var(--brand); }
.icon-ghost {
  display: inline-flex; align-items: center; justify-content: center;
  border: none; background: transparent; color: var(--text-muted);
  padding: var(--space-1); border-radius: var(--radius-xs);
}
.icon-ghost:hover { color: var(--brand); background: var(--brand-subtle); }
.icon-ghost.cat-danger:hover { color: var(--danger); background: var(--danger-subtle); }
</style>
