<script setup lang="ts">
import TableCreateDialog from '../components/TableCreateDialog.vue'
import { useAppContext } from '../composables/useAppContext'
import { useTableAdmin } from '../composables/useTableAdmin'

const { atLeast } = useAppContext()
const { tables, busy, showCreate, err, createBatch, remove, clear } = useTableAdmin('DINING', false, '桌台')
</script>

<template>
  <main class="main-simple">
    <div class="page-head">
      <div class="page-title">
        <h2>餐饮桌台管理</h2>
      </div>
      <button v-if="atLeast('MANAGER')" class="btn primary" @click="showCreate = true">
        批量新增
      </button>
    </div>
    <p v-if="err" class="error">{{ err }}</p>
    <div class="card">
      <table class="tbl">
        <thead>
          <tr><th>编号</th><th>名称</th><th>区域</th><th class="num-cell">座位</th><th>状态</th><th>操作</th></tr>
        </thead>
        <tbody>
          <tr v-for="t in tables" :key="t.table_id">
            <td class="mono">{{ t.code }}</td>
            <td>{{ t.name }}<span v-if="t.member_phone" class="muted small">（会员 {{ t.member_phone }}）</span></td>
            <td>{{ t.zone || '未分区' }}</td>
            <td class="num-cell">{{ t.seats }}</td>
            <td><span :class="t.status === 'OCCUPIED' ? 'warning-text' : ''">{{ t.status === 'OCCUPIED' ? '用餐中' : '空闲' }}</span></td>
            <td class="row-actions-cell">
              <div class="row-actions">
                <button v-if="t.status === 'OCCUPIED'" class="btn btn-sm" :disabled="busy" @click="clear(t)">清台</button>
                <button v-else-if="atLeast('MANAGER')" class="btn btn-sm ghost danger-text" :disabled="busy" @click="remove(t)">
                  删除
                </button>
              </div>
            </td>
          </tr>
          <tr v-if="!tables.length"><td colspan="6" class="empty">还没有桌台，点击「批量新增」创建</td></tr>
        </tbody>
      </table>
    </div>
    <TableCreateDialog
      :open="showCreate" title="餐饮桌台" capacity-label="座位" :capacity-default="4"
      :show-fee="false" :fee-products="[]" default-prefix="A"
      @close="showCreate = false" @create="createBatch"
    />
  </main>
</template>
