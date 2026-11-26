<script setup lang="ts">
import TableCreateDialog from '../components/TableCreateDialog.vue'
import { useAppContext } from '../composables/useAppContext'
import { useTableAdmin } from '../composables/useTableAdmin'

const { atLeast } = useAppContext()
const { tables, feeProducts, busy, showCreate, err, feeText, createBatch, remove, clear } =
  useTableAdmin('WELLNESS', true, '服务间')
</script>

<template>
  <main class="main-simple scene-wellness-room">
    <div class="page-head">
      <div class="page-title">
        <h2>养生服务间管理</h2>
      </div>
      <button v-if="atLeast('MANAGER')" class="btn primary" @click="showCreate = true">
        批量新增
      </button>
    </div>
    <p v-if="err" class="error">{{ err }}</p>
    <!-- 触屏卡片化：桌面表格行内小按钮误触率高，改为整卡热区 + 底部整宽操作 -->
    <div class="tcard-list">
      <div v-for="t in tables" :key="t.table_id" class="tcard">
        <div class="tcard-head">
          <span class="mono tcard-code">{{ t.code }}</span>
          <span :class="t.status === 'OCCUPIED' ? 'warning-text' : ''">{{ t.status === 'OCCUPIED' ? '服务中' : '空闲' }}</span>
        </div>
        <div class="tcard-line"><span class="tcard-key">名称</span><span class="tcard-value">{{ t.name }}<template v-if="t.member_phone">（会员 {{ t.member_phone }}）</template></span></div>
        <div class="tcard-line"><span class="tcard-key">区域</span><span class="tcard-value">{{ t.zone || '未分区' }}</span></div>
        <div class="tcard-line"><span class="tcard-key">床位</span><span class="tcard-value">{{ t.seats }}</span></div>
        <div class="tcard-line"><span class="tcard-key">房费项目</span><span class="tcard-value muted small">{{ feeText(t) }}</span></div>
        <div class="tcard-actions">
          <button v-if="t.status === 'OCCUPIED'" class="btn" :disabled="busy" @click="clear(t)">清台</button>
          <button v-else-if="atLeast('MANAGER')" class="btn ghost danger-text" :disabled="busy" @click="remove(t)">删除</button>
        </div>
      </div>
      <div v-if="!tables.length" class="empty">还没有服务间，点击「批量新增」创建</div>
    </div>
    <TableCreateDialog
      :open="showCreate" title="养生服务间" capacity-label="床位" :capacity-default="1"
      :show-fee="true" :fee-products="feeProducts" default-prefix="W"
      @close="showCreate = false" @create="createBatch"
    />
  </main>
</template>
