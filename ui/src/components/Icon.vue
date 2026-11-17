<script setup lang="ts">
// 自绘 SVG 图标（Lucide 风格 path），不使用 emoji，符合界面硬约束。
const props = defineProps<{
  name: 'scan' | 'cart' | 'cash' | 'park' | 'report' | 'settings' | 'user' | 'print' | 'close' | 'check' | 'plus' | 'minus' | 'refresh'
  size?: number
}>()
const size = props.size ?? 20

const paths: Record<string, string> = {
  scan: 'M3 7V5a2 2 0 0 1 2-2h2M17 3h2a2 2 0 0 1 2 2v2M21 17v2a2 2 0 0 1-2 2h-2M7 21H5a2 2 0 0 1-2-2v-2M3 12h18',
  cart: 'M6 6h15l-1.5 9h-12L6 6Z M9 20a1 1 0 1 0 0-2 1 1 0 0 0 0 2Z M18 20a1 1 0 1 0 0-2 1 1 0 0 0 0 2Z M6 6 5 3H2',
  cash: 'M2 7h20v12H2z M2 12h20 M6 12a3 3 0 1 0 0 0.01',
  park: 'M4 4h16v4H4z M4 12h16v8H4z M9 16h6',
  report: 'M4 20V10 M10 20V4 M16 20v-7 M22 20H2',
  settings: 'M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z M19.4 15a1.7 1.7 0 0 0 .34 1.88l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.7 1.7 0 0 0-1.88-.34 1.7 1.7 0 0 0-1 1.56V21a2 2 0 1 1-4 0v-.07a1.7 1.7 0 0 0-1.1-1.56 1.7 1.7 0 0 0-1.88.34l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.7 1.7 0 0 0 4.6 15a1.7 1.7 0 0 0-1.56-1H3a2 2 0 1 1 0-4h.07A1.7 1.7 0 0 0 4.6 9a1.7 1.7 0 0 0-.34-1.88l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.7 1.7 0 0 0 9 4.6h.02A1.7 1.7 0 0 0 10 3.07V3a2 2 0 1 1 4 0v.07a1.7 1.7 0 0 0 1 1.56 1.7 1.7 0 0 0 1.88-.34l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06A1.7 1.7 0 0 0 19.4 9v.02a1.7 1.7 0 0 0 1.56 1H21a2 2 0 1 1 0 4h-.07a1.7 1.7 0 0 0-1.53 1Z',
  user: 'M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2 M12 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8Z',
  print: 'M6 9V2h12v7 M6 18H4a2 2 0 0 1-2-2v-5a2 2 0 0 1 2-2h16a2 2 0 0 1 2 2v5a2 2 0 0 1-2 2h-2 M6 14h12v8H6z',
  close: 'M18 6 6 18 M6 6l12 12',
  check: 'M20 6 9 17l-5-5',
  plus: 'M12 5v14 M5 12h14',
  minus: 'M5 12h14',
  refresh: 'M23 4v6h-6 M1 20v-6h6 M3.5 9a9 9 0 0 1 14.9-3.4L23 10 M1 14l4.6 4.4A9 9 0 0 0 20.5 15',
}
</script>

<template>
  <svg
    :width="size"
    :height="size"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="2"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
  >
    <path :d="paths[props.name]" />
  </svg>
</template>
