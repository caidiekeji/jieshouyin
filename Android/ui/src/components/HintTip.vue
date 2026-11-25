<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'
import { ICON_SIZE } from '../constants'
import Icon from './Icon.vue'

withDefaults(defineProps<{ text: string; label?: string }>(), { label: '查看说明' })
const open = ref(false)
function close() { open.value = false }
function onKey(e: KeyboardEvent) { if (e.key === 'Escape') close() }
onMounted(() => {
  document.addEventListener('click', close)
  document.addEventListener('keydown', onKey)
})
onUnmounted(() => {
  document.removeEventListener('click', close)
  document.removeEventListener('keydown', onKey)
})
</script>

<template>
  <span class="hint-tip">
    <button
      type="button"
      class="hint-tip-btn"
      :aria-label="label"
      :aria-expanded="open"
      @click.stop="open = !open"
    ><Icon name="info" :size="ICON_SIZE.sm" /></button>
    <span v-if="open" class="hint-tip-pop" role="tooltip" @click.stop>{{ text }}</span>
  </span>
</template>

<style scoped>
.hint-tip { position: relative; display: inline-flex; align-items: center; vertical-align: middle; }
.hint-tip-btn {
  display: inline-flex; align-items: center; justify-content: center;
  padding: var(--space-0_5); border: 0; border-radius: var(--radius-xs);
  background: transparent; color: var(--text-muted); cursor: pointer;
}
.hint-tip-btn:hover, .hint-tip-btn[aria-expanded='true'] { color: var(--brand); background: var(--brand-subtle); }
.hint-tip-pop {
  position: absolute; top: calc(100% + var(--space-1)); left: 0; z-index: var(--z-raised);
  width: max-content; max-width: 260px; padding: var(--space-2) var(--space-3);
  border: var(--border-width) solid var(--border); border-radius: var(--radius-xs);
  background: var(--panel); color: var(--text); box-shadow: var(--shadow-float);
  font-size: var(--font-xs); line-height: var(--line-normal); white-space: normal;
}
</style>
