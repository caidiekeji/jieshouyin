<script setup lang="ts">
/**
 * 场景看板通用骨架：容器 / 标题行 / 操作提示条 / 网格 / 空态。
 * 行业配色由宿主传入 .scene-* class（tokens.css 语义层，暗色只翻语义层）；
 * 卡片本体用全局 .scene-card 类（styles.css），行业密度用 minCard 设定。
 */
defineProps<{
  title: string
  /** 网格最小卡片宽：按行业密度设定（餐饮 140 / 包间 150 / 母婴 132 / 养生项目 210） */
  minCard?: number
  /** 列表为空时显示的引导文案 */
  emptyText?: string
  /** 顶部操作提示条（转台/并台等模式），非空时显示并附「取消」按钮 */
  notice?: string
}>()
const emit = defineEmits<{ cancel: [] }>()
</script>

<template>
  <section class="scene-board">
    <div class="scene-board-head">
      <div class="scene-board-title">
        <strong>{{ title }}</strong>
      </div>
      <slot name="action" />
    </div>
    <div v-if="notice" class="scene-notice">
      <span>{{ notice }}</span>
      <button class="link" @click="emit('cancel')">取消</button>
    </div>
    <div class="scene-grid" :style="minCard ? { '--scene-card-min': `${minCard}px` } : undefined">
      <slot />
      <div v-if="emptyText" class="scene-empty">{{ emptyText }}</div>
    </div>
  </section>
</template>

<style scoped>
/* 容器与网格：四个行业看板共用，行业色全部来自 --scene-* 语义 token */
.scene-board {
  border-bottom: var(--border-width) solid var(--scene-line);
  padding: var(--space-2) var(--space-3) var(--space-3);
  background: var(--scene-surface);
  color: var(--scene-title);
}
.scene-board-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: var(--space-2);
  margin-bottom: var(--space-2);
  flex-wrap: wrap;
}
.scene-board-title strong { color: var(--scene-title); margin-right: var(--space-2); }
/* 操作提示条（转台/并台）：餐饮用 hint 色、包间用 soft 色，token 缺省时逐级回退 */
.scene-notice {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: var(--space-2);
  border: var(--border-width) solid var(--scene-hint-line, var(--scene-soft-line));
  background: var(--scene-hint-bg, var(--scene-soft-bg));
  color: var(--scene-hint-text, var(--scene-soft-text));
  border-radius: var(--radius-xs);
  padding: var(--space-1) var(--space-3);
  margin-bottom: var(--space-2);
  font-size: var(--font-xs);
}
.scene-notice .link {
  border: 0; background: none; padding: 0;
  color: var(--scene-link); font-size: var(--font-2xs); cursor: pointer;
}
.scene-notice .link:hover { text-decoration: underline; }
/* 卡片网格：密度由宿主经 minCard 决定，滚动高度统一 */
.scene-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(var(--scene-card-min, 140px), 1fr));
  gap: var(--space-2);
  max-height: var(--scene-board-max-height);
  overflow: auto;
}
.scene-empty {
  grid-column: 1 / -1;
  color: var(--scene-sub);
  padding: var(--space-4);
  border: var(--border-width) dashed var(--scene-empty-line);
  border-radius: var(--radius-sm);
}
</style>
