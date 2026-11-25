<script setup lang="ts">
/**
 * 手工称重入车：无秤门店输入「店内码 + 克重」后按后端 EAN-13 规则生成称重码，
 * 走已有的 lookup_weigh 接口计价，避免把称重品当按件商品误收。
 */
import { ref } from 'vue'
import { api } from '../api'
import { useAppContext } from '../composables/useAppContext'

const { pushWeighToCart, showToast } = useAppContext()

const instoreCode = ref('')
const grams = ref('')
const busy = ref(false)
const open = ref(false)

/** 与后端 ean13_check_digit 同算法：右起偶位 ×3、奇位 ×1 */
function checkDigit(body: string): number {
  let sum = 0
  for (let i = body.length - 1, rightIndex = 0; i >= 0; i--, rightIndex++) {
    const d = Number(body[i])
    sum += d * (rightIndex % 2 === 0 ? 3 : 1)
  }
  return (10 - (sum % 10)) % 10
}

function buildCode(): string | null {
  const code = instoreCode.value.trim()
  const rawGrams = grams.value.trim()
  if (!/^\d{5}$/.test(code)) return '店内码必须是 5 位数字'
  if (!/^\d{1,5}$/.test(rawGrams) || Number(rawGrams) <= 0) return '克重必须是 1–5 位数字且大于 0'
  const body = `22${code}${String(Number(rawGrams)).padStart(5, '0')}`
  return `${body}${checkDigit(body)}`
}

async function submit() {
  const code = buildCode()
  if (!code) return
  if (!/^\d{13}$/.test(code)) {
    showToast(code, true)
    return
  }
  busy.value = true
  try {
    const hit = await api.lookupWeigh(code)
    pushWeighToCart(hit)
    instoreCode.value = ''
    grams.value = ''
    open.value = false
  } catch (e) {
    showToast(`称重商品未找到：${String(e)}`, true)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <div class="weigh-entry">
    <button type="button" class="weigh-toggle" :aria-expanded="open" @click="open = !open">
      手工称重
    </button>
    <div v-if="open" class="weigh-form">
      <input v-model="instoreCode" class="input" inputmode="numeric" maxlength="5" placeholder="店内码 5 位" aria-label="称重商品店内码" />
      <input v-model="grams" class="input" inputmode="numeric" maxlength="5" placeholder="克重" aria-label="称重克重" />
      <button class="btn primary" :disabled="busy" @click="submit">{{ busy ? '计价中…' : '加入购物车' }}</button>
    </div>
  </div>
</template>
