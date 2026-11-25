<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { api, formatMoney, type ProductHit, type ServiceTable, type TableCreateSpec } from '../api'
import { ICON_SIZE } from '../constants'
import Icon from '../components/Icon.vue'
import TableCreateDialog from '../components/TableCreateDialog.vue'
import { useAppContext } from '../composables/useAppContext'

/**
 * 桌台/包间管理：行业专属的台位后台页（餐饮=桌台，棋牌/养生=包间）。
 * 看板保持纯净只做开台点单；批量创建、删除、清台等管理动作集中在这里。
 */
const ctx = useAppContext()
const { industry, atLeast } = ctx

const scene = computed<'DINING' | 'CHESS' | 'WELLNESS' | null>(() => {
  if (industry.value === 'RESTAURANT') return 'DINING'
  if (industry.value === 'BOARD_GAME') return 'CHESS'
  if (industry.value === 'WELLNESS') return 'WELLNESS'
  return null
})
const title = computed(() =>
  scene.value === 'DINING' ? '餐饮桌台管理' : scene.value === 'CHESS' ? '棋牌包间管理' : '养生包间管理'
)
/** 批量创建弹窗的条目标题（餐饮桌台 / 棋牌包间 / 养生包间） */
const itemTitle = computed(() => title.value.replace('管理', ''))

const tables = ref<ServiceTable[]>([])
const feeProducts = ref<ProductHit[]>([])
const busy = ref(false)
const showCreate = ref(false)
const err = ref('')

const canManage = computed(() => atLeast('MANAGER'))
const freeCount = computed(() => tables.value.filter((t) => t.status === 'FREE').length)
const occupiedCount = computed(() => tables.value.length - freeCount.value)

async function reload() {
  if (!scene.value) return
  try {
    tables.value = await api.listServiceTables(scene.value)
    err.value = ''
  } catch (e) {
    err.value = String(e)
  }
}

async function loadFees() {
  if (scene.value !== 'CHESS' && scene.value !== 'WELLNESS') return
  try {
    feeProducts.value = await api.lookup('', 200, '房费')
  } catch {
    feeProducts.value = []
  }
}

function feeText(t: ServiceTable): string {
  if (scene.value === 'DINING') return '—'
  const fee = feeProducts.value.find((p) => p.sku_id === t.fee_sku_id)
  if (!fee) return '未绑定房费'
  const unit = t.fee_unit === 'HOUR' ? '/小时' : t.fee_unit === 'MINUTE' ? '/分钟' : '（单位按门店配置）'
  return `${fee.spu_name} ¥${formatMoney(fee.price_cents)}${unit}`
}

/** 批量创建：逐个写入，遇错停止并保留已创建部分 */
async function createBatch(items: TableCreateSpec[]) {
  if (!scene.value || busy.value) return
  busy.value = true
  try {
    let done = 0
    for (const it of items) {
      try {
        await api.createServiceTable(
          scene.value, it.code, it.name, it.zone, it.seats,
          it.feeSkuId, it.feeUnit, it.minMinutes, it.roundMinutes
        )
        done++
      } catch (e) {
        err.value = done ? `已创建 ${done}/${items.length}，${it.code} 失败：${String(e)}` : String(e)
        break
      }
    }
    showCreate.value = false
    await reload()
  } finally {
    busy.value = false
  }
}

/** 删除空台位：占用中的台位必须先清台/结算 */
async function remove(t: ServiceTable) {
  const label = scene.value === 'DINING' ? '桌台' : '包间'
  if (t.status !== 'FREE') {
    err.value = `${t.code} 正在使用中，请先清台或结算后再删除`
    return
  }
  if (!window.confirm(`删除空${label} ${t.code}？`)) return
  busy.value = true
  try {
    await api.deleteServiceTable(t.table_id)
    err.value = ''
    await reload()
  } catch (e) {
    err.value = String(e)
  } finally {
    busy.value = false
  }
}

/** 未结算清台：丢弃购物车，留审计 */
async function clear(t: ServiceTable) {
  if (!window.confirm(`未结算清台 ${t.code}？购物车会丢弃，本操作留审计。`)) return
  busy.value = true
  try {
    await api.closeServiceTable(t.table_id, true)
    err.value = ''
    await reload()
  } catch (e) {
    err.value = String(e)
  } finally {
    busy.value = false
  }
}

onMounted(() => {
  void reload()
  void loadFees()
})
</script>

<template>
  <main class="main-simple">
    <template v-if="scene">
      <div class="page-head">
        <div class="page-title">
          <h2>{{ title }}</h2>
          <p class="muted small">
            共 {{ tables.length }} 个 · 空闲 {{ freeCount }} · 使用中 {{ occupiedCount }}。看板保持纯净，删除等管理动作集中在本页。
          </p>
        </div>
        <button v-if="canManage" class="btn primary" @click="showCreate = true">
          <Icon name="plus" :size="ICON_SIZE.md" />批量新增
        </button>
      </div>
      <p v-if="err" class="error">{{ err }}</p>
      <div class="card">
        <table class="tbl">
          <thead>
            <tr>
              <th>编号</th>
              <th>名称</th>
              <th>区域</th>
              <th class="num-cell">{{ scene === 'DINING' ? '座位' : '人数' }}</th>
              <th v-if="scene !== 'DINING'">房费绑定</th>
              <th>状态</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="t in tables" :key="t.table_id">
              <td class="mono">{{ t.code }}</td>
              <td>{{ t.name }}<span v-if="t.member_phone" class="muted small">（会员 {{ t.member_phone }}）</span></td>
              <td>{{ t.zone || '未分区' }}</td>
              <td class="num-cell">{{ t.seats }}</td>
              <td v-if="scene !== 'DINING'" class="muted small">{{ feeText(t) }}</td>
              <td>
                <span :class="t.status === 'OCCUPIED' ? 'warning-text' : ''">
                  {{ t.status === 'OCCUPIED' ? '使用中' : '空闲' }}
                </span>
              </td>
              <td class="row-actions-cell">
                <div class="row-actions">
                  <button v-if="t.status === 'OCCUPIED'" class="btn btn-sm" :disabled="busy" @click="clear(t)">清台</button>
                  <button
                    v-else-if="canManage"
                    class="btn btn-sm ghost danger-text"
                    :disabled="busy"
                    @click="remove(t)"
                  ><Icon name="trash" :size="ICON_SIZE.sm" />删除</button>
                </div>
              </td>
            </tr>
            <tr v-if="!tables.length">
              <td colspan="7" class="empty">还没有{{ scene === 'DINING' ? '桌台' : '包间' }}，点击「批量新增」创建</td>
            </tr>
          </tbody>
        </table>
      </div>
      <TableCreateDialog
        :open="showCreate"
        :title="itemTitle"
        :show-fee="scene !== 'DINING'"
        :fee-products="feeProducts"
        :default-prefix="scene === 'DINING' ? 'A' : scene === 'CHESS' ? 'B' : 'W'"
        @close="showCreate = false"
        @create="createBatch"
      />
    </template>
    <div v-else class="card">
      <p class="muted">当前行业不需要桌台/包间管理。桌台与包间仅餐饮、棋牌、养生行业可用。</p>
    </div>
  </main>
</template>
