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
    <!-- label 仅作 aria-label 供读屏；语义标签由调用方在左侧渲染，避免重复 -->
  </button>
</template>

<style scoped>
/* 组件层尺度：从语义间距派生，改全站密度时随之调整 */
.switch {
  --sw-track-w: 40px;   /* 10 × 4px 基数，容纳 thumb + 两侧 3px 边距 + 滑动行程 */
  --sw-track-h: 22px;   /* 拇指 16px + 上下各 3px 呼吸，触控仍可点 */
  --sw-thumb: 16px;
  --sw-pad: 3px;        /* 半步 2px 略紧，取 3px 让拇指与轨道留一圈描边感 */
}
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
  width: var(--sw-track-w);
  height: var(--sw-track-h);
  border-radius: var(--radius-pill);
  background: var(--border-strong);
  transition: background var(--duration-fast) var(--ease-out);
  flex: none;
}
.switch-thumb {
  position: absolute;
  top: var(--sw-pad);
  left: var(--sw-pad);
  width: var(--sw-thumb);
  height: var(--sw-thumb);
  border-radius: var(--radius-circle);
  background: var(--on-chrome);
  box-shadow: var(--shadow);
  transition: transform var(--duration-fast) var(--ease-out);
}
.switch.on .switch-track { background: var(--brand); }
/* 位移 = 轨宽 − 拇指 − 两侧边距，改尺寸时自动跟随 */
.switch.on .switch-thumb { transform: translateX(calc(var(--sw-track-w) - var(--sw-thumb) - var(--sw-pad) * 2)); }
.switch:focus-visible { outline: 2px solid var(--brand); outline-offset: 2px; }
</style>
