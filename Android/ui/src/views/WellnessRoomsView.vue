<script setup lang="ts">
import { computed } from 'vue'
import HintTip from '../components/HintTip.vue'
import TableCreateDialog from '../components/TableCreateDialog.vue'
import { useAppContext } from '../composables/useAppContext'
import { useTableAdmin } from '../composables/useTableAdmin'

const { atLeast } = useAppContext()
const { tables, feeProducts, busy, showCreate, err, feeText, createBatch, remove, clear } =
  useTableAdmin('WELLNESS', true, '服务间')
const freeCount = computed(() => tables.value.filter((t) => t.status === 'FREE').length)
const hint = computed(() => `共 ${tables.value.length} 间服务间 · 空闲 ${freeCount.value} · 使用中 ${tables.value.length - freeCount.value}。`)
</script>

<template>
  <main class="main-simple scene-wellness-room">
    <div class="page-head">
      <div class="page-title">
        <h2>养生服务间管理</h2>
        <HintTip :text="hint" />
      </div>
      <button v-if="atLeast('MANAGER')" class="btn primary" @click="showCreate = true">
        批量新增
      </button>
    </div>
    <p v-if="err" class="error">{{ err }}</p>
    <div class="card">
      <table class="tbl">
        <thead>
          <tr><th>服务间</th><th>名称</th><th>区域</th><th class="num-cell">床位</th><th>房费项目</th><th>状态</th><th>操作</th></tr>
        </thead>
        <tbody>
          <tr v-for="t in tables" :key="t.table_id">
            <td class="mono">{{ t.code }}</td>
            <td>{{ t.name }}<span v-if="t.member_phone" class="muted small">（会员 {{ t.member_phone }}）</span></td>
            <td>{{ t.zone || '未分区' }}</td>
            <td class="num-cell">{{ t.seats }}</td>
            <td class="muted small">{{ feeText(t) }}</td>
            <td><span :class="t.status === 'OCCUPIED' ? 'warning-text' : ''">{{ t.status === 'OCCUPIED' ? '服务中' : '空闲' }}</span></td>
            <td class="row-actions-cell">
              <div class="row-actions">
                <button v-if="t.status === 'OCCUPIED'" class="btn btn-sm" :disabled="busy" @click="clear(t)">清台</button>
                <button v-else-if="atLeast('MANAGER')" class="btn btn-sm ghost danger-text" :disabled="busy" @click="remove(t)">
                  删除
                </button>
              </div>
            </td>
          </tr>
          <tr v-if="!tables.length"><td colspan="7" class="empty">还没有服务间，点击「批量新增」创建</td></tr>
        </tbody>
      </table>
    </div>
    <TableCreateDialog
      :open="showCreate" title="养生服务间" capacity-label="床位" :capacity-default="1"
      :show-fee="true" :fee-products="feeProducts" default-prefix="W"
      @close="showCreate = false" @create="createBatch"
    />
  </main>
</template>
