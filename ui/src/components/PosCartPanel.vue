<script setup lang="ts">
/**
 * 收银购物车/结算组件（可复用）：会员、优惠、支付、挂单与小票。
 * 行业页面只决定它与商品区的排列方式；餐饮/养生的专属行操作保留在组件内按行业开关。
 */
import { useAppContext } from '../composables/useAppContext'
import { ICON_SIZE } from '../constants'
import HintTip from './HintTip.vue'
import Icon from './Icon.vue'
import Checkbox from './Checkbox.vue'

const {
  ICON_SIZE: ctxIconSize,
  activeTable,
  addPaySplit,
  cart,
  cashTendered,
  changeQty,
  channelEnabled,
  clearCart,
  clearCoupon,
  couponCode,
  couponHint,
  couponRecoverable,
  currentUser,
  effectivePrice,
  enterMixedMode,
  formatMoney,
  industry,
  lastOrder,
  memberBalances,
  memberId,
  nativeMode,
  onDutyTechnicians,
  openCouponDialog,
  openMemberDialog,
  openParkDialog,
  payAuthCode,
  payChannel,
  payCodeInput,
  payMethods,
  payMode,
  payShortfall,
  paySplits,
  payableAmount,
  paying,
  printAuto,
  printReceipt,
  productImageSrc,
  quote,
  rebindTableMember,
  removePaySplit,
  returnTableLine,
  selectPayChannel,
  setLineQty,
  storeProfile,
  submitCheckout,
  submitTableOrder,
  tableSentQty,
} = useAppContext()
const iconSize = ctxIconSize ?? ICON_SIZE
</script>

<template>
  <section class="panel cart-panel">
    <!-- 常驻上下文：门店 / 收银员 / 当前台位，收银员不用离开结算区确认身份 -->
    <div class="cart-context">
      <span><Icon name="store" :size="iconSize.sm" />{{ storeProfile?.store_name || '未命名门店' }}</span>
      <span><Icon name="user" :size="iconSize.sm" />{{ currentUser || '收银员' }}</span>
      <span><Icon name="scan" :size="iconSize.sm" />{{ activeTable ? activeTable.code : '散客收银' }}</span>
    </div>
    <h3>
      购物车 <span class="badge">{{ cart.length }}</span>
      <button
        v-if="activeTable"
        class="btn btn-sm ghost cart-order"
        :disabled="memberId == null"
        @click="rebindTableMember"
      >换绑会员</button>
      <button
        v-if="industry === 'RESTAURANT' && activeTable"
        class="btn btn-sm cart-order"
        @click="submitTableOrder"
      >下单厨打</button>
      <button v-if="cart.length" class="btn ghost btn-sm cart-clear" @click="clearCart">清空</button>
    </h3>
    <div class="cart-list">
      <div v-for="(l, i) in cart" :key="`${l.sku_id}-${i}`" class="cart-line">
        <img v-if="productImageSrc(l)" :src="productImageSrc(l)!" class="cart-thumb" alt="" />
        <span v-else class="cart-thumb cart-thumb-empty"></span>
        <div class="line-name">
          {{ l.name || `商品 ${l.sku_id}` }}
          <span v-if="l.member_price_cents" class="tag">会员价</span>
          <span v-if="tableSentQty[l.sku_id]" class="tag sent-tag">已下 {{ tableSentQty[l.sku_id] }}</span>
          <button
            v-if="industry === 'RESTAURANT' && activeTable && (tableSentQty[l.sku_id] || 0) > 0"
            class="link danger return-link"
            @click="returnTableLine(l)"
          >退菜</button>
          <select
            v-if="industry === 'WELLNESS'"
            v-model.number="l.technician_id"
            class="input input-sm tech-select"
            aria-label="指定技师"
          >
            <option :value="null">不指定技师</option>
            <option v-for="t in onDutyTechnicians" :key="t.technician_id" :value="t.technician_id">
              {{ t.name }} · {{ (t.commission_bps / 100).toFixed(0) }}%
            </option>
          </select>
        </div>
        <div v-if="l.is_chess_fee" class="qty-ctrl chess-fee-qty">
          <span>{{ l.chess_minutes }} 分钟</span>
        </div>
        <div v-else class="qty-ctrl">
          <button @click="changeQty(i, -1)" aria-label="减少数量"><Icon name="minus" :size="iconSize.sm" /></button>
          <input
            class="qty-input"
            type="number"
            min="1"
            :value="l.qty"
            aria-label="商品数量"
            @change="setLineQty(i, ($event.target as HTMLInputElement).value)"
          />
          <button @click="changeQty(i, 1)" aria-label="增加数量"><Icon name="plus" :size="iconSize.sm" /></button>
        </div>
        <span class="price">¥{{ formatMoney(l.qty * effectivePrice(l)) }}</span>
      </div>
      <div v-if="!cart.length" class="empty">购物车为空</div>
    </div>
    <div class="cart-foot">
      <div class="pay-quick-row">
        <button class="btn pay-quick" :class="{ active: memberId != null }" @click="openMemberDialog">
          <Icon name="user" :size="iconSize.md" />{{ memberId != null ? '会员（已识别）' : '会员' }}
        </button>
        <button
          class="btn pay-quick"
          :class="{
            active: !!couponCode && !couponRecoverable,
            pending: couponRecoverable,
          }"
          @click="openCouponDialog"
        >
          <Icon name="ticket" :size="iconSize.md" />{{ couponCode ? `券 ${couponCode}` : '优惠券' }}
        </button>
        <button v-if="couponCode" class="btn ghost pay-quick" @click="clearCoupon"><Icon name="close" :size="iconSize.sm" />清除券</button>
      </div>
      <span v-if="couponHint" class="small" :class="couponRecoverable ? 'warning-text' : 'muted'">{{ couponHint }}</span>
      <span v-if="memberBalances" class="member-balance-inline">
        本金 ¥{{ formatMoney(memberBalances.principal_cents) }} · 赠金 ¥{{ formatMoney(memberBalances.bonus_cents) }} · 积分 {{ memberBalances.points }}
      </span>
      <div v-if="memberId != null" class="points-earn-inline">
        <Icon name="star" :size="iconSize.sm" />
        <span v-if="(quote?.points_earned ?? 0) > 0">本单预计累计 <strong>+{{ quote?.points_earned }}</strong> 积分</span>
        <span v-else>本单无积分累计（未达整元或积分功能已关闭）</span>
      </div>
      <template v-if="cart.length">
        <div class="cart-total">
          <div class="cart-total-copy">
            <span>应收</span>
            <span v-if="quote && quote.discount_cents > 0" class="cart-total-meta">
              原价 ¥{{ formatMoney(quote.total_cents) }} · 优惠 ¥{{ formatMoney(quote.discount_cents) }}
              <template v-if="quote.round_off_cents > 0"> · 抹零 ¥{{ formatMoney(quote.round_off_cents) }}</template>
            </span>
          </div>
          <strong>¥{{ formatMoney(payableAmount) }}</strong>
        </div>
        <div class="pay-methods">
          <button
            v-for="m in payMethods"
            :key="m.id"
            :class="['pay-method-btn', { active: payMode === 'simple' && payChannel === m.id }]"
            :aria-pressed="payMode === 'simple' && payChannel === m.id"
            @click="selectPayChannel(m.id)"
          >
            <Icon :name="m.icon" :size="iconSize.md" />{{ m.label }}
          </button>
          <button
            :class="['pay-method-btn', { active: payMode === 'mixed' }]"
            :aria-pressed="payMode === 'mixed'"
            @click="enterMixedMode"
          >
            <Icon name="plus" :size="iconSize.md" />组合
          </button>
        </div>
        <div v-if="payMode === 'simple' && (payChannel === 'WECHAT' || payChannel === 'ALIPAY')" class="pay-panel">
          <template v-if="!nativeMode">
            <input
              ref="payCodeInput"
              v-model="payAuthCode"
              class="input input-lg"
              placeholder="扫描顾客付款码，回车即收款"
              @keyup.enter="submitCheckout"
            />
            <HintTip text="扫码枪扫顾客付款码即完成收款，金额已按应收自动带出。" />
            <button class="btn ghost" @click="nativeMode = true"><Icon name="qr" :size="iconSize.sm" />改为顾客扫屏幕二维码</button>
          </template>
          <template v-else>
            <p class="pay-hint">
              <Icon name="qr" :size="iconSize.md" />
              点击「收款」后将展示二维码，顾客用{{ payChannel === 'WECHAT' ? '微信' : '支付宝' }}扫码支付 ¥{{ formatMoney(payableAmount) }}。
            </p>
            <button class="btn ghost" @click="nativeMode = false"><Icon name="undo" :size="iconSize.sm" />改回扫描顾客付款码</button>
          </template>
        </div>
        <div v-else-if="payMode === 'simple' && payChannel === 'STORED_VALUE'" class="pay-panel">
          <p v-if="memberId == null" class="pay-hint warn"><Icon name="alert" :size="iconSize.md" />储值支付需先点击「会员」识别会员。</p>
          <p v-else class="pay-hint ok"><Icon name="wallet" :size="iconSize.md" />将从会员储值余额扣除 ¥{{ formatMoney(payShortfall > 0 ? payShortfall : 0) }}。</p>
        </div>
        <div v-else-if="payMode === 'mixed'" class="pay-splits">
          <div v-for="(s, i) in paySplits" :key="i" class="pay-split">
            <select v-model="s.channel" class="input split-channel" aria-label="支付渠道">
              <option v-if="channelEnabled.CASH !== false" value="CASH">现金</option>
              <option v-if="channelEnabled.STORED_VALUE !== false" value="STORED_VALUE">储值</option>
              <option v-if="channelEnabled.WECHAT !== false" value="WECHAT">微信</option>
              <option v-if="channelEnabled.ALIPAY !== false" value="ALIPAY">支付宝</option>
            </select>
            <input
              v-model="s.amountYuan"
              class="input split-amount"
              placeholder="金额(元)"
              aria-label="该支付方式金额"
              type="number"
              step="0.01"
            />
            <button
              v-if="paySplits.length > 1"
              class="btn ghost split-remove"
              aria-label="移除该支付方式"
              @click="removePaySplit(i)"
            ><Icon name="close" :size="iconSize.sm" /></button>
            <input
              v-if="(s.channel === 'WECHAT' || s.channel === 'ALIPAY') && !nativeMode"
              v-model="s.authCode"
              class="input split-auth"
              aria-label="顾客付款码"
              placeholder="扫描或输入顾客付款码"
            />
          </div>
          <div class="pay-split-tools">
            <button class="link add-split" @click="addPaySplit">+ 添加支付方式</button>
            <Checkbox v-model="nativeMode" label="顾客扫屏幕二维码支付" />
          </div>
          <input
            v-if="paySplits.some((s) => s.channel === 'CASH')"
            v-model="cashTendered"
            class="input"
            placeholder="现金实收(元)"
            aria-label="现金实收"
            type="number"
          />
          <div class="pay-mixed-status">
            <span class="pay-shortfall" :class="{ ok: payShortfall <= 0 }">
              待收 ¥{{ formatMoney(payShortfall > 0 ? payShortfall : 0) }}
            </span>
            <button class="link" @click="selectPayChannel('CASH')"><Icon name="undo" :size="iconSize.sm" />返回单笔支付</button>
          </div>
        </div>
        <div class="actions checkout-actions">
          <button class="btn" @click="openParkDialog"><Icon name="park" :size="iconSize.md" />挂单</button>
          <button class="btn primary" :disabled="paying" @click="submitCheckout">
            <span v-if="paying" class="btn-spinner" role="status" aria-label="处理中"></span>
            <Icon v-else name="check" :size="iconSize.md" />{{ paying ? '处理中…' : '收款' }}
          </button>
        </div>
      </template>
      <div v-else class="cart-idle">
        <Icon name="scan" :size="iconSize.lg" />
        <span>扫码或选择商品后开始收款</span>
      </div>
      <button v-if="lastOrder && printAuto" class="btn ghost full" @click="printReceipt"><Icon name="print" :size="iconSize.md" />重打小票 {{ lastOrder }}</button>
    </div>
  </section>
</template>
