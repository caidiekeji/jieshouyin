<script setup lang="ts">
import Icon from './Icon.vue'
import { ICON_SIZE } from '../constants'

/**
 * 通用弹窗：遮罩 + 卡片 + 标题行（含关闭按钮）+ 内容 + actions 底部按钮行。
 * 宽度三档 token：--layout-modal-sm(440) / md(560) / lg(680)，默认 md。
 * 交互：点击遮罩关闭；关闭动作由父组件在 @close 里决定（可直接置 false 或走确认逻辑）。
 */
defineProps<{ title: string; open: boolean; size?: 'sm' | 'md' | 'lg' }>()
const emit = defineEmits<{ close: [] }>()
</script>

<template>
  <div v-if="open" class="modal-mask" @click.self="emit('close')">
    <div :class="['modal', size ? `modal--${size}` : 'modal--md']" role="dialog" aria-modal="true" :aria-label="title">
      <div class="modal-head">
        <h3>{{ title }}</h3>
        <button class="icon-ghost" aria-label="关闭" @click="emit('close')">
          <Icon name="close" :size="ICON_SIZE.sm" />
        </button>
      </div>
      <slot />
      <div v-if="$slots.actions" class="actions">
        <slot name="actions" />
      </div>
    </div>
  </div>
</template>
