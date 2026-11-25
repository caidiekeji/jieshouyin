<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { listen } from '@tauri-apps/api/event'
import Icon from './components/Icon.vue'
import ProductAdmin from './components/ProductAdmin.vue'
import CategoryAdmin from './components/CategoryAdmin.vue'
import SupplyAdmin from './components/SupplyAdmin.vue'
import MemberAdmin from './components/MemberAdmin.vue'
import OrderDetailPanel from './components/OrderDetailPanel.vue'
import RefundPanel from './components/RefundPanel.vue'
import AuthView from './views/AuthView.vue'
import PosView from './views/PosView.vue'
import ReportView from './views/ReportView.vue'
import SettingsView from './views/SettingsView.vue'
import RestaurantTablesView from './views/RestaurantTablesView.vue'
import ChessRoomsView from './views/ChessRoomsView.vue'
import WellnessRoomsView from './views/WellnessRoomsView.vue'
import DialogLayer from './views/DialogLayer.vue'
import { INDUSTRY_UI } from './constants'
import { useAppStore } from './stores/app'
import { usePosStore } from './stores/pos'
import { useOpsStore } from './stores/ops'
import { useAppContext } from './composables/useAppContext'

const ctx = useAppContext()
const {
  ICON_SIZE, applyTheme, atLeast, authDialog,
  banner, bootError, cancelLockSetup, cancelNative,
  cancelSensitiveAuth, cart, cashDialogOpen, clearCart,
  couponDialogOpen, credDialogOpen, closeCredDialog, currentRole, currentUser, doLogout,
  focusScan, goPOS, goProducts, goReport, goShift,
  goSettings, goodsTab, gotoOrder, gotoRefund, industry, keyword,
  loadLockCode, loadPosCatalog, loadTechnicians, lockScreen, lockSetupOpen, memberDialogOpen,
  memberId, nativeDialog, nativeMode, openMemberDialog, openPwdDialog,
  openParkDialog, orderNo, park, parkDialogOpen, pwdDialogOpen, closePwdDialog,
  parked, payChannel, payMode, posDialogOpen,
  products, refreshQuote, refundOrderNo, requireSensitiveAuth,
  scheduleTableSave, search, settingsTab, settingsTabs,
  stopChessFeeTimer, stopNativePolling, storeProfile, theme,
  view
} = ctx

// 认证四屏（boot/industry/activate/login）统一交给 AuthView 渲染
const isAuthStage = computed(() => ['boot', 'industry', 'activate', 'login'].includes(view.value))
const industryUi = computed(() => INDUSTRY_UI[industry.value] ?? INDUSTRY_UI.SUPERMARKET)

// store 实例：生命周期 watch / 快捷键直接使用
const app = useAppStore()
const pos = usePosStore()
const ops = useOpsStore()

// ===== 视图层 watch（状态联动）=====
watch(cart, () => pos.scheduleTableSave(), { deep: true })
watch(industry, () => pos.onIndustryChange())
watch(keyword, (val) => pos.onKeywordChange(val))
watch(view, (v) => {
  if (v === 'pos') {
    void pos.loadPosCatalog()
    void pos.loadTechnicians()
  }
})
watch([view, currentRole], () => {
  const guarded: string[] = ['products', 'categories', 'tables', 'supply', 'member', 'refund', 'order', 'report', 'settings', 'parked']
  if (guarded.includes(view.value) && !app.atLeast('MANAGER')) view.value = 'pos'
  if (view.value === 'settings') {
    const tab = app.settingsTabs.find((t) => t.id === settingsTab.value)
    if (tab && !app.atLeast(tab.minRole)) settingsTab.value = 'overview'
  }
})
watch(
  () => [payMode.value, payChannel.value, nativeMode.value] as const,
  () => {
    if (payMode.value === 'simple' && !nativeMode.value &&
        (payChannel.value === 'WECHAT' || payChannel.value === 'ALIPAY')) {
      void nextTick(() => pos.onPayChannelFocusWatch())
    }
  }
)
watch(cashDialogOpen, (open) => { if (open) void nextTick(() => pos.onCashDialogOpenWatch()) })
watch(memberDialogOpen, (open) => { if (open) void nextTick(() => pos.onMemberDialogOpenWatch()) })
watch(couponDialogOpen, (open) => { if (open) void nextTick(() => pos.onCouponDialogOpenWatch()) })
watch(
  [() => cart.value, memberId],
  () => { void pos.refreshQuote() },
  { deep: true }
)

// ===== 快捷键（视图层）=====
function handlePosShortcut(e: KeyboardEvent): boolean {
  if (view.value !== 'pos' || e.repeat) return false
  const key = e.key.toLowerCase()
  if (e.key === 'F2' && !posDialogOpen()) {
    e.preventDefault(); focusScan(); return true
  }
  if (e.key === 'F3' && !posDialogOpen()) {
    e.preventDefault(); openMemberDialog(); return true
  }
  if (e.ctrlKey && !e.altKey && key === 'g' && !posDialogOpen()) {
    e.preventDefault(); void park(); return true
  }
  if (e.ctrlKey && !e.altKey && key === 'q' && !posDialogOpen()) {
    e.preventDefault(); void openParkDialog(); return true
  }
  const tag = (e.target as HTMLElement | null)?.tagName
  const editing = tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT'
  if (e.ctrlKey && !e.altKey && key === 'z' && !editing && !posDialogOpen()) {
    e.preventDefault(); clearCart(); return true
  }
  return false
}
function onGlobalKeydown(e: KeyboardEvent) {
  if (handlePosShortcut(e)) return
  if (e.key !== 'Escape') return
  if (userMenuOpen.value) { userMenuOpen.value = false; return }
  if (lockSetupOpen.value) { cancelLockSetup(); return }
  if (pwdDialogOpen.value) { closePwdDialog(); return }
  if (memberDialogOpen.value) { memberDialogOpen.value = false; return }
  if (couponDialogOpen.value) { couponDialogOpen.value = false; return }
  if (cashDialogOpen.value) { cashDialogOpen.value = false; return }
  if (parkDialogOpen.value) { parkDialogOpen.value = false; return }
  if (authDialog.value) { cancelSensitiveAuth(); return }
  if (credDialogOpen.value) { closeCredDialog(); return }
  if (nativeDialog.value) { cancelNative(); return }
}

// 用户下拉菜单：交班/锁机/修改密码/退出统一收进账户菜单，顶部导航只留业务视图
const userMenuOpen = ref(false)
function pickMenu(fn: () => void) {
  userMenuOpen.value = false
  fn()
}
function onDocClick(e: MouseEvent) {
  if (!userMenuOpen.value) return
  if (!(e.target as HTMLElement | null)?.closest('.user')) userMenuOpen.value = false
}

onMounted(async () => {
  loadLockCode()
  void listen('lock-request', () => { lockScreen() }).catch(() => {})
  const saved = localStorage.getItem('checkoutr-theme') as 'light' | 'dark' | null
  if (saved) applyTheme(saved)
  window.addEventListener('keydown', onGlobalKeydown)
  window.addEventListener('click', onDocClick)
  await app.bootstrap()
})
onUnmounted(() => {
  window.removeEventListener('keydown', onGlobalKeydown)
  window.removeEventListener('click', onDocClick)
  pos.stopNativePolling()
  pos.stopChessFeeTimer()
})
</script>

<template>
  <div class="app-root">
    <div v-if="bootError" class="fatal">启动失败：{{ bootError }}</div>

    <!-- 认证流程：启动中 / 行业选择 / 在线激活 / 登录 -->
    <AuthView v-else-if="isAuthStage" />

    <!-- 主界面 -->
    <div v-else class="shell">
      <header class="topbar">
        <div class="logo">
          <img src="/logo.png" class="logo-img" alt="logo" />
          <span>{{ storeProfile?.store_name || 'Checkout-R' }}</span>
          <span v-if="storeProfile?.industry_name" class="badge">{{ storeProfile.industry_name }}</span>
        </div>
        <nav class="nav">
          <button :class="['nav-btn', { active: view === 'pos' }]" @click="goPOS">收银</button>
          <template v-if="atLeast('MANAGER')">
            <button :class="['nav-btn', { active: view === 'products' || view === 'categories' }]" @click="goProducts">商品</button>
            <!-- 桌台/包间管理：台位的批量创建与删除集中在此，收银看板保持纯净 -->
            <button
              v-if="industryUi.navTables"
              :class="['nav-btn', { active: view === 'tables' }]"
              @click="view = 'tables'"
            >{{ industryUi.navTables }}</button>
            <button :class="['nav-btn', { active: view === 'supply' }]" @click="view = 'supply'">采购盘点</button>
            <button :class="['nav-btn', { active: view === 'member' }]" @click="view = 'member'">会员营销</button>
            <button :class="['nav-btn', { active: view === 'refund' }]" @click="gotoRefund()">退款</button>
            <button :class="['nav-btn', { active: view === 'order' }]" @click="gotoOrder(orderNo)">订单</button>
            <button :class="['nav-btn', { active: view === 'report' }]" @click="goReport">报表</button>
            <button :class="['nav-btn', { active: view === 'settings' }]" @click="goSettings">设置</button>
          </template>
        </nav>
        <div class="user">
          <button
            class="user-chip"
            :aria-expanded="userMenuOpen"
            aria-haspopup="menu"
            @click.stop="userMenuOpen = !userMenuOpen"
          >
            <span class="user-name">{{ currentUser }}</span>
            <span class="role-tag">{{ currentRole === 'ADMIN' ? '管理员' : currentRole === 'MANAGER' ? '店长' : '店员' }}</span>
            <Icon name="chevron-down" :size="ICON_SIZE.sm" class="user-caret" :class="{ open: userMenuOpen }" />
          </button>
          <!-- 账户菜单：交班为收银员日常动作（接班收银员或店长二次授权），锁机/改密/退出为个人操作 -->
          <div v-if="userMenuOpen" class="user-menu" role="menu">
            <button class="user-menu-item" role="menuitem" @click="pickMenu(goShift)">交班</button>
            <button class="user-menu-item" role="menuitem" @click="pickMenu(lockScreen)">锁机</button>
            <button class="user-menu-item" role="menuitem" @click="pickMenu(openPwdDialog)">修改密码</button>
            <div class="user-menu-sep" role="separator"></div>
            <button class="user-menu-item danger" role="menuitem" @click="pickMenu(doLogout)">退出登录</button>
          </div>
        </div>
      </header>

      <div v-if="banner" class="banner">{{ banner }}</div>

      <!-- 收银 -->
      <PosView v-if="view === 'pos'" />

      <!-- 商品与分类：页签与内容同处统一管理骨架，左对齐 -->
      <main v-else-if="view === 'products' || view === 'categories'" class="main-simple">
        <div class="page-head">
          <h2>{{ goodsTab === 'category' ? '分类管理' : '商品管理' }}</h2>
          <div class="seg" role="tablist">
            <button :class="['seg-btn', { active: goodsTab === 'list' }]" role="tab" :aria-selected="goodsTab === 'list'" @click="goodsTab = 'list'; view = 'products'">
              商品列表
            </button>
            <button :class="['seg-btn', { active: goodsTab === 'category' }]" role="tab" :aria-selected="goodsTab === 'category'" @click="goodsTab = 'category'; view = 'categories'">
              分类管理
            </button>
          </div>
        </div>
        <ProductAdmin v-if="goodsTab === 'list'" />
        <CategoryAdmin v-else />
      </main>

      <!-- 台位管理按行业拆成独立页面，互不复用 -->
      <RestaurantTablesView v-else-if="view === 'tables' && industry === 'RESTAURANT'" />
      <ChessRoomsView v-else-if="view === 'tables' && industry === 'BOARD_GAME'" />
      <WellnessRoomsView v-else-if="view === 'tables' && industry === 'WELLNESS'" />

      <!-- 采购与盘点 -->
      <SupplyAdmin v-else-if="view === 'supply'" :request-auth="requireSensitiveAuth" />

      <!-- 会员营销 -->
      <MemberAdmin v-else-if="view === 'member'" />

      <!-- 报表 -->
      <ReportView v-else-if="view === 'report' || view === 'shift'" />


      <!-- 退款 -->
      <RefundPanel
        v-else-if="view === 'refund'"
        :initial-order-no="refundOrderNo"
        :request-auth="requireSensitiveAuth"
        @detail="gotoOrder"
      />

      <!-- 订单详情 -->
      <OrderDetailPanel
        v-else-if="view === 'order'"
        :order-no="orderNo"
        @refund="gotoRefund"
        @search="gotoOrder"
      />

      <!-- 设置中心：后台管理式布局，左栏贴左全高，右侧内容独立滚动 -->
      <SettingsView v-else />

      <!-- 全局弹窗与状态提示 -->
      <DialogLayer />
    </div>
  </div>
</template>
