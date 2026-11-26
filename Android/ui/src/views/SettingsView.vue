<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import KitchenRoutePanel from '../components/KitchenRoutePanel.vue'
import TechnicianPanel from '../components/TechnicianPanel.vue'
import ReceiptPreview from '../components/ReceiptPreview.vue'
import Switch from '../components/Switch.vue'
import { useAppContext } from '../composables/useAppContext'

const ctx = useAppContext()
const {
  activateError, activateInSettings, activateToken, addUser, applyTheme, atLeast,
  channelDraft, clearLockCode, configDraft, configLabel,
  configOptions, confirmIndustry, doRetryPrint, formatMoney,
  gatewayModes, importBill, industries, industry,
  license, licenseStatusLabel, loadReconDiffs, loadTechnicians,
  lockCode, lockCodeDraft, openCredDialog, openOfficialSite, pendingIndustry, pointsConfigs,
  previewReceipt, printQueue, receiptPaperWidth, receiptPreview,
  receiptTemplateJson, reconDate, reconDiffs, reconSummary, trialDaysLeft,
  reconText, refreshLicense,
  configGroupChanged, saveConfigGroup, savePointsConfigs,
  saveLockCode, saveReceiptTemplate, saveStoreName, search,
  settingsTab, storeNameDraft, storeProfile, systemConfigGroups,
  theme, toggleChannel, toggleConfigSwitch, toggleUser,
  uForm, users, visibleSettingsTabs
} = ctx

/** 小票模板 JSON 高级编辑区默认折叠，默认展示所见即所得预览 */
const showTemplateJson = ref(false)
/** 系统配置分组顶部 tab：当前选中的分组标题（营业规则/支付时序/…/安全） */
const activeConfigGroup = ref('')
/** 分组标签 + 固定的「安全」卡片，前者随配置加载动态变化 */
const configGroupTabs = computed(() => [
  ...systemConfigGroups.value.map((g) => g.title),
  '安全',
])
// 配置加载完成或行业切换导致分组变化时同步选中项：
// 首次拿到真实分组前 tabs 只有「安全」，先不选；拿到后默认落到第一个业务分组。
watch(configGroupTabs, (tabs) => {
  const realGroups = tabs.filter((t) => t !== '安全')
  if (!activeConfigGroup.value && realGroups.length) {
    activeConfigGroup.value = realGroups[0]
  } else if (!tabs.includes(activeConfigGroup.value)) {
    activeConfigGroup.value = tabs[0] ?? ''
  }
}, { immediate: true })
</script>

<template>
      <main class="admin-shell">
        <!-- 设置中心一级导航：恢复左栏（后台管理式布局），与报表中心同构 -->
        <aside class="admin-side settings-side">
          <div class="admin-side-title">设置中心</div>
          <button
            v-for="t in visibleSettingsTabs"
            :key="t.id"
            :class="['report-nav-item', { active: settingsTab === t.id }]"
            @click="settingsTab = t.id"
          >
            {{ t.label }}
          </button>
        </aside>

        <div class="settings-content admin-main">
          <!-- 概览：门店信息 + 授权状态 -->
          <template v-if="settingsTab === 'overview'">
            <div class="card wide">
              <h3>门店信息</h3>
              <div class="form-grid">
                <label>店名
                  <input v-model="storeNameDraft" class="input" maxlength="30" placeholder="如 城南生活超市" />
                </label>
                <label>所属行业
                  <select v-model="pendingIndustry" class="input">
                    <option v-for="ind in industries" :key="ind.id" :value="ind.id">{{ ind.name }}</option>
                  </select>
                </label>
              </div>
              <div class="op-form">
                <button class="btn primary" @click="saveStoreName">保存店名</button>
                <button class="btn" @click="confirmIndustry">应用行业分类</button>
              </div>
            </div>
            <div class="card wide">
              <h3>授权状态</h3>
              <div class="license-info">
                <dl class="license-facts">
                  <div class="license-fact">
                    <dt>授权状态</dt>
                    <dd><strong>{{ licenseStatusLabel }}</strong></dd>
                  </div>
                  <div class="license-fact">
                    <dt>有效期</dt>
                    <dd>{{ license?.valid_until || '买断许可' }}</dd>
                  </div>
                  <div v-if="license?.trial_ends_at" class="license-fact">
                    <dt>试用期至</dt>
                    <dd>{{ license.trial_ends_at }}</dd>
                  </div>
                </dl>
                <div class="license-actions">
                  <button class="btn" @click="refreshLicense">刷新授权状态</button>
                </div>
                <!-- 试用期内可直接输入激活码激活，无需退出登录 -->
                <div v-if="license?.status === 'TRIAL_ACTIVE' || license?.status === 'GRACE_PERIOD' || license?.status === 'OFFLINE_DEGRADED'" class="license-activate">
                  <p class="muted small">
                    {{ license?.status === 'TRIAL_ACTIVE' ? `试用期剩余 ${trialDaysLeft} 天` : '当前授权已到期或即将到期' }}，输入激活码即可正式激活。
                    <button class="link" @click="openOfficialSite">免费获取激活码</button>
                  </p>
                  <div class="activate-code-row">
                    <input v-model="activateToken" class="input mono" placeholder="输入激活码" autocapitalize="characters" @keyup.enter="activateInSettings" />
                    <button class="btn primary" @click="activateInSettings">立即激活</button>
                  </div>
                  <p v-if="activateError" class="error">{{ activateError }}</p>
                </div>
              </div>
            </div>
          </template>

          <!-- 外观 -->
          <div v-else-if="settingsTab === 'appearance'" class="card wide">
            <h3>外观主题</h3>
            <div class="theme-row">
              <button :class="['btn', { primary: theme === 'light' }]" @click="applyTheme('light')">浅色</button>
              <button :class="['btn', { primary: theme === 'dark' }]" @click="applyTheme('dark')">深色</button>
            </div>
          </div>

          <!-- 员工管理 -->
          <div v-else-if="settingsTab === 'staff'" class="card wide">
            <h3>员工管理</h3>
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

          <!-- 系统配置：分组用顶部 tab 切换，同组同卡片；支付渠道归 payment 页、积分归 points 页，不重复 -->
          <template v-else-if="settingsTab === 'system'">
            <nav class="config-tabs" role="tablist" aria-label="系统配置分组">
              <button
                v-for="title in configGroupTabs"
                :key="title"
                role="tab"
                :aria-selected="activeConfigGroup === title"
                :class="['config-tab', { active: activeConfigGroup === title }]"
                @click="activeConfigGroup = title"
              >{{ title }}</button>
            </nav>
            <div
              v-for="g in systemConfigGroups"
              v-show="activeConfigGroup === g.title"
              :key="g.title"
              class="card wide"
            >
              <h3>{{ g.title }}</h3>
              <div v-for="c in g.items" :key="c.config_key" class="config-row config-row-stacked">
                <div class="config-label">
                  <strong>{{ c.label }}</strong>
                  <!-- 激活服务地址只保留输入框，不再用副标题解释 -->
                  <span v-if="c.config_key !== 'activation_endpoint'" class="muted small">{{ c.description }}</span>
                </div>
                <div class="config-control">
                  <Switch
                    v-if="c.kind === 'switch'"
                    :model-value="configDraft[c.config_key] === '1'"
                    :label="c.label"
                    @update:model-value="toggleConfigSwitch(c.config_key)"
                  />
                  <select
                    v-else-if="c.kind === 'select'"
                    v-model="configDraft[c.config_key]"
                    class="input"
                  >
                    <option v-for="o in configOptions(c.config_key)" :key="o.value" :value="o.value">
                      {{ o.label }}
                    </option>
                  </select>
                  <input
                    v-else
                    v-model="configDraft[c.config_key]"
                    class="input"
                    :type="c.kind === 'text' ? 'text' : 'number'"
                    :step="c.kind === 'decimal' ? '0.01' : '1'"
                    :placeholder="c.config_key === 'activation_endpoint' ? '留空使用内置服务' : undefined"
                  />
                </div>
              </div>
              <!-- 组级统一保存：改完一组点一次，不再每个字段一个保存按钮 -->
              <div class="config-group-actions">
                <button
                  class="btn btn-sm primary"
                  :disabled="!configGroupChanged(g.title)"
                  @click="saveConfigGroup(g.title)"
                >保存本组修改{{ configGroupChanged(g.title) ? `（${configGroupChanged(g.title)} 项）` : '' }}</button>
              </div>
            </div>
            <div v-show="activeConfigGroup === '安全'" class="card wide">
              <h3>安全</h3>
              <div class="config-row config-row-stacked">
                <div class="config-label">
                  <strong>锁机码</strong>
                </div>
                <div class="config-control">
                  <input
                    v-model="lockCodeDraft"
                    class="input"
                    type="password"
                    :placeholder="lockCode ? '已设置，输入新值覆盖' : '输入锁机码'"
                  />
                  <button class="btn btn-sm primary" @click="saveLockCode">保存</button>
                  <button v-if="lockCode" class="btn btn-sm ghost" @click="clearLockCode">清除</button>
                </div>
              </div>
            </div>
          </template>

          <!-- 支付设置 -->
          <div v-else-if="settingsTab === 'payment'" class="card wide">
            <h3>支付设置</h3>
            <div class="search-row">
              <span class="config-key">网关模式</span>
              <span class="muted small">微信：{{ gatewayModes.wechat === 'REAL' ? '真实渠道' : '本地确认（未配置凭据）' }}；支付宝：{{ gatewayModes.alipay === 'REAL' ? '真实渠道' : '本地确认（未配置凭据）' }}。保存凭据后需重启应用生效。</span>
            </div>
            <h4>支付渠道开关</h4>
            <div class="config-row" v-for="(v, k) in channelDraft" :key="k">
              <span class="config-key">{{ configLabel(k) }}</span>
              <Switch :model-value="v === '1'" :label="configLabel(k)" @update:model-value="toggleChannel(k, v)" />
            </div>
            <h4>商户凭据（存 Windows 凭据库，不落库不回显）</h4>
            <div class="cred-channel-row">
              <button class="btn" @click="openCredDialog('WECHAT')">微信支付凭据</button>
              <button class="btn" @click="openCredDialog('ALIPAY')">支付宝凭据</button>
            </div>
          </div>

          <!-- 积分配置 -->
          <div v-else-if="settingsTab === 'points'" class="card wide">
            <h3>积分配置</h3>
            <div v-for="c in pointsConfigs" :key="c.config_key" class="config-row config-row-stacked">
              <div class="config-label">
                <strong>{{ c.label }}</strong>
                <span class="muted small">{{ c.description }}</span>
              </div>
              <div class="config-control">
                <Switch
                  v-if="c.kind === 'switch'"
                  :model-value="configDraft[c.config_key] === '1'"
                  :label="c.label"
                  @update:model-value="toggleConfigSwitch(c.config_key)"
                />
                <input
                  v-else
                  v-model="configDraft[c.config_key]"
                  class="input"
                  type="number"
                  step="1"
                  min="0"
                />
              </div>
            </div>
            <div class="config-group-actions">
              <button class="btn btn-sm primary" @click="savePointsConfigs">保存积分配置</button>
            </div>
            <div class="points-summary">
              <span>当前规则：每消费 1 元累计 <strong>{{ configDraft['points_per_yuan'] ?? '1' }}</strong> 积分；{{ configDraft['points_enabled'] === '0' ? '积分累计已关闭' : '积分累计已开启' }}。</span>
            </div>
          </div>

          <!-- 渠道对账 -->
          <div v-else-if="settingsTab === 'recon'" class="card wide">
            <h3>渠道账单对账</h3>
            <div class="search-row">
              <input v-model="reconDate" class="input" type="date" />
              <button class="btn" @click="loadReconDiffs">查询差异</button>
            </div>
            <textarea v-model="reconText" class="input mono" rows="5" placeholder="粘贴渠道账单文本"></textarea>
            <div class="op-form">
              <button class="btn" @click="importBill('WECHAT')">导入微信账单</button>
              <button class="btn" @click="importBill('ALIPAY')">导入支付宝账单</button>
            </div>
            <div v-if="reconSummary" class="stats">
              <div class="stat"><span>匹配</span><strong>{{ reconSummary.matched }}</strong></div>
              <div class="stat"><span>渠道缺失</span><strong>{{ reconSummary.missing_remote }}</strong></div>
              <div class="stat"><span>本地缺失</span><strong>{{ reconSummary.missing_local }}</strong></div>
              <div class="stat"><span>金额不符</span><strong>{{ reconSummary.amount_mismatch }}</strong></div>
              <div class="stat"><span>状态不符</span><strong>{{ reconSummary.status_mismatch }}</strong></div>
            </div>
            <table v-if="reconDiffs.length" class="tbl">
              <thead><tr><th>渠道</th><th>差异类型</th><th>订单号</th><th class="num-cell">本地</th><th class="num-cell">渠道</th><th>说明</th></tr></thead>
              <tbody>
                <tr v-for="d in reconDiffs" :key="d.recon_id">
                  <td>{{ d.channel }}</td>
                  <td>{{ d.diff_type }}</td>
                  <td class="mono">{{ d.order_no || '—' }}</td>
                  <td class="num-cell">{{ formatMoney(d.local_amount_cents) }}</td>
                  <td class="num-cell">{{ formatMoney(d.channel_amount_cents) }}</td>
                  <td class="muted small">{{ d.detail }}</td>
                </tr>
              </tbody>
            </table>
          </div>

          <!-- 打印设置：小票样式 + 厨打档口路由统一在同一张卡片内分节展示 -->
          <template v-else-if="settingsTab === 'print'">
            <div class="card wide">
              <h3>打印机设置</h3>
              <h4 class="print-subtitle">小票设置与预览</h4>
              <div class="receipt-toolbar">
                <label>纸张宽度
                  <select v-model.number="receiptPaperWidth" class="input input-sm">
                    <option :value="58">58mm</option>
                    <option :value="80">80mm</option>
                  </select>
                </label>
                <button class="btn btn-sm" :disabled="!atLeast('MANAGER')" @click="saveReceiptTemplate">保存纸张设置</button>
                <button class="link" @click="showTemplateJson = !showTemplateJson">{{ showTemplateJson ? '收起模板源码' : '高级：编辑模板源码' }}</button>
              </div>
              <ReceiptPreview :store-name="storeProfile?.store_name || ''" :paper-width-mm="receiptPaperWidth" />
              <div v-if="showTemplateJson" class="receipt-advanced">
                <textarea v-model="receiptTemplateJson" class="input receipt-template-input" spellcheck="false" rows="12" placeholder="sections：header / items / summary / footer；变量用双大括号包裹，如 store_name、created_at、items、pay_total、table_code 等"></textarea>
                <div class="op-form">
                  <button class="btn btn-sm" :disabled="!atLeast('MANAGER')" @click="previewReceipt">刷新预览</button>
                  <button class="btn btn-sm primary" :disabled="!atLeast('MANAGER')" @click="saveReceiptTemplate">保存模板</button>
                </div>
                <pre v-if="receiptPreview" class="receipt-raw">{{ receiptPreview }}</pre>
              </div>
              <!-- 餐饮行业：档口路由并入本卡片，不再单独成块 -->
              <template v-if="industry === 'RESTAURANT'">
                <div class="print-section-divider"></div>
                <h4 class="print-subtitle">厨打档口路由</h4>
                <KitchenRoutePanel :can-manage="atLeast('MANAGER')" />
              </template>
            </div>
            <div class="card wide">
              <h3>打印重试队列</h3>
              <div v-if="printQueue.length" class="print-queue">
                <div v-for="no in printQueue" :key="no" class="print-queue-row">
                  <span>{{ no }}</span>
                  <button class="btn btn-sm" @click="doRetryPrint(no)">重打</button>
                </div>
              </div>
              <p v-else class="muted small">无待重打订单</p>
            </div>
          </template>

          <!-- 技师管理：仅养生行业（排班/在岗管理，从收银首屏移入设置中心） -->
          <template v-else-if="settingsTab === 'technicians'">
            <div class="card wide">
              <h3>技师排班与在岗</h3>
              <TechnicianPanel :can-manage="atLeast('MANAGER')" @changed="loadTechnicians" />
            </div>
          </template>
        </div>
      </main>
</template>
