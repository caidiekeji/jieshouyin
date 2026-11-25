<script setup lang="ts">
/** 母婴用品收银页：品类卡片一屏直达，购物车共用组件。 */
import { useAppContext } from '../../composables/useAppContext'
import BabyScene from '../../components/BabyScene.vue'
import PosCartPanel from '../../components/PosCartPanel.vue'

const { view, addToCart, showSceneError } = useAppContext()
</script>

<template>
  <main v-if="view === 'pos'" class="pos-layout pos-layout--baby">
    <section class="panel product-panel">
      <div class="pos-page-head">
        <strong>母婴选购</strong>
      </div>
      <BabyScene @add="addToCart" @error="showSceneError" />
    </section>
    <PosCartPanel />
  </main>
</template>
