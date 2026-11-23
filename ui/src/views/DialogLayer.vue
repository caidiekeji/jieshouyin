<script setup lang="ts">
import Icon from '../components/Icon.vue'
import { useAppContext } from '../composables/useAppContext'

const ctx = useAppContext()
const {
  CASH_KEYPAD,
  CENTS_PER_YUAN,
  ICON_SIZE,
  QUICK_CASH_YUAN,
  applyCouponDialog,
  authDialog,
  authError,
  authPass,
  authUser,
  cancelLockSetup,
  cancelNative,
  cancelSensitiveAuth,
  cashDialogInput,
  cashDialogOpen,
  cashOverpaid,
  cashShort,
  cashTendered,
  changeDue,
  clearCoupon,
  confirmMemberDialog,
  confirmNativeLocal,
  confirmSensitiveAuth,
  couponCode,
  couponDialogInput,
  couponDialogOpen,
  couponDraft,
  exactCash,
  formatMoney,
  isLocalGateway,
  lockCodeDraft,
  lockError,
  lockInput,
  lockSetupOpen,
  locked,
  memberBalances,
  memberDialogInput,
  memberDialogOpen,
  memberFindError,
  memberPhone,
  nativeDialog,
  onCashInput,
  orderNo,
  park,
  parkDialogOpen,
  parked,
  payableAmount,
  paying,
  pressCashKey,
  resume,
  saveLockCode,
  submitCheckout,
  toast,
  toastKind,
  unlock,
} = ctx
</script>

<template>
      <!-- 二次授权弹窗（§12.4） -->
      <div v-if="authDialog" class="modal-mask">
        <div class="modal" role="dialog" aria-modal="true" aria-label="二次授权">
          <h3>{{ authDialog?.title }}</h3>
          <p class="muted small">此项操作需店长或管理员二次授权</p>
          <input v-model="authUser" class="input" placeholder="授权人用户名" />
          <input v-model="authPass" class="input" type="password" placeholder="授权人密码" @keyup.enter="confirmSensitiveAuth" />
          <p v-if="authError" class="err-text">{{ authError }}</p>
          <div class="actions">
            <button class="btn" @click="cancelSensitiveAuth">取消</button>
            <button class="btn primary" @click="confirmSensitiveAuth">确认授权</button>
          </div>
        </div>
      </div>

      <!-- Native 支付弹窗（§6.3）：二维码 + 轮询 + 本地确认兜底 -->
      <div v-if="nativeDialog" class="modal-mask">
        <div class="modal native-modal" role="dialog" aria-modal="true" aria-label="顾客扫码支付">
          <h3>顾客扫码支付</h3>
          <p class="muted small">请顾客使用微信或支付宝扫一扫完成付款，收款后自动关闭</p>
          <div class="qr-box" v-html="nativeDialog.qrSvg"></div>
          <p class="mono small muted">单号 {{ nativeDialog.orderNo }}</p>
          <div class="actions">
            <button class="btn" @click="cancelNative">取消支付</button>
            <button v-if="isLocalGateway" class="btn primary" @click="confirmNativeLocal">本地确认收款</button>
          </div>
        </div>
      </div>

      <!-- 挂单弹窗：付款区「挂单」按钮唤起 -->
      <div v-if="parkDialogOpen" class="modal-mask" @click.self="parkDialogOpen = false">
        <div class="modal" role="dialog" aria-modal="true" aria-label="待取挂单">
          <h3>待取挂单</h3>
          <p class="muted small">点击「取单」将挂单商品放回购物车，促销与会员价现场重算。</p>
          <div class="park-dialog-list">
            <div v-for="p in parked" :key="String((p as any).park_id)" class="row-item">
              <span>挂单 {{ (p as any).park_id }} · {{ (p as any).created_at }}</span>
              <button class="btn" @click="resume((p as any).park_id)">取单</button>
            </div>
            <div v-if="!parked.length" class="empty">暂无挂单</div>
          </div>
          <div class="actions">
            <button class="btn" @click="park">挂起当前购物车</button>
            <button class="btn primary" @click="parkDialogOpen = false">关闭</button>
          </div>
        </div>
      </div>

      <!-- 会员识别弹窗：付款区「会员」按钮唤起 -->
      <!-- 优惠券弹窗：点击付款区「优惠券」图标按钮唤起 -->
      <div v-if="couponDialogOpen" class="modal-mask" @click.self="couponDialogOpen = false">
        <div class="modal" role="dialog" aria-modal="true" aria-label="使用优惠券">
          <h3>使用优惠券</h3>
          <p class="muted small">输入优惠券码，应用后按券规则计算优惠；不使用可清除。</p>
          <input
            ref="couponDialogInput"
            v-model="couponDraft"
            class="input"
            placeholder="优惠券码"
            @keyup.enter="applyCouponDialog"
          />
          <div class="actions">
            <button class="btn" @click="couponDialogOpen = false">取消</button>
            <button v-if="couponCode" class="btn" @click="clearCoupon(); couponDialogOpen = false">清除已用券</button>
            <button class="btn primary" @click="applyCouponDialog">应用</button>
          </div>
        </div>
      </div>

      <!-- 现金收银弹窗：应收 / 实收 / 找零 三栏 + 快捷面额 + 数字小键盘 -->
      <div v-if="cashDialogOpen" class="modal-mask" @click.self="cashDialogOpen = false">
        <div class="modal cash-modal" role="dialog" aria-modal="true" aria-label="现金收款">
          <h3>现金收款</h3>
          <div class="cash-summary">
            <div class="cash-cell">
              <span>应收</span>
              <strong>¥{{ formatMoney(payableAmount) }}</strong>
            </div>
            <div class="cash-cell tendered">
              <span>实收</span>
              <div class="cash-amount">
                <span>¥</span>
                <input
                  ref="cashDialogInput"
                  :value="cashTendered"
                  inputmode="decimal"
                  autocomplete="off"
                  aria-label="实收现金"
                  placeholder="0.00"
                  @input="onCashInput"
                  @keyup.enter="submitCheckout"
                />
              </div>
            </div>
            <div class="cash-cell change" :class="{ ok: cashOverpaid, short: cashShort > 0 }" aria-live="polite">
              <span>找零</span>
              <strong>¥{{ formatMoney(changeDue) }}</strong>
              <small v-if="cashShort > 0" class="cash-short">还差 ¥{{ formatMoney(cashShort) }}</small>
            </div>
          </div>
          <div class="quick-cash">
            <button
              v-for="v in QUICK_CASH_YUAN"
              :key="v"
              class="btn ghost quick-cash-btn"
              :disabled="v * CENTS_PER_YUAN < payableAmount"
              :title="v * CENTS_PER_YUAN < payableAmount ? `小于应收 ¥${formatMoney(payableAmount)}` : `实收 ¥${v}`"
              @click="cashTendered = String(v)"
            >¥{{ v }}</button>
            <button class="btn ghost quick-cash-btn exact" @click="exactCash">刚好</button>
          </div>
          <div class="cash-keypad" role="group" aria-label="现金数字键盘">
            <button
              v-for="k in CASH_KEYPAD"
              :key="k"
              class="cash-key"
              :class="{ muted: k === '退格' }"
              @click="pressCashKey(k)"
            >{{ k }}</button>
          </div>
          <div class="actions">
            <button class="btn" @click="cashDialogOpen = false">取消</button>
            <button class="btn primary" :disabled="paying || cashShort > 0" @click="submitCheckout">
              <span v-if="paying" class="btn-spinner" aria-label="处理中"></span>
              <Icon v-else name="check" :size="ICON_SIZE.md" />{{ paying ? '处理中…' : '确认收款' }}
            </button>
          </div>
        </div>
      </div>

      <div v-if="memberDialogOpen" class="modal-mask" @click.self="memberDialogOpen = false">
        <div class="modal" role="dialog" aria-modal="true" aria-label="识别会员">
          <h3>识别会员</h3>
          <p class="muted small">输入会员手机号或会员码，识别后按会员价结算；本单收款完成后自动清除。</p>
          <input
            ref="memberDialogInput"
            v-model="memberPhone"
            class="input"
            placeholder="会员手机号/会员码"
            @keyup.enter="confirmMemberDialog"
          />
          <p v-if="memberFindError" class="form-error" role="alert"><Icon name="alert" :size="ICON_SIZE.sm" />{{ memberFindError }}</p>
          <div v-if="memberBalances" class="member-balance">
            本金 ¥{{ formatMoney(memberBalances.principal_cents) }} · 赠金 ¥{{ formatMoney(memberBalances.bonus_cents) }} · 积分 {{ memberBalances.points }}
          </div>
          <p class="muted small">识别后本单按会员价结算，并在收款成功后自动累计积分（赠送金抵扣部分不计分）。</p>
          <div class="actions">
            <button class="btn" @click="memberDialogOpen = false">关闭</button>
            <button class="btn primary" @click="confirmMemberDialog">识别会员</button>
          </div>
        </div>
      </div>

      <!-- 锁机码锁屏：离岗锁屏或隐藏到托盘时覆盖整个界面 -->
      <div v-if="locked" class="lock-overlay">
        <div class="lock-card">
          <div class="brand-mark"><Icon name="lock" :size="ICON_SIZE.xxl" /></div>
          <h2>收银台已锁定</h2>
          <p class="muted small">收银员临时离岗，请输入锁机码解锁</p>
          <input
            v-model="lockInput"
            class="input mono lock-input"
            type="password"
            placeholder="输入锁机码"
            @keyup.enter="unlock"
          />
          <p v-if="lockError" class="error">{{ lockError }}</p>
          <button class="btn primary" @click="unlock">解锁</button>
        </div>
      </div>

      <!-- 锁机码设置：首次锁机尚未设码时由「锁机」按钮唤起 -->
      <div v-if="lockSetupOpen" class="modal-mask" @click.self="cancelLockSetup">
        <div class="modal" role="dialog" aria-modal="true" aria-label="设置锁机码">
          <h3>设置锁机码</h3>
          <p class="muted small">离岗锁屏、或关闭窗口隐藏到托盘后，输入此码解锁。摘要仅保存在本机，清空浏览器数据会一并清除。</p>
          <input
            v-model="lockCodeDraft"
            class="input mono"
            type="password"
            placeholder="输入锁机码"
            @keyup.enter="saveLockCode"
          />
          <div class="actions">
            <button class="btn" @click="cancelLockSetup">取消</button>
            <button class="btn primary" @click="saveLockCode">保存并锁屏</button>
          </div>
        </div>
      </div>

      <div v-if="toast" class="toast" :class="{ err: toastKind === 'error' }" role="status" :aria-live="toastKind === 'error' ? 'assertive' : 'polite'">{{ toast }}</div>
</template>
