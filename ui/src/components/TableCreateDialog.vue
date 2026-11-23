<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { formatMoney, type ProductHit, type TableCreateSpec } from '../api'

/** 台位批量创建弹窗：前缀+序号区间一次生成一批（餐饮桌台 / 棋牌·养生包间共用）。 */
const props = defineProps<{
  open: boolean
  title: string
  /** 棋牌/养生包间需绑定房费商品；餐饮桌台不需要 */
  showFee: boolean
  feeProducts: ProductHit[]
  defaultPrefix?: string
  defaultZone?: string
}>()
const emit = defineEmits<{ close: []; create: [items: TableCreateSpec[]] }>()

const prefix = ref('A')
const startNo = ref(1)
const endNo = ref(4)
const namePrefix = ref('')
const zone = ref('大厅')
const seats = ref(4)
const feeSkuId = ref<number | null>(null)
const feeUnit = ref<'' | 'MINUTE' | 'HOUR'>('')
const minMinutes = ref<number | ''>('')
const roundMinutes = ref<number | ''>('')
const error = ref('')

watch(
  () => props.open,
  (v) => {
    if (v) {
      prefix.value = props.defaultPrefix || 'A'
      zone.value = props.defaultZone || '大厅'
      error.value = ''
    }
  }
)

const count = computed(() => Math.max(0, endNo.value - startNo.value + 1))
const previewCode = computed(() => `${prefix.value}${String(startNo.value).padStart(padWidth.value, '0')}`)
const lastCode = computed(() => `${prefix.value}${String(endNo.value).padStart(padWidth.value, '0')}`)
const padWidth = computed(() => String(Math.max(endNo.value, 1)).length)

function validate(): string {
  const p = prefix.value.trim()
  if (!p || p.length > 8) return '台号前缀必填且不超过 8 字符'
  if (!Number.isInteger(startNo.value) || !Number.isInteger(endNo.value)) return '序号必须是整数'
  if (startNo.value < 0 || endNo.value < startNo.value) return '结束序号不能小于起始序号'
  if (count.value > 99) return '一批最多 99 个台位'
  if (props.showFee && !feeSkuId.value) return '请选择房费商品，并按"每分钟/每小时"维护好单价'
  if (minMinutes.value !== '' && minMinutes.value < 0) return '最低计费分钟不能为负'
  if (roundMinutes.value !== '' && roundMinutes.value < 0) return '取整分钟不能为负'
  return ''
}

function submit() {
  error.value = validate()
  if (error.value) return
  const p = prefix.value.trim()
  const items: TableCreateSpec[] = []
  for (let n = startNo.value; n <= endNo.value; n++) {
    const code = `${p}${String(n).padStart(padWidth.value, '0')}`
    items.push({
      code,
      name: namePrefix.value.trim() ? `${namePrefix.value.trim()}${code}` : code,
      zone: zone.value.trim(),
      seats: Math.min(50, Math.max(1, Math.floor(seats.value) || 4)),
      feeSkuId: props.showFee ? feeSkuId.value : null,
      feeUnit: props.showFee && feeUnit.value ? feeUnit.value : null,
      minMinutes: props.showFee && minMinutes.value !== '' ? Number(minMinutes.value) : null,
      roundMinutes: props.showFee && roundMinutes.value !== '' ? Number(roundMinutes.value) : null,
    })
  }
  emit('create', items)
}
</script>

<template>
  <div v-if="open" class="modal-mask" @click.self="emit('close')">
    <div class="modal table-create-modal" role="dialog" :aria-label="title">
      <h3>{{ title }}</h3>
      <div class="tc-grid">
        <label class="field-label">台号前缀
          <input v-model="prefix" class="input input-sm" maxlength="8" placeholder="A" />
        </label>
        <label class="field-label">起始序号
          <input v-model.number="startNo" class="input input-sm" type="number" min="0" max="999" />
        </label>
        <label class="field-label">结束序号
          <input v-model.number="endNo" class="input input-sm" type="number" min="0" max="999" />
        </label>
        <label class="field-label">座位/人数
          <input v-model.number="seats" class="input input-sm" type="number" min="1" max="50" />
        </label>
        <label class="field-label">区域
          <input v-model="zone" class="input input-sm" maxlength="12" placeholder="大厅" />
        </label>
        <label class="field-label">名称前缀（可空）
          <input v-model="namePrefix" class="input input-sm" maxlength="12" placeholder="如 二楼包间" />
        </label>
        <template v-if="showFee">
          <label class="field-label tc-wide">房费商品
            <select v-model.number="feeSkuId" class="input input-sm">
              <option :value="null" disabled>选择房费商品</option>
              <option v-for="p in feeProducts" :key="p.sku_id" :value="p.sku_id">
                {{ p.spu_name }} ¥{{ formatMoney(p.price_cents) }}
              </option>
            </select>
          </label>
          <label class="field-label">计价单位
            <select v-model="feeUnit" class="input input-sm" aria-label="房费计价单位">
              <option value="">跟随门店配置</option>
              <option value="MINUTE">单价/分钟</option>
              <option value="HOUR">单价/小时</option>
            </select>
          </label>
          <label class="field-label">最低(分钟)
            <input v-model.number="minMinutes" class="input input-sm" type="number" min="0" max="1440" placeholder="跟随" />
          </label>
          <label class="field-label">取整(分钟)
            <input v-model.number="roundMinutes" class="input input-sm" type="number" min="0" max="120" placeholder="跟随" />
          </label>
        </template>
      </div>
      <p class="tc-preview">
        将创建 <strong>{{ previewCode }} ~ {{ lastCode }}</strong> 共 {{ count }} 个{{ title }}
      </p>
      <p v-if="error" class="tc-error">{{ error }}</p>
      <div class="tc-actions">
        <button class="btn ghost" @click="emit('close')">取消</button>
        <button class="btn primary" @click="submit">批量创建</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.table-create-modal { max-width: 92vw; }
.tc-grid {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: var(--space-2);
}
.tc-wide { grid-column: span 2; }
.tc-preview { margin: 0; font-size: var(--font-sm); color: var(--muted); }
.tc-preview strong { color: var(--text); }
.tc-error { margin: 0; font-size: var(--font-sm); color: var(--danger); }
.tc-actions { display: flex; justify-content: flex-end; gap: var(--space-2); }
</style>
