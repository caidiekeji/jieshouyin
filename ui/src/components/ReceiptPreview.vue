<script setup lang="ts">
/**
 * 小票所见即所得预览：用真实门店名 + 示例订单渲染成仿真小票纸面。
 * 模板 JSON 的自由编辑保留在设置页，这里只负责呈现最终效果（不实际打印）。
 */
import { computed } from 'vue'
import { formatMoney } from '../api'

const props = withDefaults(defineProps<{
  storeName?: string
  paperWidthMm?: number
}>(), { storeName: '', paperWidthMm: 58 })

/** 示例订单：金额与后端 preview_receipt 的示例值保持一致 */
const demo = {
  orderNo: 'STOREPOS0000000000000001',
  createdAt: '2026-11-23 21:30:00',
  tableCode: 'A01',
  memberPhone: '138****8888',
  items: [
    { name: '示例商品', qty: 1, amount: 1000 },
    { name: '示例配菜', qty: 1, amount: 200 },
  ],
  subtotal: 1200,
  discount: 200,
  payable: 1000,
  payChannel: '微信支付',
}

const shop = computed(() => props.storeName?.trim() || '示例门店')
/** 80mm ≈ 320px，58mm ≈ 232px，等宽字体下接近真实纸宽 */
const paperPx = computed(() => (props.paperWidthMm >= 80 ? 320 : 232))
</script>

<template>
  <div class="receipt-stage">
    <div class="receipt-paper" :style="{ width: paperPx + 'px' }">
      <div class="rp-shop">{{ shop }}</div>
      <div class="rp-meta">欢迎光临 · 谢谢惠顾</div>
      <div class="rp-divider">- - - - - - - - - - - - - - - - - -</div>
      <div class="rp-line"><span>单号</span><span class="rp-mono">{{ demo.orderNo }}</span></div>
      <div class="rp-line"><span>时间</span><span class="rp-mono">{{ demo.createdAt }}</span></div>
      <div class="rp-line"><span>桌台</span><span>{{ demo.tableCode }}</span></div>
      <div class="rp-line"><span>会员</span><span class="rp-mono">{{ demo.memberPhone }}</span></div>
      <div class="rp-divider">- - - - - - - - - - - - - - - - - -</div>
      <div class="rp-items-head"><span>商品</span><span>数量</span><span>金额</span></div>
      <div v-for="(it, i) in demo.items" :key="i" class="rp-line rp-item">
        <span>{{ it.name }}</span><span>x{{ it.qty }}</span><span>¥{{ formatMoney(it.amount) }}</span>
      </div>
      <div class="rp-divider">- - - - - - - - - - - - - - - - - -</div>
      <div class="rp-line"><span>小计</span><span>¥{{ formatMoney(demo.subtotal) }}</span></div>
      <div class="rp-line"><span>优惠</span><span>-¥{{ formatMoney(demo.discount) }}</span></div>
      <div class="rp-line rp-total"><span>实收</span><span>¥{{ formatMoney(demo.payable) }}</span></div>
      <div class="rp-line"><span>支付</span><span>{{ demo.payChannel }}</span></div>
      <div class="rp-divider">- - - - - - - - - - - - - - - - - -</div>
      <div class="rp-qr" role="img" aria-label="支付二维码示意">
        <svg viewBox="0 0 64 64" width="64" height="64" role="presentation" class="rp-qr-svg">
          <rect width="64" height="64" class="rp-qr-bg" />
          <g class="rp-qr-fg">
            <rect x="4" y="4" width="16" height="16" /><rect x="8" y="8" width="8" height="8" class="rp-qr-bg" />
            <rect x="44" y="4" width="16" height="16" /><rect x="48" y="8" width="8" height="8" class="rp-qr-bg" />
            <rect x="4" y="44" width="16" height="16" /><rect x="8" y="48" width="8" height="8" class="rp-qr-bg" />
            <rect x="26" y="4" width="3" height="16" /><rect x="34" y="4" width="3" height="16" />
            <rect x="4" y="26" width="16" height="3" /><rect x="4" y="34" width="16" height="3" />
            <rect x="26" y="26" width="6" height="6" /><rect x="36" y="26" width="4" height="6" />
            <rect x="44" y="26" width="16" height="3" /><rect x="44" y="34" width="16" height="3" />
            <rect x="26" y="44" width="3" height="16" /><rect x="34" y="44" width="3" height="16" />
            <rect x="26" y="40" width="6" height="4" /><rect x="44" y="44" width="6" height="6" />
            <rect x="54" y="44" width="6" height="6" /><rect x="44" y="54" width="6" height="6" />
          </g>
        </svg>
        <span class="rp-qr-tip">扫码支付</span>
      </div>
      <div class="rp-foot">—— 谢谢惠顾 ——</div>
    </div>
  </div>
</template>

<style scoped>
/* 小票预览：灰底舞台 + 白纸票据，字体/间距全部走 token；纸宽按纸张设置切换 */
.receipt-stage {
  display: flex; justify-content: center; align-items: flex-start;
  padding: var(--space-4); margin-top: var(--space-3);
  background: var(--surface-subtle);
  border: var(--border-width) solid var(--border);
  overflow-x: auto;
}
.receipt-paper {
  box-sizing: border-box; padding: var(--space-4) var(--space-3);
  background: var(--receipt-paper); color: var(--receipt-ink);
  border: var(--border-width) solid var(--border);
  box-shadow: var(--shadow-float);
  font-family: var(--font-family-mono); font-size: var(--font-xs); line-height: 1.7;
  /* 纸面上下锯齿：用重复线性渐变画撕裂口，纯装饰不参与布局 */
  --rp-tooth: 6px;
  background-image:
    linear-gradient(135deg, var(--surface-subtle) 50%, transparent 50%),
    linear-gradient(225deg, var(--surface-subtle) 50%, transparent 50%),
    linear-gradient(315deg, var(--surface-subtle) 50%, transparent 50%),
    linear-gradient(45deg, var(--surface-subtle) 50%, transparent 50%);
  background-position: 0 0, 0 0, 0 100%, 0 100%;
  background-size: var(--rp-tooth) var(--rp-tooth);
  background-repeat: repeat-x;
}
.rp-shop { text-align: center; font-size: var(--font-md); font-weight: 700; letter-spacing: var(--tracking-wide); }
.rp-meta { text-align: center; color: var(--receipt-subtle); font-size: var(--font-2xs); margin-bottom: var(--space-2); }
.rp-divider { color: var(--receipt-divider); white-space: nowrap; overflow: hidden; font-size: var(--font-2xs); margin: var(--space-1) 0; }
.rp-line { display: flex; justify-content: space-between; gap: var(--space-2); }
.rp-line > span:first-child { flex: none; }
.rp-mono { font-variant-numeric: tabular-nums; }
.rp-items-head { display: grid; grid-template-columns: 1fr auto 64px; gap: var(--space-2); color: var(--receipt-subtle); font-size: var(--font-2xs); }
.rp-item { display: grid; grid-template-columns: 1fr auto 64px; gap: var(--space-2); }
.rp-item > span:last-child, .rp-total > span:last-child { text-align: right; font-variant-numeric: tabular-nums; }
.rp-total { font-weight: 700; font-size: var(--font-sm); margin-top: var(--space-0_5); }
.rp-qr { display: flex; flex-direction: column; align-items: center; gap: var(--space-1); margin: var(--space-2) 0; }
/* 二维码是黑白示意图形，用纸面前景/背景色，暗色下小票仍是白纸 */
.rp-qr-bg { fill: var(--receipt-paper); }
.rp-qr-fg { fill: var(--receipt-ink); }
.rp-qr-tip { font-size: var(--font-2xs); color: var(--receipt-subtle); }
.rp-foot { text-align: center; font-size: var(--font-2xs); color: var(--receipt-subtle); margin-top: var(--space-1); }
</style>
