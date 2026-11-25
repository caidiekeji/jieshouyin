<script setup lang="ts">
/** 餐饮收银页：桌台看板 → 点单 → 厨打 → 结算，三步布局与竞品主流一致。 */
import { onMounted, ref } from 'vue'
import { INDUSTRY_UI } from '../../constants'
import { api, formatMoney, type Combo, type OptionGroup, type ProductHit } from '../../api'
import { useAppContext } from '../../composables/useAppContext'
import RestaurantScene from '../../components/RestaurantScene.vue'
import PosCatalogPanel from '../../components/PosCatalogPanel.vue'
import PosCartPanel from '../../components/PosCartPanel.vue'
import HintTip from '../../components/HintTip.vue'

const {
  view,
  sceneBoardStage,
  activeTable,
  memberId,
  atLeast,
  addToCart,
  backToDiningTables,
  selectSceneTable,
  clearActiveTable,
  showSceneError,
} = useAppContext()
const ui = INDUSTRY_UI.RESTAURANT

/** 套餐与加料都是真实 SKU，前端只负责点选；售价由服务端读取 SKU，不信任界面传值。 */
const catalogTab = ref<'dish' | 'combo' | 'option'>('dish')
const catalogTabs = [
  { id: 'dish', label: '菜品' },
  { id: 'combo', label: '套餐' },
  { id: 'option', label: '加料' },
] as const
const combos = ref<Combo[]>([])
const optionGroups = ref<OptionGroup[]>([])

onMounted(async () => {
  try {
    ;[combos.value, optionGroups.value] = await Promise.all([
      api.listCombos(),
      api.listOptionGroups('RESTAURANT'),
    ])
  } catch (e) {
    showSceneError(String(e))
  }
})

function asHit(
  skuId: number,
  name: string,
  price: number,
  kind: string
): ProductHit {
  return {
    sku_id: skuId,
    spu_id: 0,
    spu_name: name,
    barcode: '',
    spec_text: kind,
    price_cents: price,
    member_price_cents: null,
    is_weighed: false,
    category_name: kind,
    category_group: '',
    unit: '份',
    image_path: null,
    image_url: null,
  }
}
</script>

<template>
  <main v-if="view === 'pos'" class="pos-layout" :class="{ 'scene-board-stage': sceneBoardStage }">
    <section v-if="sceneBoardStage" class="panel scene-home-panel">
      <RestaurantScene
        :active-table-id="null"
        :cart="[]"
        :member-id="memberId"
        :can-manage="atLeast('MANAGER')"
        @select="selectSceneTable"
        @cleared="clearActiveTable"
        @error="showSceneError"
      />
    </section>
    <template v-else>
      <section class="panel product-panel">
        <div v-if="activeTable" class="scene-active-bar">
          <div>
            <strong>{{ activeTable.code }}</strong>
            <span class="muted small">
              {{ activeTable.name }} · {{ activeTable.seats }}{{ ui.capacityUnit }}
              · {{ activeTable.status === 'OCCUPIED' ? '用餐中' : '未开台 · 点菜后自动开台' }}
            </span>
          </div>
          <button class="btn btn-sm ghost" @click="backToDiningTables">← 返回{{ ui.itemTitle }}</button>
        </div>
        <div class="pos-page-head">
          <strong>{{ activeTable ? `${activeTable.code} 点单` : ui.boardTitle }}</strong>
          <HintTip :text="ui.boardHint" />
        </div>
        <div class="pos-quick-row" aria-label="点单类型">
          <button
            v-for="t in catalogTabs"
            :key="t.id"
            :class="['pos-quick-chip', { active: catalogTab === t.id }]"
            @click="catalogTab = t.id"
          >{{ t.label }}</button>
        </div>
        <div v-if="catalogTab === 'combo'" class="pos-grid pos-extra-grid">
          <button
            v-for="c in combos"
            :key="c.combo_id"
            class="pos-card"
            @click="addToCart(asHit(c.sku_id, c.name, c.price_cents, '套餐'))"
          >
            <span class="pos-card-name">{{ c.name }}</span>
            <span class="pos-card-meta">{{ c.items.map((i) => i.name).join(' + ') || '未配置明细' }}</span>
            <strong class="price">¥{{ formatMoney(c.price_cents) }}</strong>
          </button>
          <div v-if="!combos.length" class="empty">还没有套餐。先在商品档案建一个服务项目，再到「餐饮配置」登记为套餐。</div>
        </div>
        <div v-else-if="catalogTab === 'option'" class="pos-grid pos-extra-grid">
          <template v-for="g in optionGroups" :key="g.group_id">
            <button
              v-for="o in g.items"
              :key="o.option_item_id"
              class="pos-card"
              @click="addToCart(asHit(o.sku_id, `${g.name}·${o.name}`, o.price_cents, g.name))"
            >
              <span class="pos-card-name">{{ o.name }}</span>
              <span class="pos-card-meta">{{ g.name }}</span>
              <strong class="price">+¥{{ formatMoney(o.price_cents) }}</strong>
            </button>
          </template>
          <div v-if="!optionGroups.length" class="empty">还没有加料选项组。</div>
        </div>
        <PosCatalogPanel v-else />
      </section>
      <PosCartPanel />
    </template>
  </main>
</template>
