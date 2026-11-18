<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { api, formatMoney, type MemberRow, type PromotionRow } from '../api'
import Icon from './Icon.vue'

const tab = ref<'member' | 'promo'>('member')
const err = ref('')
const okMsg = ref('')

function fail(msg: unknown) {
  err.value = String(msg)
  okMsg.value = ''
  setTimeout(() => (err.value = ''), 6000)
}
function done(msg: string) {
  okMsg.value = msg
  err.value = ''
  setTimeout(() => (okMsg.value = ''), 4000)
}

// ===== 会员 =====
const members = ref<MemberRow[]>([])
const memberKw = ref('')
const showCreate = ref(false)
const mForm = ref({ phone: '', name: '' })
// 充值
const rechargeMemberId = ref<number | null>(null)
const rechargeName = ref('')
const principal = ref('')
const bonus = ref('')

async function reloadMembers() {
  try {
    members.value = await api.listMembers(memberKw.value.trim(), 100)
  } catch (e) {
    fail(e)
  }
}

async function addMember() {
  try {
    await api.createMember(mForm.value.phone.trim() || null, mForm.value.name.trim() || null)
    showCreate.value = false
    mForm.value = { phone: '', name: '' }
    done('会员已创建')
    await reloadMembers()
  } catch (e) {
    fail(e)
  }
}

function openRecharge(m: MemberRow) {
  rechargeMemberId.value = m.member_id
  rechargeName.value = m.name || m.phone || m.member_code
  principal.value = ''
  bonus.value = ''
}

async function submitRecharge() {
  if (rechargeMemberId.value == null) return
  try {
    await api.rechargeMember(
      rechargeMemberId.value,
      Math.round(parseFloat(principal.value || '0') * 100),
      Math.round(parseFloat(bonus.value || '0') * 100)
    )
    rechargeMemberId.value = null
    done('充值成功')
    await reloadMembers()
  } catch (e) {
    fail(e)
  }
}

// ===== 促销 =====
const promos = ref<PromotionRow[]>([])
const showCreatePromo = ref(false)
const pForm = ref({
  name: '',
  scope: 'SINGLE_ITEM' as string,
  priority: 100,
  exclusive: false,
  stackable: false,
  minQty: 0,
  minAmount: '',
  mode: 'DISCOUNT' as 'DISCOUNT' | 'FULL_CUT',
  discountRate: '',
  discountAmount: '',
})

async function reloadPromos() {
  try {
    promos.value = await api.listPromotions()
  } catch (e) {
    fail(e)
  }
}

async function addPromo() {
  const isRate = pForm.value.mode === 'DISCOUNT'
  try {
    await api.createPromotion({
      name: pForm.value.name,
      scope: pForm.value.scope,
      priority: pForm.value.priority,
      exclusive: pForm.value.exclusive,
      stackable: pForm.value.stackable,
      minQty: pForm.value.minQty,
      minAmountCents: Math.round(parseFloat(pForm.value.minAmount || '0') * 100),
      discountRate: isRate ? parseFloat(pForm.value.discountRate || '0') : null,
      discountCents: isRate ? null : Math.round(parseFloat(pForm.value.discountAmount || '0') * 100),
      targetSkus: [],
      validFrom: null,
      validUntil: null
    })
    showCreatePromo.value = false
    done('促销规则已创建')
    await reloadPromos()
  } catch (e) {
    fail(e)
  }
}

async function togglePromo(p: PromotionRow) {
  try {
    await api.setPromotionStatus(p.promotion_id, p.status === 1 ? 0 : 1)
    done(p.status === 1 ? '已停用' : '已启用')
    await reloadPromos()
  } catch (e) {
    fail(e)
  }
}

onMounted(() => {
  reloadMembers()
  reloadPromos()
})
</script>

<template>
  <main class="main-simple">
    <div class="page-head">
      <h2>会员营销</h2>
      <div class="tab-row">
        <button :class="['btn', { primary: tab === 'member' }]" @click="tab = 'member'">会员管理</button>
        <button :class="['btn', { primary: tab === 'promo' }]" @click="tab = 'promo'">促销规则</button>
      </div>
    </div>

    <p v-if="err" class="error">{{ err }}</p>
    <p v-if="okMsg" class="ok-msg">{{ okMsg }}</p>

    <!-- 会员 -->
    <template v-if="tab === 'member'">
      <div class="toolbar">
        <input v-model="memberKw" class="input" placeholder="搜索手机号 / 会员名 / 会员号" @keyup.enter="reloadMembers" />
        <button class="btn" @click="reloadMembers"><Icon name="refresh" :size="16" />查询</button>
        <button class="btn primary" @click="showCreate = !showCreate"><Icon name="plus" :size="16" />新增会员</button>
      </div>

      <div v-if="showCreate" class="card">
        <h3>新增会员</h3>
        <div class="inline-form">
          <input v-model="mForm.phone" class="input" placeholder="手机号" />
          <input v-model="mForm.name" class="input" placeholder="姓名" />
          <button class="btn primary" @click="addMember">创建</button>
        </div>
      </div>

      <div class="card">
        <table class="tbl">
          <thead>
            <tr><th>会员</th><th>手机号</th><th>会员号</th><th>储值本金</th><th>赠送金</th><th>积分</th><th>成长值</th><th>操作</th></tr>
          </thead>
          <tbody>
            <tr v-for="m in members" :key="m.member_id">
              <td>{{ m.name || '—' }}</td>
              <td class="mono">{{ m.phone || '—' }}</td>
              <td class="mono">{{ m.member_code }}</td>
              <td>{{ formatMoney(m.principal_cents) }}</td>
              <td>{{ formatMoney(m.bonus_cents) }}</td>
              <td>{{ m.points }}</td>
              <td>{{ m.growth }}</td>
              <td><button class="btn btn-sm" @click="openRecharge(m)">充值</button></td>
            </tr>
            <tr v-if="!members.length"><td colspan="8" class="muted">暂无会员</td></tr>
          </tbody>
        </table>
      </div>

      <div v-if="rechargeMemberId != null" class="card">
        <h3>会员充值：{{ rechargeName }}</h3>
        <p class="muted small">实充进入本金，赠送额进入赠送金账户（§13.3，不可提现）</p>
        <div class="inline-form">
          <input v-model="principal" class="input" type="number" placeholder="实充金额（元）" />
          <input v-model="bonus" class="input" type="number" placeholder="赠送金额（元）" />
          <button class="btn primary" @click="submitRecharge">确认充值</button>
          <button class="btn ghost" @click="rechargeMemberId = null">取消</button>
        </div>
      </div>
    </template>

    <!-- 促销 -->
    <template v-else>
      <div class="toolbar">
        <button class="btn primary" @click="showCreatePromo = !showCreatePromo"><Icon name="plus" :size="16" />新建促销</button>
      </div>

      <div v-if="showCreatePromo" class="card">
        <h3>新建促销规则</h3>
        <div class="form-grid">
          <label>名称<input v-model="pForm.name" class="input" placeholder="如 可乐第二件半价" /></label>
          <label>作用范围
            <select v-model="pForm.scope" class="input">
              <option value="SINGLE_ITEM">单品级</option>
              <option value="ORDER_LEVEL">整单级</option>
            </select>
          </label>
          <label>优先级（小者优先）<input v-model.number="pForm.priority" class="input" type="number" /></label>
          <label>满件数门槛<input v-model.number="pForm.minQty" class="input" type="number" min="0" /></label>
          <label>满金额门槛（元）<input v-model="pForm.minAmount" class="input" placeholder="如 100（可留 0）" /></label>
          <label>优惠方式
            <select v-model="pForm.mode" class="input">
              <option value="DISCOUNT">折扣率</option>
              <option value="FULL_CUT">满减</option>
            </select>
          </label>
          <label v-if="pForm.mode === 'DISCOUNT'">折扣率<input v-model="pForm.discountRate" class="input" placeholder="0.85 = 85折" /></label>
          <label v-else>满减金额（分）<input v-model="pForm.discountAmount" class="input" placeholder="如 500 = 5 元" /></label>
          <label class="checkbox-label"><input v-model="pForm.exclusive" type="checkbox" /> 互斥（不可叠加）</label>
          <label class="checkbox-label"><input v-model="pForm.stackable" type="checkbox" /> 可叠加</label>
        </div>
        <p class="muted small">当前版本规则作用于全部商品；指定商品促销（target_skus）后续版本支持</p>
        <button class="btn primary" @click="addPromo">创建规则</button>
      </div>

      <div class="card">
        <table class="tbl">
          <thead>
            <tr><th>名称</th><th>范围</th><th>门槛</th><th>优惠</th><th>优先级</th><th>状态</th><th>操作</th></tr>
          </thead>
          <tbody>
            <tr v-for="p in promos" :key="p.promotion_id">
              <td>{{ p.name }}</td>
              <td>{{ p.scope === 'SINGLE_ITEM' ? '单品级' : '整单级' }}</td>
              <td>
                <span v-if="p.min_qty > 0">满 {{ p.min_qty }} 件</span>
                <span v-if="p.min_amount_cents > 0">满 {{ formatMoney(p.min_amount_cents) }}</span>
                <span v-if="p.min_qty === 0 && p.min_amount_cents === 0">无门槛</span>
              </td>
              <td>
                <span v-if="p.discount_rate != null">{{ (p.discount_rate * 10).toFixed(1) }} 折</span>
                <span v-else>减 {{ formatMoney(p.discount_cents ?? 0) }}</span>
              </td>
              <td>{{ p.priority }}</td>
              <td>{{ p.status === 1 ? '启用' : '停用' }}</td>
              <td><button class="btn btn-sm ghost" @click="togglePromo(p)">{{ p.status === 1 ? '停用' : '启用' }}</button></td>
            </tr>
            <tr v-if="!promos.length"><td colspan="7" class="muted">暂无促销规则</td></tr>
          </tbody>
        </table>
      </div>
    </template>
  </main>
</template>

<style scoped>
.page-head { display: flex; align-items: center; gap: 16px; margin-bottom: 12px; }
.tab-row { display: flex; gap: 8px; }
.toolbar { display: flex; gap: 8px; margin-bottom: 12px; }
.toolbar .input { flex: 1; }
.tbl { width: 100%; border-collapse: collapse; font-size: 13px; }
.tbl th, .tbl td { padding: 8px 10px; text-align: left; border-bottom: 1px solid var(--border); }
.tbl th { color: var(--muted); font-weight: 600; font-size: 12px; }
.btn-sm { padding: 2px 8px; font-size: 12px; }
.input-sm { width: 70px; padding: 2px 6px; font-size: 12px; }
.form-grid { display: grid; grid-template-columns: repeat(4, 1fr); gap: 10px; margin-bottom: 10px; }
.form-grid label { display: flex; flex-direction: column; gap: 4px; font-size: 12px; color: var(--muted); }
.checkbox-label { flex-direction: row !important; align-items: center; gap: 6px !important; }
.chip-row { display: flex; flex-wrap: wrap; gap: 8px; margin-bottom: 10px; }
.ok-msg { color: var(--ok, #27ae60); font-size: 13px; margin-bottom: 8px; }
.inline-form { display: flex; gap: 8px; align-items: center; }
.card { margin-bottom: 12px; }
.mono { font-family: monospace; }
.warning-text { color: var(--danger, #c0392b); }
</style>
