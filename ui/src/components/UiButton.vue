<script setup lang="ts">
import Icon from './Icon.vue'
import { ICON_SIZE } from '../constants'

/**
 * 通用按钮：variant 三态（primary 主行动 / ghost 次行动 / danger 破坏性），
 * size 三档高度 token（--btn-h-sm/md/lg），可选前置图标。
 * 未传 variant 时渲染中性次按钮（ghost）。
 */
const props = withDefaults(
  defineProps<{
    variant?: 'primary' | 'ghost' | 'danger'
    size?: 'sm' | 'md' | 'lg'
    icon?: InstanceType<typeof Icon>['$props']['name']
    disabled?: boolean
    type?: 'button' | 'submit'
  }>(),
  { variant: 'ghost', size: 'md', icon: undefined, disabled: false, type: 'button' },
)
</script>

<template>
  <button
    :class="['btn', 'btn--ui', `btn--${props.variant}`, `btn--${props.size}`]"
    :type="props.type"
    :disabled="props.disabled"
  >
    <Icon v-if="props.icon" :name="props.icon" :size="ICON_SIZE.sm" />
    <slot />
  </button>
</template>
