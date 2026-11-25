<script setup lang="ts">
/** 棋牌包间收银页：房态看板 → 计时点单 → 房费与茶水一单结算。 */
import { INDUSTRY_UI } from '../../constants'
import { useAppContext } from '../../composables/useAppContext'
import ChessRoomBoard from '../../components/ChessRoomBoard.vue'
import PosCatalogPanel from '../../components/PosCatalogPanel.vue'
import PosCartPanel from '../../components/PosCartPanel.vue'
import HintTip from '../../components/HintTip.vue'

const {
  view,
  sceneBoardStage,
  activeTable,
  memberId,
  atLeast,
  backToDiningTables,
  selectSceneTable,
  clearActiveTable,
  showSceneError,
} = useAppContext()
const ui = INDUSTRY_UI.BOARD_GAME
</script>

<template>
  <main v-if="view === 'pos'" class="pos-layout" :class="{ 'scene-board-stage': sceneBoardStage }">
    <section v-if="sceneBoardStage" class="panel scene-home-panel">
      <ChessRoomBoard
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
              · {{ activeTable.status === 'OCCUPIED' ? '使用中' : '未开台 · 点单后自动开台' }}
            </span>
          </div>
          <button class="btn btn-sm ghost" @click="backToDiningTables">← 返回{{ ui.itemTitle }}</button>
        </div>
        <div class="pos-page-head">
          <strong>{{ activeTable ? `${activeTable.code} 计时点单` : ui.boardTitle }}</strong>
          <HintTip :text="ui.boardHint" />
        </div>
        <PosCatalogPanel />
      </section>
      <PosCartPanel />
    </template>
  </main>
</template>
