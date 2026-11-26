<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { api, formatMoney, type CouponRow, type MemberRow, type PromotionRow } from '../api'
import {
  CENTS_PER_YUAN,
  DISCOUNT_ZHE_FACTOR,
  ERROR_DURATION_MS,
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
        <button class="btn" @click="reloadMembers">查询</button>
        <button class="btn primary" @click="showCreate = !showCreate">新增会员</button>
      </div>

      <div v-if="showCreate" class="card">
        <h3>新增会员</h3>
        <div class="form-row">
          <input v-model="mForm.phone" class="input" placeholder="手机号" />
          <input v-model="mForm.name" class="input" placeholder="姓名" />
          <button class="btn primary" @click="addMember">创建</button>
        </div>
      </div>

      <!-- 触屏卡片化：8 列表格改会员卡，充值按钮整宽 48px 热区 -->
      <div class="tcard-list">
        <div v-for="m in members" :key="m.member_id" class="tcard">
          <div class="tcard-head">
            <span class="tcard-code">{{ m.name || '—' }}</span>
            <span class="mono muted small">{{ m.member_code }}</span>
          </div>
          <div class="tcard-line"><span class="tcard-key">手机号</span><span class="tcard-value mono">{{ m.phone || '—' }}</span></div>
          <div class="tcard-line"><span class="tcard-key">储值</span><span class="tcard-value">本金 ¥{{ formatMoney(m.principal_cents) }} · 赠金 ¥{{ formatMoney(m.bonus_cents) }}</span></div>
          <div class="tcard-line"><span class="tcard-key">积分</span><span class="tcard-value">{{ m.points }} · 成长值 {{ m.growth }}</span></div>
          <div class="tcard-actions">
            <button class="btn primary" @click="openRecharge(m)">充值</button>
          </div>
        </div>
        <div v-if="!members.length" class="empty">暂无会员，可先在前台收银时识别或到设置页新增</div>
      </div>

      <div v-if="rechargeMemberId != null" class="card">
        <h3>会员充值：{{ rechargeName }}</h3>
        <div class="form-row">
          <input v-model="principal" class="input" type="number" placeholder="实充金额（元）" />
          <input v-model="bonus" class="input" type="number" placeholder="赠送金额（元，不可提现不可找零）" />
          <span class="input">收款方式：现金</span>
          <button class="btn primary" @click="submitRecharge">确认充值</button>
          <button class="btn ghost" @click="rechargeMemberId = null">取消</button>
        </div>
      </div>
    </template>

    <!-- 促销 -->
    <template v-else-if="tab === 'promo'">
      <div class="toolbar">
        <button class="btn primary" @click="showCreatePromo = !showCreatePromo">新建促销</button>
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
          <label v-if="pForm.mode === 'DISCOUNT'">折扣率<input v-model="pForm.discountRate" class="input" placeholder="填 0–1 小数，0.85 = 8.5 折" /></label>
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
        <button class="btn primary" @click="addPromo">创建规则</button>
      </div>

      <!-- 触屏卡片化：促销规则卡，状态徽标进标题行，启停按钮整宽 -->
      <div class="tcard-list">
        <div v-for="p in promos" :key="p.promotion_id" class="tcard">
          <div class="tcard-head">
            <span class="tcard-code">{{ p.name }}</span>
            <span class="badge" :class="p.status === 1 ? 'badge-ok' : 'badge-off'">{{ p.status === 1 ? '启用' : '停用' }}</span>
          </div>
          <div class="tcard-line"><span class="tcard-key">范围</span><span class="tcard-value">{{ p.scope === 'SINGLE_ITEM' ? '单品级' : '整单级' }}</span></div>
          <div class="tcard-line"><span class="tcard-key">门槛</span><span class="tcard-value">
            <span v-if="p.min_qty > 0">满 {{ p.min_qty }} 件 </span>
            <span v-if="p.min_amount_cents > 0">满 {{ formatMoney(p.min_amount_cents) }}</span>
            <span v-if="p.min_qty === 0 && p.min_amount_cents === 0">无门槛</span>
          </span></div>
          <div class="tcard-line"><span class="tcard-key">优惠</span><span class="tcard-value">
            <span v-if="p.discount_rate != null">{{ (p.discount_rate * DISCOUNT_ZHE_FACTOR).toFixed(1) }} 折</span>
            <span v-else>减 {{ formatMoney(p.discount_cents ?? 0) }}</span>
          </span></div>
          <div class="tcard-line"><span class="tcard-key">优先级</span><span class="tcard-value">{{ p.priority }}</span></div>
          <div class="tcard-actions">
            <button class="btn ghost" @click="togglePromo(p)">{{ p.status === 1 ? '停用' : '启用' }}</button>
          </div>
        </div>
        <div v-if="!promos.length" class="empty">暂无促销规则，用上方表单创建第一条</div>
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
        <button class="btn primary" @click="addCoupon">发券</button>
      </div>

      <div class="card">
        <h3>券列表</h3>
        <!-- 触屏卡片化：券卡只读展示，无行内操作 -->
        <div class="tcard-list">
          <div v-for="c in coupons" :key="c.coupon_code" class="tcard">
            <div class="tcard-head">
              <span class="mono tcard-code">{{ c.coupon_code }}</span>
              <span class="muted small">{{ c.status }}</span>
            </div>
            <div class="tcard-line"><span class="tcard-key">对象</span><span class="tcard-value">{{ c.member_name ?? '通用券' }}</span></div>
            <div class="tcard-line"><span class="tcard-key">优惠</span><span class="tcard-value">¥{{ formatMoney(c.discount_cents) }}<template v-if="c.min_order_cents > 0"> · 门槛 ¥{{ formatMoney(c.min_order_cents) }}</template><template v-else> · 无门槛</template></span></div>
            <div class="tcard-line"><span class="tcard-key">有效期</span><span class="tcard-value">{{ c.valid_until ?? '不限' }}</span></div>
          </div>
          <div v-if="!coupons.length" class="empty">暂无优惠券，用上方表单发放第一张</div>
        </div>
      </div>
    </template>
  </main>
</template>
