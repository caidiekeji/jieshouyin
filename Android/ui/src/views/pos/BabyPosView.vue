<script setup lang="ts">
/** 母婴用品收银页：品类卡片一屏直达，购物车共用组件。 */
import { useAppContext } from '../../composables/useAppContext'
import BabyScene from '../../components/BabyScene.vue'
import PosCartPanel from '../../components/PosCartPanel.vue'
import HintTip from '../../components/HintTip.vue'

const { view, addToCart, showSceneError } = useAppContext()
</script>

<template>
  <main v-if="view === 'pos'" class="pos-layout pos-layout--baby">
    <section class="panel product-panel">
      <div class="pos-page-head">
        <strong>母婴选购</strong>
        <HintTip text="奶粉、纸尿裤、辅食、洗护按品类卡片点选；可按月龄段筛选，卡片显示适用月龄与最早批次效期。批次效期仅作提示，不参与库存自动扣减。" />
      </div>
      <BabyScene @add="addToCart" @error="showSceneError" />
    </section>
    <PosCartPanel />
  </main>
</template>
