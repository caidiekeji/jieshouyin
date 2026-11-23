<script setup lang="ts">
// 复选框原子组件：与 Switch 同族的勾选控件。
// 语义区分：勾选是提交时生效的选择（Switch 是立即生效的动作），
// 视觉区分：方形勾选框 vs 圆形轨道开关。
const props = defineProps<{
  modelValue: boolean
  label?: string
  disabled?: boolean
}>()
const emit = defineEmits<{ 'update:modelValue': [value: boolean]; change: [value: boolean] }>()

function onChange(e: Event) {
  const checked = (e.target as HTMLInputElement).checked
  emit('update:modelValue', checked)
  emit('change', checked)
}
</script>

<template>
  <label class="checkbox" :class="{ disabled }">
    <input
      type="checkbox"
      class="checkbox-box"
      :checked="modelValue"
      :disabled="disabled"
      :aria-label="label"
      @change="onChange"
    />
    <span v-if="label" class="checkbox-text">{{ label }}</span>
  </label>
</template>

<style scoped>
/* 组件层尺度：与 Switch 一致由局部变量派生，改全站密度时随之调整 */
.checkbox {
  --cb-size: 18px;   /* 4px 基数 ×4 + 描边，触控可点 */
  --cb-check: 9px;   /* 对勾臂长：size 的约一半，留边呼吸 */
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  color: var(--text);
  cursor: pointer;
  font-size: var(--font-sm);
}
.checkbox.disabled { cursor: not-allowed; opacity: var(--opacity-disabled); }
.checkbox-box {
  appearance: none;
  width: var(--cb-size); height: var(--cb-size); flex: none;
  margin: 0;
  border: var(--border-width) solid var(--border-strong);
  border-radius: var(--radius-xs);
  background: var(--panel);
  cursor: pointer;
  display: inline-flex; align-items: center; justify-content: center;
  transition: background var(--duration-fast) var(--ease-out),
              border-color var(--duration-fast) var(--ease-out);
}
.checkbox-box:checked { background: var(--brand); border-color: var(--brand); }
/* 对勾：纯 CSS 双边框旋转，不为单符号引入图标依赖 */
.checkbox-box:checked::after {
  content: '';
  width: var(--cb-check);
  height: calc(var(--cb-check) / 2);
  border-left: 2px solid var(--text-on-brand);
  border-bottom: 2px solid var(--text-on-brand);
  transform: rotate(-45deg) translateY(-1px);
}
.checkbox-box:focus-visible { outline: 2px solid var(--brand); outline-offset: 2px; }
.checkbox.disabled .checkbox-box { cursor: not-allowed; }
</style>
