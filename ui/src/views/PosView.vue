<script setup lang="ts">
import RestaurantScene from '../components/RestaurantScene.vue'
import RoomBoard from '../components/RoomBoard.vue'
import BabyScene from '../components/BabyScene.vue'
import WellnessScene from '../components/WellnessScene.vue'
import Icon from '../components/Icon.vue'
import Checkbox from '../components/Checkbox.vue'
import { useAppContext } from '../composables/useAppContext'

const ctx = useAppContext()
const {
  ICON_SIZE,
  activeTable,
  addFromCatalog,
  addPaySplit,
  addToCart,
  atLeast,
  backToDiningTables,
  cart,
  cashTendered,
  changeQty,
  channelEnabled,
  clearActiveTable,
  clearCart,
  clearCoupon,
  couponCode,
  couponHint,
  couponRecoverable,
  sceneBoardStage,
  sceneCatalogOnly,
  effectivePrice,
  enterMixedMode,
  focusScan,
  formatMoney,
  industry,
  keyword,
  lastOrder,
  loadTechnicians,
  memberBalances,
  memberId,
  memberPriceOf,
  nativeMode,
  onDutyTechnicians,
  onScanKeydown,
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
  posCatalogBusy,
  posCategoryId,
  posProducts,
  posRootCategories,
  printAuto,
  printReceipt,
  productImageSrc,
  products,
  quote,
  rebindTableMember,
  removePaySplit,
  returnTableLine,
  scanCapture,
  search,
  searchBusy,
  selectPayChannel,
  selectPosCategory,
  selectSceneTable,
  setLineQty,
  showSceneError,
  submitCheckout,
  submitTableOrder,
  tableSentQty,
  view,
} = ctx
</script>

<template>
      <!-- 收银 -->
      <main
        v-if="view === 'pos'"
        class="pos-layout"
        :class="{ 'scene-board-stage': sceneBoardStage }"
      >
        <!-- 台位看板首页：餐饮桌台 / 棋牌·养生包间。未选中台位时首屏只显示看板，进入台位后再点单收银 -->
        <section v-if="sceneBoardStage" class="panel scene-home-panel">
          <RestaurantScene
            v-if="industry === 'RESTAURANT'"
            :active-table-id="null"
            :cart="[]"
            :member-id="memberId"
            :can-manage="atLeast('MANAGER')"
            @select="selectSceneTable"
            @cleared="clearActiveTable"
            @error="showSceneError"
          />
          <RoomBoard
            v-else-if="industry === 'BOARD_GAME' || industry === 'WELLNESS'"
            :scene="industry === 'BOARD_GAME' ? 'CHESS' : 'WELLNESS'"
            :title="industry === 'BOARD_GAME' ? '棋牌包间' : '养生包间'"
            :hint="industry === 'BOARD_GAME' ? '按分钟计时，最低 60 分钟；开台不再预收整小时房费' : '按分钟计时，进入包间即开台；房费由收银台按时长自动生成'"
            :theme="industry === 'BOARD_GAME' ? 'chess' : 'wellness'"
            :default-prefix="industry === 'BOARD_GAME' ? 'B' : 'W'"
            :active-table-id="null"
            :cart="[]"
            :member-id="memberId"
            :can-manage="atLeast('MANAGER')"
            @select="selectSceneTable"
            @cleared="clearActiveTable"
            @error="showSceneError"
          />
        </section>

        <!-- 点菜/收银阶段：餐饮已选桌台，或其他行业 -->
        <template v-else>
          <section class="panel product-panel">
            <!-- 进入台位后：当前台位信息条 + 返回看板入口 -->
            <div v-if="activeTable" class="scene-active-bar">
              <div>
                <strong>{{ activeTable.code }}</strong>
                <span class="muted small">
                  {{ activeTable.name }}
                  · {{ activeTable.status === 'OCCUPIED' ? (industry === 'RESTAURANT' ? '用餐中' : '使用中') : '未开台 · 点菜后自动开台' }}
                </span>
              </div>
              <button class="btn btn-sm ghost" @click="backToDiningTables">← 返回{{ industry === 'RESTAURANT' ? '桌台' : '包间' }}</button>
            </div>

            <BabyScene
              v-if="industry === 'BABY'"
              @add="addToCart"
              @error="showSceneError"
            />
            <WellnessScene
              v-if="industry === 'WELLNESS'"
              @add="addToCart"
              @error="showSceneError"
            />
            <div class="search-row scan-search">
            <input
              ref="scanCapture"
              v-model="keyword"
              class="input"
              placeholder="扫码 / 条码 / 商品名，自动联想"
              @keydown="onScanKeydown"
              @focus="focusScan"
            />
            <span v-if="searchBusy" class="scan-spinner" aria-label="搜索中"></span>
            <span v-else-if="products.length" class="scan-hint">{{ products.length }} 个候选 · 回车收第一条</span>
          </div>
          <div v-if="keyword.trim()" class="product-list">
            <div v-for="p in products" :key="p.sku_id" class="product-item" @click="addToCart(p)">
              <img v-if="productImageSrc(p)" :src="productImageSrc(p)!" class="product-thumb" alt="" />
              <span v-else class="product-thumb product-thumb-empty"></span>
              <div class="product-info">
                <strong>{{ p.spu_name }}</strong>
                <span class="muted small"> {{ p.barcode }}</span>
              </div>
              <div class="price-stack">
                <span class="price">¥{{ formatMoney(p.price_cents) }}</span>
                <span v-if="memberPriceOf(p)" class="price member">会员 ¥{{ formatMoney(memberPriceOf(p)!) }}</span>
              </div>
            </div>
            <div v-if="!products.length" class="empty">
              {{ searchBusy ? '正在查询…' : '没有匹配的商品' }}
            </div>
          </div>
          <!-- 能扫能点：分类页签 + 商品卡片，触摸屏不用键盘也能点单 -->
          <div v-else-if="!sceneCatalogOnly" class="pos-catalog">
            <div v-if="posRootCategories.length" class="pos-cat-row">
              <button
                v-for="c in posRootCategories"
                :key="c.category_id"
                :class="['pos-cat', { active: posCategoryId === c.category_id }]"
                @click="selectPosCategory(c.category_id)"
              >{{ c.name }}</button>
            </div>
            <div class="pos-grid">
              <button
                v-for="p in posProducts"
                :key="p.sku_id"
                class="pos-card"
                :class="{ weighed: p.is_weighed }"
                @click="addFromCatalog(p)"
              >
                <img v-if="productImageSrc(p)" :src="productImageSrc(p)!" class="pos-card-img" alt="" />
                <span v-else class="pos-card-img pos-card-empty"><Icon name="package" :size="ICON_SIZE.md" /></span>
                <span class="pos-card-name">{{ p.spu_name }}</span>
                <span class="pos-card-meta">{{ p.category_name }}<template v-if="p.is_weighed"> · 称重</template></span>
                <strong class="price">¥{{ formatMoney(p.price_cents) }}<span class="muted small">/{{ p.unit }}</span></strong>
              </button>
              <div v-if="!posProducts.length" class="empty pos-empty">
                {{ posCatalogBusy ? '正在装载商品…' : '该分类暂无在售商品，可切分类或直接扫码' }}
              </div>
            </div>
            <div class="pos-shortcuts">
              <span>F2 扫码</span><span>F3 会员</span><span>Ctrl+G 挂单</span><span>Ctrl+Q 取单</span><span>Ctrl+Z 清空</span>
            </div>
          </div>
        </section>

        <section class="panel cart-panel">
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
                <button @click="changeQty(i, -1)" aria-label="减少数量"><Icon name="minus" :size="ICON_SIZE.sm" /></button>
                <input
                  class="qty-input"
                  type="number"
                  min="1"
                  :value="l.qty"
                  aria-label="商品数量"
                  @change="setLineQty(i, ($event.target as HTMLInputElement).value)"
                />
                <button @click="changeQty(i, 1)" aria-label="增加数量"><Icon name="plus" :size="ICON_SIZE.sm" /></button>
              </div>
              <span class="price">¥{{ formatMoney(l.qty * effectivePrice(l)) }}</span>
            </div>
            <div v-if="!cart.length" class="empty">购物车为空</div>
          </div>
          <div class="cart-foot">
            <div class="pay-quick-row">
              <button class="btn pay-quick" :class="{ active: memberId != null }" @click="openMemberDialog">
                <Icon name="user" :size="ICON_SIZE.md" />{{ memberId != null ? '会员（已识别）' : '会员' }}
              </button>
              <button
                class="btn pay-quick"
                :class="{
                  active: !!couponCode && !couponRecoverable,
                  pending: couponRecoverable,
                }"
                @click="openCouponDialog"
              >
                <Icon name="ticket" :size="ICON_SIZE.md" />{{ couponCode ? `券 ${couponCode}` : '优惠券' }}
              </button>
              <button v-if="couponCode" class="btn ghost pay-quick" @click="clearCoupon"><Icon name="close" :size="ICON_SIZE.sm" />清除券</button>
            </div>
            <span v-if="couponHint" class="small" :class="couponRecoverable ? 'warning-text' : 'muted'">{{ couponHint }}</span>
            <span v-if="memberBalances" class="member-balance-inline">
              本金 ¥{{ formatMoney(memberBalances.principal_cents) }} · 赠金 ¥{{ formatMoney(memberBalances.bonus_cents) }} · 积分 {{ memberBalances.points }}
            </span>
            <div v-if="memberId != null" class="points-earn-inline">
              <Icon name="star" :size="ICON_SIZE.sm" />
              <span v-if="(quote?.points_earned ?? 0) > 0">本单预计累计 <strong>+{{ quote?.points_earned }}</strong> 积分</span>
              <span v-else>本单无积分累计（未达整元或积分功能已关闭）</span>
            </div>
            <!-- 空车 = 空闲态：不显示任何金额与支付方式（对齐 Square / Shopify POS 空闲屏模式） -->
            <template v-if="cart.length">
              <div class="cart-total">
                <div class="cart-total-copy">
                  <span>应收</span>
                  <span v-if="quote && quote.discount_cents > 0" class="cart-total-meta">
                    原价 ¥{{ formatMoney(quote.total_cents) }} · 优惠 ¥{{ formatMoney(quote.discount_cents) }}
                  </span>
                </div>
                <strong>¥{{ formatMoney(payableAmount) }}</strong>
              </div>
              <!-- 支付方式紧凑单行：金额自动=应收，选中后只补渠道必需信息 -->
              <div class="pay-methods">
              <button
                v-for="m in payMethods"
                :key="m.id"
                :class="['pay-method-btn', { active: payMode === 'simple' && payChannel === m.id }]"
                :aria-pressed="payMode === 'simple' && payChannel === m.id"
                @click="selectPayChannel(m.id)"
              >
                <Icon :name="m.icon" :size="ICON_SIZE.md" />{{ m.label }}
              </button>
              <button
                :class="['pay-method-btn', { active: payMode === 'mixed' }]"
                :aria-pressed="payMode === 'mixed'"
                @click="enterMixedMode"
              >
                <Icon name="plus" :size="ICON_SIZE.md" />组合
              </button>
            </div>
            <!-- 微信 / 支付宝：只露付款码输入，金额已自动带出 -->
            <div v-if="payMode === 'simple' && (payChannel === 'WECHAT' || payChannel === 'ALIPAY')" class="pay-panel">
              <template v-if="!nativeMode">
                <input
                  ref="payCodeInput"
                  v-model="payAuthCode"
                  class="input input-lg"
                  placeholder="扫描顾客付款码，回车即收款"
                  @keyup.enter="submitCheckout"
                />
                <p class="muted small">扫码枪扫顾客付款码即完成收款，金额已按应收自动带出。</p>
                <button class="btn ghost" @click="nativeMode = true"><Icon name="qr" :size="ICON_SIZE.sm" />改为顾客扫屏幕二维码</button>
              </template>
              <template v-else>
                <p class="pay-hint">
                  <Icon name="qr" :size="ICON_SIZE.md" />
                  点击「收款」后将展示二维码，顾客用{{ payChannel === 'WECHAT' ? '微信' : '支付宝' }}扫码支付 ¥{{ formatMoney(payableAmount) }}。
                </p>
                <button class="btn ghost" @click="nativeMode = false"><Icon name="undo" :size="ICON_SIZE.sm" />改回扫描顾客付款码</button>
              </template>
            </div>
            <!-- 现金：点击「现金」按钮弹出收银弹窗（实收 / 找零在弹窗内完成） -->
            <!-- 储值：需先识别会员，金额自动带出 -->
            <div v-else-if="payMode === 'simple' && payChannel === 'STORED_VALUE'" class="pay-panel">
              <p v-if="memberId == null" class="pay-hint warn"><Icon name="alert" :size="ICON_SIZE.md" />储值支付需先点击「会员」识别会员。</p>
              <p v-else class="pay-hint ok"><Icon name="wallet" :size="ICON_SIZE.md" />将从会员储值余额扣除 ¥{{ formatMoney(payShortfall > 0 ? payShortfall : 0) }}。</p>
            </div>
            <!-- 组合支付：渠道与金额同排，付款码/实收等补充项按需展开 -->
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
                ><Icon name="close" :size="ICON_SIZE.sm" /></button>
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
                <button class="link" @click="selectPayChannel('CASH')"><Icon name="undo" :size="ICON_SIZE.sm" />返回单笔支付</button>
              </div>
            </div>
            <div class="actions checkout-actions">
              <button class="btn" @click="openParkDialog"><Icon name="pause" :size="ICON_SIZE.md" />挂单</button>
              <button class="btn primary" :disabled="paying" @click="submitCheckout">
                <span v-if="paying" class="btn-spinner" aria-label="处理中"></span>
                <Icon v-else name="check" :size="ICON_SIZE.md" />{{ paying ? '处理中…' : '收款' }}
              </button>
            </div>
            </template>
            <!-- 空车引导：金额与支付选择只在有商品后出现 -->
            <div v-else class="cart-idle">
              <Icon name="scan" :size="ICON_SIZE.lg" />
              <span>扫码或选择商品后开始收款</span>
            </div>
            <button v-if="lastOrder && printAuto" class="btn ghost full" @click="printReceipt"><Icon name="print" :size="ICON_SIZE.md" />重打小票 {{ lastOrder }}</button>
          </div>
        </section>
        </template>
      </main>

</template>
