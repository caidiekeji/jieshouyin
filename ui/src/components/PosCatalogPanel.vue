<script setup lang="ts">
/**
 * 收银商品区组件（可复用）：扫码输入、搜索候选、分类点选。
 * 页面按行业拆分后，各收银页只决定布局与行业专属区块，不重复实现这套交互。
 */
import { computed, ref } from 'vue'
import { type ProductHit } from '../api'
import { useAppContext } from '../composables/useAppContext'
import { ICON_SIZE } from '../constants'
import Icon from './Icon.vue'
import WeighEntryPanel from './WeighEntryPanel.vue'

const props = defineProps<{
  /** 超市展示手工称重入口；其他行业不出现，避免误按件计价 */
  weigh?: boolean
  /** 服装启用款色码规格筛选；规格来自 SKU 快照，选中后按 SKU 下单 */
  specFilter?: boolean
}>()

/** 从 SKU 规格快照提取可选标签，避免为筛选再建一套前端规格字典 */
const specOptions = computed(() => {
  if (!props.specFilter) return [] as string[]
  const values = new Set<string>()
  for (const p of [...products.value, ...posProducts.value]) {
    for (const part of (p.spec_text || '').split(' / ')) {
      if (part.trim()) values.add(part.trim())
    }
  }
  return [...values].sort()
})
const activeSpec = ref('')

function visibleProduct(p: ProductHit): boolean {
  if (!props.specFilter || !activeSpec.value) return true
  return (p.spec_text || '').split(' / ').includes(activeSpec.value)
}

const {
  ICON_SIZE: ctxIconSize,
  keyword,
  products,
  posRootCategories,
  posProducts,
  posCategoryId,
  posCatalogBusy,
  searchBusy,
  sceneCatalogOnly,
  scanCapture,
  addFromCatalog,
  addToCart,
  focusScan,
  formatMoney,
  memberPriceOf,
  onScanKeydown,
  productImageSrc,
  selectPosCategory,
} = useAppContext()

const iconSize = ctxIconSize ?? ICON_SIZE
</script>

<template>
  <div class="pos-catalog-wrap">
    <div class="search-row scan-search">
      <input
        ref="scanCapture"
        v-model="keyword"
        class="input scan-input"
        placeholder="扫码 / 条码 / 商品名，自动联想"
        @keydown="onScanKeydown"
        @focus="focusScan"
      />
      <span v-if="searchBusy" class="scan-spinner" role="status" aria-label="搜索中"></span>
      <span v-else-if="products.length" class="scan-hint">{{ products.length }} 个候选 · 回车收第一条</span>
    </div>

    <WeighEntryPanel v-if="weigh" />

    <div v-if="specFilter && specOptions.length" class="pos-quick-row" aria-label="规格筛选">
      <span class="pos-quick-label">规格</span>
      <button
        v-for="s in specOptions"
        :key="s"
        type="button"
        :class="['pos-quick-chip', { active: activeSpec === s }]"
        @click="activeSpec = activeSpec === s ? '' : s"
      >{{ s }}</button>
      <button v-if="activeSpec" type="button" class="pos-quick-chip" @click="activeSpec = ''">全部</button>
    </div>

    <div v-if="keyword.trim()" class="product-list">
      <button v-for="p in products.filter(visibleProduct)" :key="p.sku_id" type="button" class="product-item" @click="addToCart(p)">
        <img v-if="productImageSrc(p)" :src="productImageSrc(p)!" class="product-thumb" alt="" />
        <span v-else class="product-thumb product-thumb-empty"></span>
        <span class="product-info">
          <strong>{{ p.spu_name }}</strong>
          <span class="muted small"> {{ p.barcode }}</span>
        </span>
        <span class="price-stack">
          <span class="price">¥{{ formatMoney(p.price_cents) }}</span>
          <span v-if="memberPriceOf(p)" class="price member">会员 ¥{{ formatMoney(memberPriceOf(p)!) }}</span>
        </span>
      </button>
      <div v-if="!products.length" class="empty">
        {{ searchBusy ? '正在查询…' : '没有匹配的商品' }}
      </div>
    </div>

    <div v-else-if="!sceneCatalogOnly" class="pos-catalog">
      <div v-if="posRootCategories.length" class="pos-cat-row">
        <button
          v-for="c in posRootCategories"
          :key="c.category_id"
          :class="['pos-cat', { active: posCategoryId === c.category_id }]"
          @click="selectPosCategory(c.category_id)"
        >{{ c.name }}</button>
      </div>
      <div class="pos-grid">
        <button
          v-for="p in posProducts.filter(visibleProduct)"
          :key="p.sku_id"
          type="button"
          class="pos-card"
          :class="{ weighed: p.is_weighed }"
          @click="addFromCatalog(p)"
        >
          <img v-if="productImageSrc(p)" :src="productImageSrc(p)!" class="pos-card-img" alt="" />
          <span v-else class="pos-card-img pos-card-empty"><Icon name="package" :size="iconSize.md" /></span>
          <span class="pos-card-body">
            <span class="pos-card-name" :title="p.spu_name">{{ p.spu_name }}</span>
            <span class="pos-card-meta">
              {{ p.spec_text || p.category_name }}<template v-if="p.is_weighed"> · 称重</template>
            </span>
            <strong class="price">¥{{ formatMoney(p.price_cents) }}<span class="muted small">/{{ p.unit }}</span></strong>
          </span>
        </button>
        <div v-if="!posProducts.length" class="empty pos-empty">
          {{ posCatalogBusy ? '正在装载商品…' : '该分类暂无在售商品，可切分类或直接扫码' }}
        </div>
      </div>
      <div class="pos-shortcuts">
        <span>F2 扫码</span><span>F3 会员</span><span>Ctrl+G 挂单</span><span>Ctrl+Q 取单</span><span>Ctrl+Z 清空</span>
      </div>
    </div>
  </div>
</template>
