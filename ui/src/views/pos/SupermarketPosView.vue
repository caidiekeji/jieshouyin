<script setup lang="ts">
/** 超市便利店收银页：扫码/快选为主，保留手工称重入口。 */
import { useAppContext } from '../../composables/useAppContext'
import PosCatalogPanel from '../../components/PosCatalogPanel.vue'
import PosCartPanel from '../../components/PosCartPanel.vue'

const { view } = useAppContext()
</script>

<template>
  <main v-if="view === 'pos'" class="pos-layout pos-layout--retail">
    <section class="panel product-panel">
      <div class="pos-page-head">
        <strong>超市选购</strong>
      </div>
      <PosCatalogPanel :weigh="true" />
    </section>
    <PosCartPanel />
  </main>
</template>
