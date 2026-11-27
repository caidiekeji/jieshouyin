<script setup lang="ts">
/**
 * 通用弹窗：遮罩 + 卡片 + 标题 + 内容 + actions 底部按钮行。
 * 宽度三档 token：--layout-modal-sm(440) / md(560) / lg(680)，默认 md。
 * 关闭只保留底部「取消/关闭」按钮与点遮罩两条路径：此前标题栏还有 X，
 * 与底部取消功能重复（三个关闭入口），且 X 的 .icon-ghost 全局无定义，
 * 落成了浏览器默认灰框方块；触屏收银场景底部按钮热区更大，故移除 X。
 * 关闭动作由父组件在 @close 里决定（可直接置 false 或走确认逻辑）。
 */
defineProps<{ title: string; open: boolean; size?: 'sm' | 'md' | 'lg' }>()
const emit = defineEmits<{ close: [] }>()
</script>

<template>
  <div v-if="open" class="modal-mask" @click.self="emit('close')">
    <div :class="['modal', size ? `modal--${size}` : 'modal--md']" role="dialog" aria-modal="true" :aria-label="title">
      <div class="modal-head">
        <h3>{{ title }}</h3>
      </div>
      <slot />
      <div v-if="$slots.actions" class="actions">
        <slot name="actions" />
      </div>
    </div>
  </div>
</template>
