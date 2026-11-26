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

// ===== 原位编辑：所有命名输入都在布局原位完成，不弹窗（触屏同样原位，软键盘直接唤起）=====
/** 改名中的分类（id + 当前草稿） */
const editing = ref<{ id: number; value: string } | null>(null)
/** 新建中的分类（parentId=null 为一级；value 为草稿） */
const creating = ref<{ parentId: number | null; value: string } | null>(null)

/** 原位输入自动聚焦（v-for 内 ref 会数组化，用指令最稳） */
const vFocus = { mounted: (el: HTMLInputElement) => el.focus() }

function startEdit(node: Category) {
  creating.value = null
  editing.value = { id: node.category_id, value: node.name }
}
function startCreate(parentId: number | null) {
  editing.value = null
  creating.value = { parentId, value: '' }
}
function cancelEdit() {
  editing.value = null
  creating.value = null
}
/** 回车与失焦都会触发；提交后 editing/creating 置空，guard 防止重复提交 */
async function commitEdit() {
  if (editing.value) {
    const { id, value } = editing.value
    const node = categories.value.find((c) => c.category_id === id)
    editing.value = null
    const next = value.trim()
    if (!node || !next || next === node.name) return
    try {
      await api.updateCategory(id, node.parent_id, next)
      flash('分类已改名')
      await reload()
    } catch (e) {
      err.value = String(e)
    }
    return
  }
  if (creating.value) {
    const { parentId, value } = creating.value
    creating.value = null
    const next = value.trim()
    if (!next) return
    try {
      await api.createCategory(parentId, next)
      flash(parentId == null ? '分类已创建' : '子分类已创建')
      await reload()
    } catch (e) {
      err.value = String(e)
    }
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
        <button class="btn primary" @click="startCreate(null)">
          <Icon name="plus" :size="ICON_SIZE.sm" />新建分类
        </button>
      </div>
      <p v-if="err" class="error">{{ err }}</p>
      <p v-if="okMsg" class="ok-msg">{{ okMsg }}</p>

      <!-- 新建中的一级分类：原位虚线区块输入行 -->
      <div v-if="creating && creating.parentId === null" class="cat-block cat-block--creating">
        <input
          v-focus
          v-model="creating!.value"
          class="cat-edit cat-edit--title"
          placeholder="输入一级分类名称，回车确认"
          maxlength="30"
          @keyup.enter="commitEdit"
          @keyup.esc="cancelEdit"
          @blur="commitEdit"
        />
      </div>

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

      <!-- 一级分类区块：纵向排列，区块间浅灰细线分隔；触屏无 hover，操作图标常显 -->
      <div v-for="root in tree" :key="root.category_id" class="cat-block">
        <div class="cat-block-head">
          <!-- 标题原位改名 -->
          <input
            v-focus
            v-if="editing?.id === root.category_id"
            v-model="editing.value"
            class="cat-edit cat-edit--title"
            @keyup.enter="commitEdit"
            @keyup.esc="cancelEdit"
            @blur="commitEdit"
          />
          <strong v-else>{{ root.name }}</strong>
          <span v-if="editing?.id !== root.category_id" class="cat-ops">
            <button class="icon-ghost" aria-label="改名" @click="startEdit(root)"><Icon name="edit" :size="ICON_SIZE.sm" /></button>
            <button class="icon-ghost cat-danger" aria-label="删除" @click="remove(root)"><Icon name="trash" :size="ICON_SIZE.sm" /></button>
          </span>
        </div>
        <div class="cat-tags">
          <template v-for="t in tagsOf(root)" :key="t.node.category_id">
            <!-- 标签原位改名：输入态即标签本体变形 -->
            <input
              v-focus
              v-if="editing?.id === t.node.category_id"
              v-model="editing.value"
              class="cat-edit cat-edit--tag"
              @keyup.enter="commitEdit"
              @keyup.esc="cancelEdit"
              @blur="commitEdit"
            />
            <span v-else class="cat-tag">
              {{ t.label }}
              <button class="icon-ghost cat-op" aria-label="改名" @click="startEdit(t.node)"><Icon name="edit" :size="ICON_SIZE.sm" /></button>
              <button class="icon-ghost cat-op cat-danger" aria-label="删除" @click="remove(t.node)"><Icon name="trash" :size="ICON_SIZE.sm" /></button>
            </span>
          </template>
          <!-- 添加子分类：标签群尾原位虚线输入 -->
          <input
            v-focus
            v-if="creating?.parentId === root.category_id"
            v-model="creating.value"
            class="cat-edit cat-edit--tag cat-edit--new"
            placeholder="子分类名，回车确认"
            maxlength="30"
            @keyup.enter="commitEdit"
            @keyup.esc="cancelEdit"
            @blur="commitEdit"
          />
          <button v-else class="cat-add" @click="startCreate(root.category_id)">+ 添加</button>
        </div>
      </div>

      <p v-if="!tree.length && creating?.parentId !== null" class="empty">还没有自建分类，点右上角「新建分类」开始</p>
    </div>
  </div>
</template>

<style scoped>
/* 区块卡片 + 标签群平铺；全宽跟随内容区；触屏分叉：操作常显 + 48px 热区 + 原位输入 48px 高 */
.cat-head { display: flex; align-items: center; justify-content: space-between; gap: var(--space-3); margin-bottom: var(--space-4); }
.cat-head h3 { margin: 0; }
.cat-head .btn { min-height: var(--touch-target-min); }
.cat-block { padding: var(--space-4) 0; border-bottom: var(--border-width) solid var(--border); }
.cat-block:last-of-type { border-bottom: none; }
.cat-block--creating { border-bottom-style: dashed; }
.cat-block-head { display: flex; align-items: center; gap: var(--space-2); margin-bottom: var(--space-2); }
.cat-block-head strong { font-size: var(--font-md); }
.cat-block-head .muted { margin-left: auto; }
.cat-ops { display: inline-flex; gap: var(--space-1); }
.cat-tags { display: flex; flex-wrap: wrap; gap: var(--space-2); align-items: center; }
.cat-tag {
  display: inline-flex; align-items: center; gap: var(--space-1);
  min-height: var(--touch-target-min);
  padding: var(--space-1) var(--space-2);
  border-radius: var(--radius-sm);
  background: var(--surface-subtle); color: var(--text);
  font-size: var(--font-sm);
}
.cat-tag:active { background: var(--chrome-subtle); }
.cat-tag--ghost {
  background: transparent; color: var(--text-muted);
  border: var(--border-width) dashed var(--border);
}
.cat-add {
  min-height: var(--touch-target-min);
  padding: var(--space-1) var(--space-3);
  border: var(--border-width) dashed var(--brand-border);
  border-radius: var(--radius-sm);
  background: transparent; color: var(--brand);
  font-size: var(--font-sm);
}
.cat-add:active { background: var(--brand-subtle); border-color: var(--brand); }
/* 原位编辑输入：触屏 48px 热区 */
.cat-edit {
  min-height: var(--touch-target-min);
  border: var(--border-width) dashed var(--brand-border);
  border-radius: var(--radius-sm);
  background: var(--panel); color: var(--text);
  padding: var(--space-1) var(--space-2);
  font-size: var(--font-md);
}
.cat-edit:focus { outline: none; border-color: var(--brand); border-style: solid; }
.cat-edit--title { width: min(320px, 100%); font-weight: 600; }
.cat-edit--tag { width: 180px; font-size: var(--font-sm); }
.cat-edit--new { border-style: dashed; }
.icon-ghost {
  display: inline-flex; align-items: center; justify-content: center;
  min-width: var(--touch-target-min); min-height: var(--touch-target-min);
  border: none; background: transparent; color: var(--text-muted);
  border-radius: var(--radius-xs);
}
.icon-ghost.cat-danger { color: var(--danger); }
</style>
