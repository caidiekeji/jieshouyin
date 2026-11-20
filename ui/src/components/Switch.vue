<script setup lang="ts">
// 开关原子组件：勾选是提交时生效的选择，开关是立即生效的动作。
// 用语义化 role=switch + aria-checked，自绘无 emoji。
const props = defineProps<{
  modelValue: boolean
  label?: string
  disabled?: boolean
}>()
const emit = defineEmits<{ 'update:modelValue': [value: boolean] }>()

function toggle() {
  if (props.disabled) return
  emit('update:modelValue', !props.modelValue)
}
</script>

<template>
  <button
    type="button"
    class="switch"
    :class="{ on: modelValue, disabled }"
    role="switch"
    :aria-checked="modelValue"
    :aria-label="label"
    :disabled="disabled"
    @click="toggle"
  >
    <span class="switch-track"><span class="switch-thumb" /></span>
    <span v-if="label" class="switch-label">{{ modelValue ? '已启用' : '已停用' }}</span>
  </button>
</template>

<style scoped>
.switch {
  display: inline-flex;
  align-items: center;
  gap: var(--space-2);
  border: none;
  background: transparent;
  color: var(--text);
  cursor: pointer;
  padding: var(--space-0_5) var(--space-0);
}
.switch.disabled { cursor: not-allowed; opacity: var(--opacity-disabled); }
.switch-track {
  position: relative;
  width: 40px;
  height: 22px;
  border-radius: var(--radius-pill);
  background: var(--border-strong);
  transition: background var(--duration-fast) var(--ease-out);
  flex: none;
}
.switch-thumb {
  position: absolute;
  top: 3px;
  left: 3px;
  width: 16px;
  height: 16px;
  border-radius: var(--radius-circle);
  background: var(--p-ink-0);
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.25);
  transition: transform var(--duration-fast) var(--ease-out);
}
.switch.on .switch-track { background: var(--brand); }
.switch.on .switch-thumb { transform: translateX(18px); }
.switch-label { font-size: var(--font-xs); white-space: nowrap; }
</style>
