<script setup lang="ts">
/** 养生收银页：服务间床位看板 → 项目点选 → 指定技师 → 计时结算。 */
import { INDUSTRY_UI } from '../../constants'
import { useAppContext } from '../../composables/useAppContext'
import WellnessRoomBoard from '../../components/WellnessRoomBoard.vue'
import WellnessScene from '../../components/WellnessScene.vue'
import PosCartPanel from '../../components/PosCartPanel.vue'
import HintTip from '../../components/HintTip.vue'

const {
  view,
  sceneBoardStage,
  activeTable,
  memberId,
  atLeast,
  backToDiningTables,
  addToCart,
  selectSceneTable,
  clearActiveTable,
  showSceneError,
} = useAppContext()
const ui = INDUSTRY_UI.WELLNESS
</script>

<template>
  <main v-if="view === 'pos'" class="pos-layout" :class="{ 'scene-board-stage': sceneBoardStage }">
    <section v-if="sceneBoardStage" class="panel scene-home-panel">
      <WellnessRoomBoard
        :title="ui.boardTitle"
        :hint="ui.boardHint"
        :default-prefix="ui.defaultPrefix"
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
              · {{ activeTable.status === 'OCCUPIED' ? '服务中' : '未开台 · 点项目后自动开台' }}
            </span>
          </div>
          <button class="btn btn-sm ghost" @click="backToDiningTables">← 返回{{ ui.itemTitle }}</button>
        </div>
        <div class="pos-page-head">
          <strong>{{ activeTable ? `${activeTable.code} 服务项目` : ui.boardTitle }}</strong>
          <HintTip :text="ui.boardHint" />
        </div>
        <WellnessScene @add="addToCart" @error="showSceneError" />
      </section>
      <PosCartPanel />
    </template>
  </main>
</template>
