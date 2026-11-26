<script setup lang="ts">
/** 服装鞋帽收银页：款×色×码 SKU 规格筛选 + 分类点选 + 条码检索。 */
import { useAppContext } from '../../composables/useAppContext'
import PosCatalogPanel from '../../components/PosCatalogPanel.vue'
import PosCartPanel from '../../components/PosCartPanel.vue'

const { view } = useAppContext()
</script>

<template>
  <main v-if="view === 'pos'" class="pos-layout pos-layout--apparel">
    <section class="panel product-panel">
      <div class="pos-page-head">
        <strong>服装选购</strong>
      </div>
      <PosCatalogPanel spec-filter />
    </section>
    <PosCartPanel />
  </main>
</template>
