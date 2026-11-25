<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { api, formatMoney, type CouponRow, type MemberRow, type PromotionRow } from '../api'
import Icon from './Icon.vue'
import {
  CENTS_PER_YUAN,
  DISCOUNT_ZHE_FACTOR,
  ERROR_DURATION_MS,
  ICON_SIZE,
  PAGE_SIZE,
  PROMO_DEFAULT_PRIORITY,
  TOAST_DURATION_MS,
} from '../constants'

const tab = ref<'member' | 'promo' | 'coupon'>('member')
const err = ref('')
const okMsg = ref('')

function fail(msg: unknown) {
  err.value = String(msg)
  okMsg.value = ''
  setTimeout(() => (err.value = ''), ERROR_DURATION_MS)
}
function done(msg: string) {
  okMsg.value = msg
  err.value = ''
  setTimeout(() => (okMsg.value = ''), TOAST_DURATION_MS)
}

// ===== 会员 =====
const members = ref<MemberRow[]>([])
const memberKw = ref('')
const showCreate = ref(false)
const mForm = ref({ phone: '', name: '' })
// 充值
const rechargeMemberId = ref<number | null>(null)
const rechargeChannel = ref('CASH')
const rechargeRequestId = ref('')
const rechargeName = ref('')
const principal = ref('')
const bonus = ref('')

async function reloadMembers() {
  try {
    members.value = await api.listMembers(memberKw.value.trim(), PAGE_SIZE.MEMBERS)
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
  rechargeChannel.value = 'CASH'
  // 同一充值会话复用同一 requestId：双击/失败重试都不会重复入账
  rechargeRequestId.value =
    typeof crypto !== 'undefined' && 'randomUUID' in crypto
      ? crypto.randomUUID()
      : `${Date.now()}-${Math.random().toString(36).slice(2)}`
}

async function submitRecharge() {
  if (rechargeMemberId.value == null) return
  const principalYuan = parseFloat(principal.value || '0')
  const bonusYuan = parseFloat(bonus.value || '0')
  if (!Number.isFinite(principalYuan) || principalYuan <= 0) {
    fail('实充金额须大于 0 元')
    return
  }
  if (!Number.isFinite(bonusYuan) || bonusYuan < 0) {
    fail('赠送金额不能为负数')
    return
  }
  try {
    await api.rechargeMember(
      rechargeMemberId.value,
      Math.round(principalYuan * CENTS_PER_YUAN),
      Math.round(bonusYuan * CENTS_PER_YUAN),
      rechargeChannel.value,
      rechargeRequestId.value
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
  priority: PROMO_DEFAULT_PRIORITY,
  exclusive: false,
  stackable: false,
  minQty: 0,
  minAmount: '',
  mode: 'DISCOUNT' as 'DISCOUNT' | 'FULL_CUT',
  discountRate: '',
  discountAmount: '',
  /** 适用商品 SKU ID，逗号分隔；留空表示全部商品 */
  targetSkusText: '',
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
    const rate = Number(pForm.value.discountRate)
    if (isRate && (!Number.isFinite(rate) || rate <= 0 || rate >= 1)) {
      throw new Error('折扣率需在 0 与 1 之间（如 0.85）')
    }
    const cut = Number(pForm.value.discountAmount || '0')
    if (!isRate && (!Number.isFinite(cut) || cut <= 0)) throw new Error('满减金额必须大于 0')
    const minAmount = Number(pForm.value.minAmount || '0')
    if (!Number.isFinite(minAmount) || minAmount < 0) throw new Error('门槛金额不能为负')
    const targetSkus = pForm.value.targetSkusText
      .split(/[，,]/)
      .map((s) => s.trim())
      .filter(Boolean)
      .map((s) => {
        const id = Number(s)
        if (!Number.isInteger(id) || id <= 0) throw new Error(`SKU ID 无效：${s}`)
        return id
      })
    await api.createPromotion({
      name: pForm.value.name,
      scope: pForm.value.scope,
      priority: pForm.value.priority,
      exclusive: pForm.value.exclusive,
      stackable: pForm.value.stackable,
      minQty: pForm.value.minQty,
      minAmountCents: Math.round(minAmount * CENTS_PER_YUAN),
      discountRate: isRate ? rate : null,
      discountCents: isRate ? null : Math.round(cut * CENTS_PER_YUAN),
      targetSkus,
      validFrom: null,
      validUntil: null
    })
    showCreatePromo.value = false
    pForm.value.targetSkusText = ''
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

// ===== 优惠券 =====
const coupons = ref<CouponRow[]>([])
/** 发放对象：空字符串表示通用券，否则为会员 ID（字符串存 select 值） */
const couponForm = ref({ memberId: '', discountYuan: '', minOrderYuan: '', validUntil: '' })

async function reloadCoupons() {
  try {
    coupons.value = await api.listCoupons()
  } catch (e) {
    fail(e)
  }
}

async function addCoupon() {
  const discount = parseFloat(couponForm.value.discountYuan)
  const minOrder = parseFloat(couponForm.value.minOrderYuan || '0')
  if (!Number.isFinite(discount) || discount <= 0) {
    fail('优惠金额须大于 0 元')
    return
  }
  if (!Number.isFinite(minOrder) || minOrder < 0) {
    fail('用券门槛不能为负数')
    return
  }
  // 归属必须显式选择：避免沿用收银台当前识别会员导致发错对象
  if (couponForm.value.memberId === '') {
    fail('请选择发放对象：指定会员或通用券')
    return
  }
  const memberId = couponForm.value.memberId === 'ALL' ? null : Number(couponForm.value.memberId)
  if (memberId != null && !Number.isInteger(memberId)) {
    fail('会员选择无效，请重新选择')
    return
  }
  try {
    const code = await api.issueCoupon(
      memberId,
      Math.round(discount * CENTS_PER_YUAN),
      Math.round(minOrder * CENTS_PER_YUAN),
      couponForm.value.validUntil.trim() || null
    )
    couponForm.value = { memberId: '', discountYuan: '', minOrderYuan: '', validUntil: '' }
    done(`优惠券 ${code} 已发放`)
    await reloadCoupons()
  } catch (e) {
    fail(e)
  }
}

onMounted(() => {
  reloadMembers()
  reloadPromos()
  reloadCoupons()
})
</script>

<template>
  <main class="main-simple">
    <div class="page-head">
      <h2>会员营销</h2>
      <div class="seg" role="tablist">
        <button :class="['seg-btn', { active: tab === 'member' }]" role="tab" :aria-selected="tab === 'member'" @click="tab = 'member'">会员管理</button>
        <button :class="['seg-btn', { active: tab === 'promo' }]" role="tab" :aria-selected="tab === 'promo'" @click="tab = 'promo'">促销规则</button>
        <button :class="['seg-btn', { active: tab === 'coupon' }]" role="tab" :aria-selected="tab === 'coupon'" @click="tab = 'coupon'">优惠券</button>
      </div>
    </div>

    <p v-if="err" class="error">{{ err }}</p>
    <p v-if="okMsg" class="ok-msg">{{ okMsg }}</p>

    <!-- 会员 -->
    <template v-if="tab === 'member'">
      <div class="toolbar">
        <input v-model="memberKw" class="input" placeholder="搜索手机号 / 会员名 / 会员号" @keyup.enter="reloadMembers" />
        <button class="btn" @click="reloadMembers"><Icon name="search" :size="ICON_SIZE.md" />查询</button>
        <button class="btn primary" @click="showCreate = !showCreate"><Icon name="plus" :size="ICON_SIZE.md" />新增会员</button>
      </div>

      <div v-if="showCreate" class="card">
        <h3>新增会员</h3>
        <div class="form-row">
          <input v-model="mForm.phone" class="input" placeholder="手机号" />
          <input v-model="mForm.name" class="input" placeholder="姓名" />
          <button class="btn primary" @click="addMember">创建</button>
        </div>
      </div>

      <div class="card">
        <table class="tbl">
          <thead>
            <tr><th>会员</th><th>手机号</th><th>会员号</th><th class="num-cell">储值本金</th><th class="num-cell">赠送金</th><th class="num-cell">积分</th><th class="num-cell">成长值</th><th>操作</th></tr>
          </thead>
          <tbody>
            <tr v-for="m in members" :key="m.member_id">
              <td>{{ m.name || '—' }}</td>
              <td class="mono">{{ m.phone || '—' }}</td>
              <td class="mono">{{ m.member_code }}</td>
              <td class="num-cell">{{ formatMoney(m.principal_cents) }}</td>
              <td class="num-cell">{{ formatMoney(m.bonus_cents) }}</td>
              <td class="num-cell">{{ m.points }}</td>
              <td class="num-cell">{{ m.growth }}</td>
              <td><button class="btn btn-sm" @click="openRecharge(m)">充值</button></td>
            </tr>
            <tr v-if="!members.length"><td colspan="8" class="empty">暂无会员，可先在前台收银时识别或到设置页新增</td></tr>
          </tbody>
        </table>
      </div>

      <div v-if="rechargeMemberId != null" class="card">
        <h3>会员充值：{{ rechargeName }}</h3>
        <p class="muted small">实充金额进入本金，赠送金额进入赠送金账户；赠送金不可提现、不可找零。</p>
        <div class="form-row">
          <input v-model="principal" class="input" type="number" placeholder="实充金额（元）" />
          <input v-model="bonus" class="input" type="number" placeholder="赠送金额（元）" />
          <span class="input">收款方式：现金</span>
          <button class="btn primary" @click="submitRecharge">确认充值</button>
          <button class="btn ghost" @click="rechargeMemberId = null">取消</button>
        </div>
        <p class="muted small">扫码充值需要渠道收款回调确认，当前版本仅支持现金充值（微信/支付宝充值待渠道收款接入后开放）。</p>
      </div>
    </template>

    <!-- 促销 -->
    <template v-else-if="tab === 'promo'">
      <div class="toolbar">
        <button class="btn primary" @click="showCreatePromo = !showCreatePromo"><Icon name="plus" :size="ICON_SIZE.md" />新建促销</button>
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
          <label v-if="pForm.mode === 'DISCOUNT'">折扣率<input v-model="pForm.discountRate" class="input" placeholder="0.85 = 8.5 折" /></label>
          <label v-else>满减金额（元）<input v-model="pForm.discountAmount" class="input" type="number" step="0.01" placeholder="如 5 = 减 5 元" /></label>
          <label>适用商品 SKU（逗号分隔）
            <input v-model="pForm.targetSkusText" class="input" placeholder="留空 = 全部商品；如 12,15" />
          </label>
          <div class="field">
            <span class="field-label">叠加方式</span>
            <div class="segmented" role="radiogroup" aria-label="叠加方式">
              <button type="button" role="radio" :aria-checked="!pForm.stackable" :class="{ active: !pForm.stackable }" @click="pForm.stackable = false; pForm.exclusive = true">互斥，不可叠加</button>
              <button type="button" role="radio" :aria-checked="pForm.stackable" :class="{ active: pForm.stackable }" @click="pForm.stackable = true; pForm.exclusive = false">可与其他规则叠加</button>
            </div>
          </div>
        </div>
        <p class="muted small">折扣率填 0–1 之间的小数（0.85 即 8.5 折）；指定 SKU 后规则只作用于这些商品。</p>
        <button class="btn primary" @click="addPromo">创建规则</button>
      </div>

      <div class="card">
        <table class="tbl">
          <thead>
            <tr><th>名称</th><th>范围</th><th>门槛</th><th>优惠</th><th class="num-cell">优先级</th><th>状态</th><th>操作</th></tr>
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
                <span v-if="p.discount_rate != null">{{ (p.discount_rate * DISCOUNT_ZHE_FACTOR).toFixed(1) }} 折</span>
                <span v-else>减 {{ formatMoney(p.discount_cents ?? 0) }}</span>
              </td>
              <td class="num-cell">{{ p.priority }}</td>
              <td><span class="badge" :class="p.status === 1 ? 'badge-ok' : 'badge-off'">{{ p.status === 1 ? '启用' : '停用' }}</span></td>
              <td><button class="btn btn-sm ghost" @click="togglePromo(p)">{{ p.status === 1 ? '停用' : '启用' }}</button></td>
            </tr>
            <tr v-if="!promos.length"><td colspan="7" class="empty">暂无促销规则，用上方表单创建第一条</td></tr>
          </tbody>
        </table>
      </div>
    </template>

    <!-- 优惠券 -->
    <template v-else-if="tab === 'coupon'">
      <div class="card">
        <h3>发放优惠券</h3>
        <div class="form-grid">
          <label class="field">
            <span class="field-label">发放对象</span>
            <select v-model="couponForm.memberId" class="input">
              <option value="" disabled>请选择发放对象</option>
              <option value="ALL">通用券（所有人可用）</option>
              <option v-for="m in members" :key="m.member_id" :value="String(m.member_id)">
                专属 · {{ m.name || m.phone || m.member_code }}
              </option>
            </select>
          </label>
          <label class="field">
            <span class="field-label">优惠金额（元）</span>
            <input v-model="couponForm.discountYuan" class="input" type="number" min="0.01" step="0.01" placeholder="如 20" />
          </label>
          <label class="field">
            <span class="field-label">使用门槛（元）</span>
            <input v-model="couponForm.minOrderYuan" class="input" type="number" min="0" step="0.01" placeholder="0 为无门槛" />
          </label>
          <label class="field">
            <span class="field-label">有效期至</span>
            <input v-model="couponForm.validUntil" class="input" type="datetime-local" />
          </label>
        </div>
        <p class="muted small">专属券仅发放对象识别为会员后可用；通用券任何人可用。有效期留空表示不设截止。</p>
        <button class="btn primary" @click="addCoupon">发券</button>
      </div>

      <div class="card">
        <h3>券列表</h3>
        <table class="tbl">
          <thead>
            <tr><th>券码</th><th>发放对象</th><th class="num-cell">优惠额</th><th class="num-cell">门槛</th><th>有效期至</th><th>状态</th></tr>
          </thead>
          <tbody>
            <tr v-for="c in coupons" :key="c.coupon_code">
              <td class="mono">{{ c.coupon_code }}</td>
              <td>{{ c.member_name ?? '通用券' }}</td>
              <td class="num-cell">¥{{ formatMoney(c.discount_cents) }}</td>
              <td class="num-cell">¥{{ formatMoney(c.min_order_cents) }}</td>
              <td>{{ c.valid_until ?? '不限' }}</td>
              <td>{{ c.status }}</td>
            </tr>
            <tr v-if="!coupons.length"><td colspan="6" class="empty">暂无优惠券，用上方表单发放第一张</td></tr>
          </tbody>
        </table>
      </div>
    </template>
  </main>
</template>
