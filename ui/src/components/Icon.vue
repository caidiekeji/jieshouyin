<script setup lang="ts">
import { computed } from 'vue'
import { ICON_SIZE } from '../constants'

/**
 * 自绘 SVG 图标，统一 24×24 网格、Lucide 风格线性笔画。
 * 每个图标由若干 path 组成，避免用 0.01 弧线等 hack 画圆，保证各尺寸下清晰一致。
 * 约束：不使用 emoji、不引入外部图标 JS、不使用渐变。
 */
type IconName =
  | 'scan' | 'search' | 'cart' | 'package' | 'box' | 'clipboard' | 'park'
  | 'heart' | 'users' | 'user' | 'undo' | 'receipt' | 'print' | 'report'
  | 'settings' | 'store' | 'sun' | 'shield' | 'sliders' | 'wallet' | 'cash' | 'star'
  | 'ticket' | 'pause' | 'image' | 'truck' | 'alert' | 'info'
  | 'plus' | 'minus' | 'check' | 'close' | 'refresh' | 'edit' | 'trash'
  | 'lock' | 'logout'

const props = defineProps<{
  name: IconName
  size?: number
}>()
const size = computed(() => props.size ?? ICON_SIZE.xl)

/** 每个图标拆成独立 path，便于单独校准笔画与圆角。 */
const ICONS: Record<IconName, string[]> = {
  // ── 扫码 / 搜索 ──
  scan: [
    'M3 7V5a2 2 0 0 1 2-2h2',
    'M17 3h2a2 2 0 0 1 2 2v2',
    'M21 17v2a2 2 0 0 1-2 2h-2',
    'M7 21H5a2 2 0 0 1-2-2v-2',
    'M3 12h18',
  ],
  search: [
    'M21 21l-4.35-4.35',
    'M11 19a8 8 0 1 0 0-16 8 8 0 0 0 0 16Z',
  ],

  // ── 收银 / 商品 / 采购 ──
  cart: [
    'M2 3h2.2l2.4 11.4a2 2 0 0 0 2 1.6h8.3a2 2 0 0 0 2-1.6L20.5 7H5.2',
    'M9.5 20a1.2 1.2 0 1 0 0-2.4 1.2 1.2 0 0 0 0 2.4Z',
    'M17.5 20a1.2 1.2 0 1 0 0-2.4 1.2 1.2 0 0 0 0 2.4Z',
  ],
  package: [
    'M12 22 2.5 17V7L12 2l9.5 5v10L12 22Z',
    'M2.5 7 12 12l9.5-5',
    'M12 12v10',
    'M7 4.5 17 9.5',
  ],
  box: [
    'M21 8 12 3 3 8v8l9 5 9-5V8Z',
    'M3 8l9 5 9-5',
    'M12 13v8',
  ],
  clipboard: [
    'M9 4h6v4H9z',
    'M8 4H6a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V6a2 2 0 0 0-2-2h-2',
    'M9 13h6',
    'M9 17h4',
  ],
  park: ['M9 4h6v4H9z', 'M8 4H6a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V6a2 2 0 0 0-2-2h-2', 'M9 13h6', 'M9 17h4'],

  // ── 会员 / 人员 ──
  heart: ['M20.8 5.6a5.4 5.4 0 0 0-7.6 0L12 6.8l-1.2-1.2a5.4 5.4 0 0 0-7.6 7.6L12 21.2l8.8-8a5.4 5.4 0 0 0 0-7.6Z'],
  users: [
    'M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2',
    'M9 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8Z',
    'M22 21v-2a4 4 0 0 0-3-3.87',
    'M16 3.13a4 4 0 0 1 0 7.75',
  ],
  user: [
    'M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2',
    'M12 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8Z',
  ],
  truck: [
    'M10 17h5V5H2v12h3',
    'M15 9h4l3 4v4h-3',
    'M7.5 17a2 2 0 1 0 0-4 2 2 0 0 0 0 4Z',
    'M17.5 17a2 2 0 1 0 0-4 2 2 0 0 0 0 4Z',
  ],

  // ── 单据 / 报表 ──
  undo: [
    'M9 14 4 9l5-5',
    'M4 9h10.5a5.5 5.5 0 0 1 0 11H11',
  ],
  receipt: [
    'M5 3h14v18l-2-1-2 1-2-1-2 1-2-1-2 1V3Z',
    'M9 8h6',
    'M9 12h6',
    'M9 16h4',
  ],
  print: [
    'M6 9V3h12v6',
    'M6 18H4a2 2 0 0 1-2-2v-5a2 2 0 0 1 2-2h16a2 2 0 0 1 2 2v5a2 2 0 0 1-2 2h-2',
    'M6 14h12v8H6z',
  ],
  report: [
    'M3 3v18h18',
    'M7 16v-5',
    'M12 16V7',
    'M17 16v-8',
  ],

  // ── 设置中心 ──
  settings: [
    'M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8Z',
    'M12 2v2',
    'M12 20v2',
    'M4.93 4.93l1.42 1.42',
    'M17.66 17.66l1.41 1.41',
    'M2 12h2',
    'M20 12h2',
    'M4.93 19.07l1.42-1.42',
    'M17.66 6.34l1.41-1.41',
  ],
  store: [
    'M3 10 5.5 4h13L21 10',
    'M3 10h18v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-9Z',
    'M9 21v-7h6v7',
  ],
  sun: [
    'M12 17a5 5 0 1 0 0-10 5 5 0 0 0 0 10Z',
    'M12 1v2',
    'M12 21v2',
    'M4.22 4.22l1.42 1.42',
    'M18.36 18.36l1.42 1.42',
    'M1 12h2',
    'M21 12h2',
    'M4.22 19.78l1.42-1.42',
    'M18.36 5.64l1.42-1.42',
  ],
  shield: [
    'M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10Z',
    'M9 12l2 2 4-4',
  ],
  sliders: [
    'M4 6h16',
    'M4 12h16',
    'M4 18h16',
    'M9 3v6',
    'M15 9v6',
    'M7 15v6',
  ],
  wallet: [
    'M3 8a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8Z',
    'M3 9h18',
    'M16 14h2',
  ],
  cash: [
    'M3 7h18v10H3z',
    'M3 12h18',
    'M7 12a2 2 0 1 0 0-4 2 2 0 0 0 0 4Z',
  ],
  star: ['M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2Z'],
  ticket: [
    'M4 5h16a1 1 0 0 1 1 1v3a3 3 0 0 0 0 6v3a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1v-3a3 3 0 0 0 0-6V6a1 1 0 0 1 1-1Z',
    'M13 5v2',
    'M13 11v2',
    'M13 17v2',
  ],
  pause: ['M6 4h4v16H6z', 'M14 4h4v16h-4z'],
  image: [
    'M3 3h18v18H3z',
    'M8.5 10a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3Z',
    'M21 15l-5-5L5 21',
  ],

  // ── 状态 / 通用操作 ──
  alert: [
    'M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0Z',
    'M12 9v4',
    'M12 17h.01',
  ],
  info: [
    'M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20Z',
    'M12 16v-4',
    'M12 8h.01',
  ],
  plus: ['M12 5v14', 'M5 12h14'],
  minus: ['M5 12h14'],
  check: ['M20 6 9 17l-5-5'],
  close: ['M18 6 6 18', 'M6 6l12 12'],
  refresh: [
    'M21 12a9 9 0 0 0-9-9 9 9 0 0 0-6.7 3L3 8',
    'M3 3v5h5',
    'M3 12a9 9 0 0 0 9 9 9 9 0 0 0 6.7-3L21 16',
    'M21 21v-5h-5',
  ],
  edit: [
    'M12 20h9',
    'M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4Z',
  ],
  trash: [
    'M3 6h18',
    'M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6',
    'M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2',
  ],
  lock: [
    'M5 11h14v10H5z',
    'M8 11V7a4 4 0 0 1 8 0v4',
    'M12 15v2',
  ],
  logout: [
    'M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4',
    'M16 17l5-5-5-5',
    'M21 12H9',
  ],
}

const paths = computed(() => ICONS[props.name] ?? [])
</script>

<template>
  <svg
    :width="size"
    :height="size"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="1.8"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
    focusable="false"
  >
    <path v-for="(d, i) in paths" :key="i" :d="d" />
  </svg>
</template>
