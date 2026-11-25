<script setup lang="ts">
/**
 * 收银入口只做行业路由：功能页面按行业拆分，互不复用。
 * 六个页面只复用商品区/购物车等组件，布局与行业语义各自独立。
 */
import { useAppContext } from '../composables/useAppContext'
import SupermarketPosView from './pos/SupermarketPosView.vue'
import ClothingPosView from './pos/ClothingPosView.vue'
import BabyPosView from './pos/BabyPosView.vue'
import RestaurantPosView from './pos/RestaurantPosView.vue'
import ChessPosView from './pos/ChessPosView.vue'
import WellnessPosView from './pos/WellnessPosView.vue'

const { view, industry } = useAppContext()
</script>

<template>
  <template v-if="view === 'pos'">
    <SupermarketPosView v-if="industry === 'SUPERMARKET'" />
    <ClothingPosView v-else-if="industry === 'CLOTHING'" />
    <BabyPosView v-else-if="industry === 'BABY'" />
    <RestaurantPosView v-else-if="industry === 'RESTAURANT'" />
    <ChessPosView v-else-if="industry === 'BOARD_GAME'" />
    <WellnessPosView v-else-if="industry === 'WELLNESS'" />
    <SupermarketPosView v-else />
  </template>
</template>
