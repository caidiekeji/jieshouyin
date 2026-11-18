<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { api, formatMoney, today, type LicenseSnapshot, type ProductHit, type CartLine } from './api'
import Icon from './components/Icon.vue'
import ProductAdmin from './components/ProductAdmin.vue'
import SupplyAdmin from './components/SupplyAdmin.vue'
import MemberAdmin from './components/MemberAdmin.vue'
import { ICON_SIZE, MS_PER_DAY, TOAST_DURATION_MS } from './constants'

type View = 'boot' | 'activate' | 'login' | 'pos' | 'products' | 'supply' | 'member' | 'parked' | 'report' | 'settings'
const view = ref<View>('boot')
const bootError = ref('')
const license = ref<LicenseSnapshot | null>(null)
const needAdmin = ref(false)
const adminUser = ref('admin')
const adminPass = ref('')
const loginUser = ref('')
const loginPass = ref('')
const loginError = ref('')
const activateToken = ref('')
const activateError = ref('')
const renewMode = ref(false)
const theme = ref<'light' | 'dark'>('light')
const currentUser = ref('')

// 收银状态
const keyword = ref('')
const products = ref<ProductHit[]>([])
const cart = ref<(CartLine & { name: string })[]>([])
const cashTendered = ref('')
const payChannel = ref('CASH')
const lastOrder = ref('')
const toast = ref('')
const parked = ref<unknown[]>([])

const banner = computed(() => {
  if (!license.value) return ''
  if (license.value.status === 'TRIAL_ACTIVE') {
    const ends = license.value.trial_ends_at
    if (!ends) return '试用期进行中'
    const days = Math.max(
      1,
      Math.ceil((new Date(ends).getTime() - Date.now()) / MS_PER_DAY)
    )
    return `试用期剩余 ${days} 天，到期后需激活`
  }
  if (license.value.status === 'GRACE_PERIOD') return '授权宽限期内，请尽快续期'
  if (license.value.status === 'OFFLINE_DEGRADED') return '授权已到期，前台收银仍可用，后台管理受限'
  return ''
})

const subtotal = computed(() => cart.value.reduce((s, l) => s + l.qty * l.unit_price_cents, 0))

function showToast(msg: string) {
  toast.value = msg
  setTimeout(() => (toast.value = ''), TOAST_DURATION_MS)
}

onMounted(async () => {
  try {
    license.value = await api.bootstrap()
    if (license.value.status === 'UNACTIVATED') {
      view.value = 'activate'
    } else {
      view.value = 'login'
      // user_account 为空 → 强制初始化管理员（§12.3），无需先登录失败
      needAdmin.value = !(await api.hasUsers())
    }
  } catch (e) {
    bootError.value = String(e)
  }
})

async function submitActivate() {
  activateError.value = ''
  try {
    license.value = renewMode.value
      ? await api.renew(activateToken.value)
      : await api.activate(activateToken.value)
    if (license.value.status !== 'UNACTIVATED') {
      view.value = 'login'
      showToast('激活成功')
    }
  } catch (e) {
    activateError.value = String(e)
  }
}

async function submitAdmin() {
  try {
    await api.initAdmin(adminUser.value, adminPass.value)
    needAdmin.value = false
    showToast('管理员已创建')
  } catch (e) {
    activateError.value = String(e)
  }
}

async function submitLogin() {
  loginError.value = ''
  try {
    const s = await api.login(loginUser.value, loginPass.value)
    currentUser.value = loginUser.value
    if (s.role_name === 'ADMIN' || s.role_name === 'MANAGER') {
      // 管理员/店长可进后台，此处统一进入收银
    }
    view.value = 'pos'
  } catch (e) {
    const msg = String(e)
    loginError.value = msg
    if (msg.includes('密码') || msg.includes('argon')) {
      // 首次初始化引导
      needAdmin.value = true
    }
  }
}

async function search() {
  if (!keyword.value.trim()) {
    products.value = []
    return
  }
  try {
    products.value = await api.lookup(keyword.value.trim())
  } catch (e) {
    showToast(String(e))
  }
}

function addToCart(p: ProductHit) {
  const existing = cart.value.find((l) => l.sku_id === p.sku_id)
  if (existing) existing.qty += 1
  else cart.value.push({ sku_id: p.sku_id, qty: 1, unit_price_cents: p.price_cents, name: p.spu_name })
  keyword.value = ''
  products.value = []
}

function changeQty(index: number, delta: number) {
  const line = cart.value[index]
  line.qty += delta
  if (line.qty <= 0) cart.value.splice(index, 1)
}

async function submitCheckout() {
  try {
    const tendered = Number(cashTendered.value || '0')
    const payments = [
      { channel: payChannel.value, amount_cents: subtotal.value, tendered_cents: tendered, change_cents: tendered - subtotal.value },
    ]
    const r = await api.checkout(cart.value, payments)
    lastOrder.value = r.order_no
    cart.value = []
    cashTendered.value = ''
    showToast(`收银成功：${r.order_no}`)
  } catch (e) {
    showToast(String(e))
  }
}

async function park() {
  try {
    await api.parkCart(JSON.stringify(cart.value), null)
    cart.value = []
    showToast('已挂单')
  } catch (e) {
    showToast(String(e))
  }
}

async function loadParked() {
  try {
    parked.value = await api.listParked()
    view.value = 'parked'
  } catch (e) {
    showToast(String(e))
  }
}

async function resume(parkId: number) {
  try {
    const json = await api.resumeCart(parkId)
    cart.value = JSON.parse(json)
    view.value = 'pos'
    showToast('已取单，促销将现场重算')
  } catch (e) {
    showToast(String(e))
  }
}

async function printReceipt() {
  if (!lastOrder.value) return
  try {
    const r = await api.printReceipt(lastOrder.value)
    showToast(r)
  } catch (e) {
    showToast(String(e))
  }
}

const reportDate = ref(today())
const report = ref<{ order_count: number; total_sales_cents: number; refund_cents: number; discount_cents: number } | null>(null)
async function loadReport() {
  try {
    report.value = await api.dailyReport(reportDate.value)
  } catch (e) {
    showToast(String(e))
  }
}

// ===== 交班汇总 =====
interface ShiftRow { cashier_id: number; order_count: number; total_cents: number; pay_cents: number; cash_net_cents: number }
const shiftId = ref(1)
const shiftRows = ref<ShiftRow[]>([])
async function loadShift() {
  try {
    shiftRows.value = (await api.shiftReport(shiftId.value)) as ShiftRow[]
  } catch (e) {
    showToast(String(e))
  }
}

// ===== 订单作废 / 退货 =====
const opOrderNo = ref('')
const opItems = ref<{ item_id: number; product_name: string; qty: number; returned_qty: number; unit_price_cents: number }[]>([])
const opQty = ref<Record<number, string>>({})
const opMode = ref<'void' | 'refund'>('refund')

async function loadOrderItems() {
  const no = opOrderNo.value.trim()
  if (!no) return
  try {
    opItems.value = await api.getOrderItems(no)
  } catch (e) {
    showToast(String(e))
  }
}

async function doVoid() {
  try {
    await api.voidOrder(opOrderNo.value.trim())
    showToast('订单已作废')
    opItems.value = []
    opOrderNo.value = ''
  } catch (e) {
    showToast(String(e))
  }
}

async function doRefund() {
  const items = opItems.value
    .filter((r) => opQty.value[r.item_id] && parseInt(opQty.value[r.item_id], 10) > 0)
    .map((r) => ({ order_item_id: r.item_id, qty: parseInt(opQty.value[r.item_id], 10) }))
  if (!items.length) {
    showToast('请填写退货数量')
    return
  }
  try {
    const total = await api.refundOrder(opOrderNo.value.trim(), items)
    showToast(`退货成功，退款 ${formatMoney(total)}`)
    opItems.value = []
    opOrderNo.value = ''
  } catch (e) {
    showToast(String(e))
  }
}

// ===== 员工管理 =====
interface UserRowUI { user_id: number; username: string; role_name: string; status: number }
const users = ref<UserRowUI[]>([])
const showUserMgmt = ref(false)
const uForm = ref({ username: '', password: '', roleName: 'CASHIER' })

async function loadUsers() {
  try {
    users.value = await api.listUsers()
  } catch (e) {
    showToast(String(e))
  }
}

async function addUser() {
  try {
    await api.createUser(uForm.value.username.trim(), uForm.value.password, uForm.value.roleName)
    uForm.value = { username: '', password: '', roleName: 'CASHIER' }
    showToast('员工账号已创建')
    await loadUsers()
  } catch (e) {
    showToast(String(e))
  }
}

async function toggleUser(u: UserRowUI) {
  try {
    await api.setUserStatus(u.user_id, u.status === 1 ? 0 : 1)
    await loadUsers()
  } catch (e) {
    showToast(String(e))
  }
}

function applyTheme(t: 'light' | 'dark') {
  theme.value = t
  document.documentElement.dataset.theme = t
  localStorage.setItem('checkoutr-theme', t)
}
onMounted(() => {
  const saved = localStorage.getItem('checkoutr-theme') as 'light' | 'dark' | null
  if (saved) applyTheme(saved)
})
function goPOS() { view.value = 'pos' }
function goProducts() { view.value = 'products' }
function goParked() { loadParked() }
function goReport() { view.value = 'report'; loadReport(); loadShift() }
function goSettings() { view.value = 'settings' }
function doLogout() { api.logout(); view.value = 'login'; currentUser.value = '' }
</script>

<template>
  <div class="app-root">
    <div v-if="bootError" class="fatal">启动失败：{{ bootError }}</div>

    <!-- 启动中 -->
    <div v-else-if="view === 'boot'" class="center-screen">
      <div class="spinner"></div>
      <p>正在初始化系统…</p>
    </div>

    <!-- 激活 -->
    <div v-else-if="view === 'activate'" class="center-screen">
      <div class="card auth-card">
        <div class="brand">
          <div class="brand-mark"><Icon name="scan" :size="ICON_SIZE.xxl" /></div>
          <div>
            <h1>Checkout-R</h1>
            <p class="muted">离线优先收银系统</p>
          </div>
        </div>
        <h2>{{ renewMode ? '导入续期码' : '设备激活' }}</h2>
        <p class="muted small">激活为纯离线过程，全程不发起网络请求。</p>
        <textarea v-model="activateToken" rows="5" placeholder="粘贴激活码（Base64URL.签名）" class="input mono"></textarea>
        <p v-if="activateError" class="error">{{ activateError }}</p>
        <button class="btn primary" @click="submitActivate">{{ renewMode ? '续期' : '激活' }}</button>
        <button class="btn ghost" @click="renewMode = !renewMode">{{ renewMode ? '返回激活' : '已有授权，导入续期码' }}</button>
        <div v-if="needAdmin" class="inline-form">
          <h3>初始化管理员</h3>
          <input v-model="adminUser" class="input" placeholder="用户名" />
          <input v-model="adminPass" type="password" class="input" placeholder="密码（≥8位，含字母数字）" />
          <button class="btn" @click="submitAdmin">创建管理员</button>
        </div>
      </div>
    </div>

    <!-- 登录 -->
    <div v-else-if="view === 'login'" class="center-screen">
      <div class="card auth-card">
        <div class="brand">
          <div class="brand-mark"><Icon name="user" :size="ICON_SIZE.xxl" /></div>
          <div><h1>登录</h1><p class="muted">输入账号密码开始收银</p></div>
        </div>
        <input v-model="loginUser" class="input" placeholder="用户名" />
        <input v-model="loginPass" type="password" class="input" placeholder="密码" @keyup.enter="submitLogin" />
        <p v-if="loginError" class="error">{{ loginError }}</p>
        <button class="btn primary" @click="submitLogin">登录</button>
        <div v-if="needAdmin" class="inline-form">
          <h3>初始化管理员</h3>
          <p class="muted small">首次使用需创建管理员账号，用户名默认 admin</p>
          <input v-model="adminUser" class="input" placeholder="管理员用户名" />
          <input v-model="adminPass" type="password" class="input" placeholder="管理员密码（≥8位，含字母数字）" />
          <button class="btn" @click="submitAdmin">创建管理员</button>
        </div>
      </div>
    </div>

    <!-- 主界面 -->
    <div v-else class="shell">
      <header class="topbar">
        <div class="logo"><Icon name="scan" :size="ICON_SIZE.xl" /><span>Checkout-R</span></div>
        <nav class="nav">
          <button :class="['nav-btn', { active: view === 'pos' }]" @click="goPOS"><Icon name="cart" :size="ICON_SIZE.lg" />收银</button>
          <button :class="['nav-btn', { active: view === 'products' }]" @click="goProducts"><Icon name="box" :size="ICON_SIZE.lg" />商品</button>
          <button :class="['nav-btn', { active: view === 'supply' }]" @click="view = 'supply'"><Icon name="park" :size="ICON_SIZE.lg" />采购盘点</button>
          <button :class="['nav-btn', { active: view === 'member' }]" @click="view = 'member'"><Icon name="users" :size="ICON_SIZE.lg" />会员营销</button>
          <button :class="['nav-btn', { active: view === 'parked' }]" @click="goParked"><Icon name="park" :size="ICON_SIZE.lg" />挂单</button>
          <button :class="['nav-btn', { active: view === 'report' }]" @click="goReport"><Icon name="report" :size="ICON_SIZE.lg" />报表</button>
          <button :class="['nav-btn', { active: view === 'settings' }]" @click="goSettings"><Icon name="settings" :size="ICON_SIZE.lg" />设置</button>
        </nav>
        <div class="user"><Icon name="user" :size="ICON_SIZE.md" /><span>{{ currentUser }}</span><button class="link" @click="doLogout">退出</button></div>
      </header>

      <div v-if="banner" class="banner">{{ banner }}</div>

      <!-- 收银 -->
      <main v-if="view === 'pos'" class="pos-layout">
        <section class="panel product-panel">
          <div class="search-row">
            <input v-model="keyword" class="input" placeholder="扫码 / 条码 / 商品名" @keyup.enter="search" />
            <button class="btn" @click="search"><Icon name="scan" :size="ICON_SIZE.md" />识别</button>
          </div>
          <div class="product-list">
            <div v-for="p in products" :key="p.sku_id" class="product-item" @click="addToCart(p)">
              <div><strong>{{ p.spu_name }}</strong><span class="muted small"> {{ p.barcode }}</span></div>
              <span class="price">¥{{ formatMoney(p.price_cents) }}</span>
            </div>
            <div v-if="!products.length" class="empty">输入条码或商品名，回车识别</div>
          </div>
        </section>

        <section class="panel cart-panel">
          <h3>购物车 <span class="badge">{{ cart.length }}</span></h3>
          <div class="cart-list">
            <div v-for="(l, i) in cart" :key="l.sku_id" class="cart-line">
              <div class="line-name">{{ l.name || `SKU ${l.sku_id}` }}</div>
              <div class="qty-ctrl">
                <button @click="changeQty(i, -1)"><Icon name="minus" :size="ICON_SIZE.sm" /></button>
                <span>{{ l.qty }}</span>
                <button @click="changeQty(i, 1)"><Icon name="plus" :size="ICON_SIZE.sm" /></button>
              </div>
              <span class="price">¥{{ formatMoney(l.qty * l.unit_price_cents) }}</span>
            </div>
            <div v-if="!cart.length" class="empty">购物车为空</div>
          </div>
          <div class="cart-foot">
            <div class="total"><span>合计</span><strong>¥{{ formatMoney(subtotal) }}</strong></div>
            <div class="pay-row">
              <select v-model="payChannel" class="input">
                <option value="CASH">现金</option>
                <option value="STORED_VALUE">储值</option>
                <option value="WECHAT">微信</option>
                <option value="ALIPAY">支付宝</option>
              </select>
              <input v-model="cashTendered" class="input" placeholder="收钞金额" type="number" />
            </div>
            <div class="actions">
              <button class="btn" @click="park">挂单</button>
              <button class="btn primary" @click="submitCheckout"><Icon name="check" :size="ICON_SIZE.md" />收款</button>
            </div>
            <button v-if="lastOrder" class="btn ghost full" @click="printReceipt"><Icon name="print" :size="ICON_SIZE.md" />重打小票 {{ lastOrder }}</button>
          </div>
        </section>
      </main>

      <!-- 商品管理 -->
      <ProductAdmin v-else-if="view === 'products'" />

      <!-- 采购与盘点 -->
      <SupplyAdmin v-else-if="view === 'supply'" />

      <!-- 会员营销 -->
      <MemberAdmin v-else-if="view === 'member'" />

      <!-- 挂单 -->
      <main v-else-if="view === 'parked'" class="main-simple">
        <div class="card wide">
          <h3>待取挂单</h3>
          <div v-for="p in parked" :key="String((p as any).park_id)" class="row-item">
            <span>单号 #{{ (p as any).park_id }} · {{ (p as any).created_at }}</span>
            <button class="btn" @click="resume((p as any).park_id)">取单</button>
          </div>
          <div v-if="!parked.length" class="empty">暂无挂单</div>
        </div>
      </main>

      <!-- 报表 -->
      <main v-else-if="view === 'report'" class="main-simple">
        <div class="card wide">
          <h3>日结报表</h3>
          <div class="search-row">
            <input v-model="reportDate" class="input" type="date" />
            <button class="btn" @click="loadReport">查询</button>
          </div>
          <div v-if="report" class="stats">
            <div class="stat"><span>订单数</span><strong>{{ report.order_count }}</strong></div>
            <div class="stat"><span>销售额</span><strong>¥{{ formatMoney(report.total_sales_cents) }}</strong></div>
            <div class="stat"><span>退款额</span><strong>¥{{ formatMoney(report.refund_cents) }}</strong></div>
            <div class="stat"><span>优惠额</span><strong>¥{{ formatMoney(report.discount_cents) }}</strong></div>
          </div>
        </div>

        <div class="card wide">
          <h3>交班汇总</h3>
          <div class="search-row">
            <input v-model.number="shiftId" class="input" type="number" placeholder="班次 ID" />
            <button class="btn" @click="loadShift">查询</button>
          </div>
          <table v-if="shiftRows.length" class="tbl">
            <thead><tr><th>收银员</th><th>订单数</th><th>应收</th><th>实收</th><th>现金净额</th></tr></thead>
            <tbody>
              <tr v-for="r in shiftRows" :key="r.cashier_id">
                <td>{{ r.cashier_id }}</td>
                <td>{{ r.order_count }}</td>
                <td>{{ formatMoney(r.total_cents) }}</td>
                <td>{{ formatMoney(r.pay_cents) }}</td>
                <td>{{ formatMoney(r.cash_net_cents) }}</td>
              </tr>
            </tbody>
          </table>
        </div>

        <div class="card wide">
          <h3>订单操作（作废 / 退货）</h3>
          <div class="search-row">
            <input v-model="opOrderNo" class="input" placeholder="输入订单号" />
            <button class="btn" @click="loadOrderItems">查询明细</button>
          </div>
          <table v-if="opItems.length" class="tbl">
            <thead><tr><th>商品</th><th>数量</th><th>已退</th><th>单价</th><th>本次退货数</th></tr></thead>
            <tbody>
              <tr v-for="r in opItems" :key="r.item_id">
                <td>{{ r.product_name }}</td>
                <td>{{ r.qty }}</td>
                <td>{{ r.returned_qty }}</td>
                <td>{{ formatMoney(r.unit_price_cents) }}</td>
                <td><input v-model="opQty[r.item_id]" class="input input-sm" type="number" min="0" :max="r.qty - r.returned_qty" /></td>
              </tr>
            </tbody>
          </table>
          <div v-if="opItems.length" class="op-form">
            <button class="btn" @click="doVoid">作废订单</button>
            <button class="btn primary" @click="doRefund">退货退款</button>
          </div>
          <p class="muted small">作废：仅待支付单或当日现金单；扫码已支付订单只能退货退款。退货受角色限额约束。</p>
        </div>
      </main>

      <!-- 设置 -->
      <main v-else class="main-simple">
        <div class="card wide">
          <h3>设置</h3>
          <p class="muted small">外观主题（主题切换入口仅在设置页，不占主界面）</p>
          <div class="theme-row">
            <button :class="['btn', { primary: theme === 'light' }]" @click="applyTheme('light')">浅色</button>
            <button :class="['btn', { primary: theme === 'dark' }]" @click="applyTheme('dark')">深色</button>
          </div>
          <div class="license-info">
            <p>授权状态：<strong>{{ license?.status }}</strong></p>
            <p class="muted small">{{ license?.valid_until ? `有效期至 ${license.valid_until}` : '买断许可' }}</p>
            <p v-if="license?.trial_ends_at" class="muted small">试用期至 {{ license.trial_ends_at }}</p>
          </div>
          <div class="user-mgmt">
            <p class="muted small">员工管理（仅管理员可见可用，user.manage 权限）</p>
            <button class="btn" @click="showUserMgmt = !showUserMgmt; showUserMgmt && loadUsers()"><Icon name="users" :size="ICON_SIZE.md" />展开员工管理</button>
            <div v-if="showUserMgmt">
              <table class="tbl">
                <thead><tr><th>用户名</th><th>角色</th><th>状态</th><th>操作</th></tr></thead>
                <tbody>
                  <tr v-for="u in users" :key="u.user_id">
                    <td>{{ u.username }}</td>
                    <td>{{ u.role_name }}</td>
                    <td>{{ u.status === 1 ? '启用' : '停用' }}</td>
                    <td><button class="btn btn-sm ghost" @click="toggleUser(u)">{{ u.status === 1 ? '停用' : '启用' }}</button></td>
                  </tr>
                </tbody>
              </table>
              <div class="op-form">
                <input v-model="uForm.username" class="input" placeholder="用户名" />
                <input v-model="uForm.password" class="input" type="password" placeholder="密码（≥8位含字母数字）" />
                <select v-model="uForm.roleName" class="input">
                  <option value="CASHIER">收银员</option>
                  <option value="MANAGER">店长</option>
                  <option value="ADMIN">管理员</option>
                </select>
                <button class="btn" @click="addUser">创建账号</button>
              </div>
            </div>
          </div>
        </div>
      </main>

      <div v-if="toast" class="toast">{{ toast }}</div>
    </div>
  </div>
</template>
