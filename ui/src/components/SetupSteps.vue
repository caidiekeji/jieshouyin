<script setup lang="ts">
/**
 * 首次初始化进度：设置管理员 → 登录 → 选择行业 → 激活。
 * 只做步骤提示，不参与权限与路由判断，路由仍由 App.vue 的后端状态决定。
 */
const props = defineProps<{ step: number }>()

const STEPS = [
  { no: 1, label: '设置账号' },
  { no: 2, label: '登录' },
  { no: 3, label: '选择行业' },
  { no: 4, label: '激活' },
] as const
</script>

<template>
  <div class="setup-steps" aria-label="初始化进度">
    <div
      v-for="s in STEPS"
      :key="s.no"
      :class="['setup-step', { done: props.step > s.no, active: props.step === s.no }]"
      :aria-current="props.step === s.no ? 'step' : undefined"
    >
      <span class="setup-step-no">{{ s.no }}</span>
      <span class="setup-step-label">{{ s.label }}</span>
    </div>
  </div>
</template>
