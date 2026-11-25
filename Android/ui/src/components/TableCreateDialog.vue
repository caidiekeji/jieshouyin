<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from 'vue'
import { formatMoney, type ProductHit, type TableCreateSpec } from '../api'

/** 台位批量创建弹窗：业务口径由调用方传入，组件不绑定具体行业。 */
const props = withDefaults(defineProps<{
  open: boolean
  title: string
  /** 计时包间/服务间需绑定房费商品；餐饮桌台不需要 */
  showFee: boolean
  feeProducts: ProductHit[]
  defaultPrefix?: string
  defaultZone?: string
  capacityLabel?: string
  capacityDefault?: number
}>(), {
  defaultPrefix: 'A',
  defaultZone: '大厅',
  capacityLabel: '座位',
  capacityDefault: 4,
})
const emit = defineEmits<{ close: []; create: [items: TableCreateSpec[]] }>()

function onEscape(e: KeyboardEvent) {
  if (props.open && e.key === 'Escape') emit('close')
}
window.addEventListener('keydown', onEscape)
onUnmounted(() => window.removeEventListener('keydown', onEscape))

const prefix = ref('A')
const startNo = ref(1)
const endNo = ref(4)
const namePrefix = ref('')
const zone = ref(props.defaultZone)
const seats = ref(props.capacityDefault)
const feeSkuId = ref<number | null>(null)
const feeUnit = ref<'' | 'MINUTE' | 'HOUR'>('')
const minMinutes = ref<number | ''>('')
const roundMinutes = ref<number | ''>('')
const capYuan = ref<number | ''>('')
const error = ref('')
const feeUnitLocked = computed(() => {
  const unit = props.feeProducts.find((p) => p.sku_id === feeSkuId.value)?.unit
  return unit === '小时' || unit === '分钟'
})

watch(
  () => props.open,
  (v) => {
    if (v) {
      prefix.value = props.defaultPrefix
      zone.value = props.defaultZone
      seats.value = props.capacityDefault
      capYuan.value = ''
      error.value = ''
    }
  }
)
watch(feeSkuId, (id) => {
  const unit = props.feeProducts.find((p) => p.sku_id === id)?.unit
  feeUnit.value = unit === '小时' ? 'HOUR' : unit === '分钟' ? 'MINUTE' : ''
})

const count = computed(() => Math.max(0, endNo.value - startNo.value + 1))
const previewCode = computed(() => `${prefix.value}${String(startNo.value).padStart(padWidth.value, '0')}`)
const lastCode = computed(() => `${prefix.value}${String(endNo.value).padStart(padWidth.value, '0')}`)
const padWidth = computed(() => String(Math.max(endNo.value, 1)).length)

function validate(): string {
  const p = prefix.value.trim()
  if (!p || p.length > 8) return '编号前缀必填且不超过 8 字符'
  if (!Number.isInteger(startNo.value) || !Number.isInteger(endNo.value)) return '序号必须是整数'
  if (startNo.value < 0 || endNo.value < startNo.value) return '结束序号不能小于起始序号'
  if (count.value > 99) return '一批最多 99 个台位'
  if (props.showFee && !feeSkuId.value) return '请选择房费商品，并按"每分钟/每小时"维护好单价'
  if (minMinutes.value !== '' && minMinutes.value < 0) return '最低计费分钟不能为负'
  if (roundMinutes.value !== '' && roundMinutes.value < 0) return '取整分钟不能为负'
  if (
    props.showFee &&
    capYuan.value !== '' &&
    (capYuan.value < 0 || capYuan.value > 1_000_000)
  ) return '房费封顶须在 0–1000000 元之间'
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
      seats: Math.min(50, Math.max(1, Math.floor(seats.value) || props.capacityDefault)),
      feeSkuId: props.showFee ? feeSkuId.value : null,
      feeUnit: props.showFee && feeUnit.value ? feeUnit.value : null,
      minMinutes: props.showFee && minMinutes.value !== '' ? Number(minMinutes.value) : null,
      roundMinutes: props.showFee && roundMinutes.value !== '' ? Number(roundMinutes.value) : null,
      capCents:
        props.showFee && capYuan.value !== ''
          ? Math.round(Number(capYuan.value) * 100)
          : 0,
    })
  }
  emit('create', items)
}
</script>

<template>
  <div v-if="open" class="modal-mask" @click.self="emit('close')">
    <div class="modal table-create-modal" role="dialog" aria-modal="true" :aria-label="title">
      <h3>{{ title }}</h3>
      <div class="tc-grid">
        <label class="field-label">编号前缀
          <input v-model="prefix" class="input input-sm" maxlength="8" placeholder="A" />
        </label>
        <label class="field-label">起始序号
          <input v-model.number="startNo" class="input input-sm" type="number" min="0" max="999" />
        </label>
        <label class="field-label">结束序号
          <input v-model.number="endNo" class="input input-sm" type="number" min="0" max="999" />
        </label>
        <label class="field-label">{{ capacityLabel }}
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
          <label class="field-label">计价单位（随商品带出）
            <select v-model="feeUnit" class="input input-sm" :disabled="feeUnitLocked" aria-label="房费计价单位">
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
          <label class="field-label">房费封顶(元)
            <input v-model.number="capYuan" class="input input-sm" type="number" min="0" max="1000000" step="1" placeholder="跟随门店" />
          </label>
          <p class="tc-wide muted small">封顶只在计时金额超过该值时生效；留空表示跟随系统配置里的门店封顶。</p>
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
/* 批量创建弹窗：lg 档宽度避免 select 文案被挤压截断（如「跟随门店配置」） */
.table-create-modal { width: var(--layout-modal-lg); max-width: 92vw; }
.tc-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: var(--space-3) var(--space-3);
}
/* 统一控件高度：输入框/下拉/动作按钮同高，避免比例失衡 */
.tc-grid .input {
  width: 100%;
  height: var(--control-height-form);
  padding: 0 var(--space-2);
  font-size: var(--font-sm);
}
.tc-wide { grid-column: span 3; }
.tc-preview { margin: 0; font-size: var(--font-sm); color: var(--muted); }
.tc-preview strong { color: var(--text); }
.tc-error { margin: 0; font-size: var(--font-sm); color: var(--danger); }
.tc-actions { display: flex; justify-content: flex-end; gap: var(--space-2); }
.tc-actions .btn { height: var(--control-height-form); padding: 0 var(--space-4); }
</style>
