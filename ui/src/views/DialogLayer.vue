<script setup lang="ts">
import Icon from '../components/Icon.vue'
import UiModal from '../components/UiModal.vue'
import UiButton from '../components/UiButton.vue'
import { useAppContext } from '../composables/useAppContext'

const ctx = useAppContext()
const {
  CASH_KEYPAD,
  CENTS_PER_YUAN,
  ICON_SIZE,
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
  closeCredDialog,
  confirmMemberDialog,
  confirmNativeLocal,
  confirmSensitiveAuth,
  closePwdDialog,
  couponCode,
  couponDialogInput,
  couponDialogOpen,
  couponDraft,
  credBusy,
  credChannel,
  credDialogOpen,
  credDraft,
  credSaved,
  currentCredFields,
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
  pwdBusy,
  pwdDialogOpen,
  pwdDraft,
  pwdError,
  removeCredential,
  resume,
  saveAllCredentials,
  saveLockCode,
  submitCheckout,
  submitChangePassword,
  toast,
  toastKind,
  unlock,
} = ctx
</script>

<template>
      <!-- 二次授权弹窗（§12.4） -->
      <UiModal :open="authDialog !== null" :title="authDialog?.title ?? ''" size="sm" @close="cancelSensitiveAuth">
        <input v-model="authUser" class="input" placeholder="授权人用户名" />
        <input v-model="authPass" class="input" type="password" placeholder="授权人密码" @keyup.enter="confirmSensitiveAuth" />
        <p v-if="authError" class="err-text">{{ authError }}</p>
        <template #actions>
          <UiButton variant="ghost" @click="cancelSensitiveAuth">取消</UiButton>
          <UiButton variant="primary" @click="confirmSensitiveAuth">确认授权</UiButton>
        </template>
      </UiModal>

      <!-- Native 支付弹窗（§6.3）：二维码 + 轮询 + 本地确认兜底 -->
      <UiModal :open="nativeDialog !== null" title="顾客扫码支付" @close="cancelNative">
        <div class="native-body">
          <div class="qr-box" v-html="nativeDialog?.qrSvg"></div>
          <p class="mono small muted">单号 {{ nativeDialog?.orderNo }}</p>
        </div>
        <template #actions>
          <UiButton variant="ghost" @click="cancelNative">取消支付</UiButton>
          <UiButton v-if="isLocalGateway" variant="primary" @click="confirmNativeLocal">本地确认收款</UiButton>
        </template>
      </UiModal>

      <!-- 挂单弹窗：付款区「挂单」按钮唤起 -->
      <UiModal :open="parkDialogOpen" title="待取挂单" @close="parkDialogOpen = false">
        <div class="park-dialog-list">
          <div v-for="p in parked" :key="String((p as any).park_id)" class="row-item">
            <span>挂单 {{ (p as any).park_id }} · {{ (p as any).created_at }}</span>
            <UiButton variant="ghost" size="sm" @click="resume((p as any).park_id)">取单</UiButton>
          </div>
          <div v-if="!parked.length" class="empty">暂无挂单</div>
        </div>
        <template #actions>
          <UiButton variant="ghost" @click="park">挂起当前购物车</UiButton>
          <UiButton variant="primary" @click="parkDialogOpen = false">关闭</UiButton>
        </template>
      </UiModal>

      <!-- 会员识别弹窗：付款区「会员」按钮唤起 -->
      <!-- 优惠券弹窗：点击付款区「优惠券」图标按钮唤起 -->
      <UiModal :open="couponDialogOpen" title="使用优惠券" @close="couponDialogOpen = false">
        <input
          ref="couponDialogInput"
          v-model="couponDraft"
          class="input"
          placeholder="优惠券码"
          @keyup.enter="applyCouponDialog"
        />
        <template #actions>
          <UiButton variant="ghost" @click="couponDialogOpen = false">取消</UiButton>
          <UiButton v-if="couponCode" variant="ghost" @click="clearCoupon(); couponDialogOpen = false">清除已用券</UiButton>
          <UiButton variant="primary" @click="applyCouponDialog">应用</UiButton>
        </template>
      </UiModal>

      <!-- 现金收银弹窗：应收 / 实收 / 找零 三行 + 数字小键盘 -->
      <UiModal :open="cashDialogOpen" title="现金收款" size="sm" @close="cashDialogOpen = false">
        <!-- 金额三行纵排：标签左、金额右，账目式读法 -->
        <div class="cash-summary">
          <div class="cash-line">
            <span class="cash-label">应收</span>
            <strong>¥{{ formatMoney(payableAmount) }}</strong>
          </div>
          <div class="cash-line">
            <span class="cash-label">实收</span>
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
          <div class="cash-line cash-line--change" :class="{ ok: cashOverpaid, short: cashShort > 0 }" aria-live="polite">
            <span class="cash-label">找零</span>
            <strong>¥{{ formatMoney(changeDue) }}</strong>
            <small v-if="cashShort > 0" class="cash-short">还差 ¥{{ formatMoney(cashShort) }}</small>
          </div>
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
        <template #actions>
          <UiButton variant="ghost" @click="cashDialogOpen = false">取消</UiButton>
          <UiButton variant="primary" :disabled="paying || cashShort > 0" @click="submitCheckout">
            <span v-if="paying" class="btn-spinner" role="status" aria-label="处理中"></span>
            <Icon v-else name="check" :size="ICON_SIZE.md" />{{ paying ? '处理中…' : '确认收款' }}
          </UiButton>
        </template>
      </UiModal>

      <UiModal :open="memberDialogOpen" title="识别会员" @close="memberDialogOpen = false">
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
        <template #actions>
          <UiButton variant="ghost" @click="memberDialogOpen = false">关闭</UiButton>
          <UiButton variant="primary" @click="confirmMemberDialog">识别会员</UiButton>
        </template>
      </UiModal>

      <!-- 锁机码锁屏：离岗锁屏或隐藏到托盘时覆盖整个界面 -->
      <div v-if="locked" class="lock-overlay">
        <div class="lock-card">
          <h2>收银台已锁定</h2>
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
      <UiModal :open="lockSetupOpen" title="设置锁机码" @close="cancelLockSetup">
        <input
          v-model="lockCodeDraft"
          class="input mono"
          type="password"
          placeholder="输入锁机码"
          @keyup.enter="saveLockCode"
        />
        <template #actions>
          <UiButton variant="ghost" @click="cancelLockSetup">取消</UiButton>
          <UiButton variant="primary" @click="saveLockCode">保存并锁屏</UiButton>
        </template>
      </UiModal>

      <!-- 商户凭据弹窗：由设置页「微信/支付宝凭据」按钮唤起；密钥类字段不回显，统一底部保存 -->
      <UiModal :open="credDialogOpen" :title="`商户凭据 · ${credChannel === 'WECHAT' ? '微信支付' : '支付宝'}`" @close="closeCredDialog">
        <label v-for="f in currentCredFields" :key="f.key" class="field-label cred-field">
          <span class="cred-field-head">
            {{ f.label }}
            <span v-if="credSaved[f.key]" class="cred-configured">
              已配置
              <button class="link danger" :aria-label="`删除${f.label}凭据`" @click.prevent="removeCredential(f.key)">删除</button>
            </span>
          </span>
          <textarea v-if="f.multiline" v-model="credDraft[f.key]" class="input mono" rows="3" :placeholder="credSaved[f.key] ? '已配置（写入新值将覆盖）' : '粘贴 PEM 或密钥'"></textarea>
          <input v-else v-model="credDraft[f.key]" class="input" type="password" :placeholder="credSaved[f.key] ? '已配置（写入新值将覆盖）' : '输入后统一保存'" />
        </label>
        <template #actions>
          <UiButton variant="ghost" @click="closeCredDialog">关闭</UiButton>
          <UiButton variant="primary" :disabled="credBusy" @click="saveAllCredentials">
            {{ credBusy ? '保存中…' : '保存修改' }}
          </UiButton>
        </template>
      </UiModal>

      <!-- 修改密码弹窗：用户下拉菜单唤起，当前登录用户改自己的密码 -->
      <UiModal :open="pwdDialogOpen" title="修改密码" size="sm" @close="closePwdDialog">
        <input v-model="pwdDraft.old" class="input" type="password" autocomplete="current-password" placeholder="原密码" />
        <input v-model="pwdDraft.next" class="input" type="password" autocomplete="new-password" placeholder="新密码（至少 8 位，含字母和数字）" />
        <input v-model="pwdDraft.confirm" class="input" type="password" autocomplete="new-password" placeholder="确认新密码" @keyup.enter="submitChangePassword" />
        <p v-if="pwdError" class="form-error" role="alert"><Icon name="alert" :size="ICON_SIZE.sm" />{{ pwdError }}</p>
        <template #actions>
          <UiButton variant="ghost" @click="closePwdDialog">取消</UiButton>
          <UiButton variant="primary" :disabled="pwdBusy" @click="submitChangePassword">
            {{ pwdBusy ? '保存中…' : '确认修改' }}
          </UiButton>
        </template>
      </UiModal>

      <div v-if="toast" class="toast" :class="{ err: toastKind === 'error' }" role="status" :aria-live="toastKind === 'error' ? 'assertive' : 'polite'">{{ toast }}</div>
</template>
