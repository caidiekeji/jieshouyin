<script setup lang="ts">
/** 超市便利店收银页：扫码/快选为主，保留手工称重入口。 */
import { useAppContext } from '../../composables/useAppContext'
import PosCatalogPanel from '../../components/PosCatalogPanel.vue'
import PosCartPanel from '../../components/PosCartPanel.vue'
import HintTip from '../../components/HintTip.vue'

const { view } = useAppContext()
</script>

<template>
  <main v-if="view === 'pos'" class="pos-layout pos-layout--retail">
    <section class="panel product-panel">
      <div class="pos-page-head">
        <strong>超市选购</strong>
        <HintTip text="扫码或点选商品；称重商品可扫店内码，也可展开「手工称重」按店内码与克重计价。" />
      </div>
      <PosCatalogPanel :weigh="true" />
    </section>
    <PosCartPanel />
  </main>
</template>
