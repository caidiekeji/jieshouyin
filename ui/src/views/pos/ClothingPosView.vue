<script setup lang="ts">
/** 服装鞋帽收银页：款×色×码 SKU 规格筛选 + 分类点选 + 条码检索。 */
import { useAppContext } from '../../composables/useAppContext'
import PosCatalogPanel from '../../components/PosCatalogPanel.vue'
import PosCartPanel from '../../components/PosCartPanel.vue'
import HintTip from '../../components/HintTip.vue'

const { view } = useAppContext()
</script>

<template>
  <main v-if="view === 'pos'" class="pos-layout pos-layout--apparel">
    <section class="panel product-panel">
      <div class="pos-page-head">
        <strong>服装选购</strong>
        <HintTip text="按分类点选或扫码/款号检索；维护了款色码规格的商品会显示「颜色:黑 / 尺码:M」标签，点击标签筛选对应 SKU，下单以所选 SKU 价格为准。" />
      </div>
      <PosCatalogPanel spec-filter />
    </section>
    <PosCartPanel />
  </main>
</template>
