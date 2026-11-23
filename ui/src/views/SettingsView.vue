<script setup lang="ts">
import KitchenRoutePanel from '../components/KitchenRoutePanel.vue'
import Switch from '../components/Switch.vue'
import Icon from '../components/Icon.vue'
import { useAppContext } from '../composables/useAppContext'

const ctx = useAppContext()
const {
  ICON_SIZE,
  addUser,
  applyTheme,
  atLeast,
  channelDraft,
  clearLockCode,
  configDraft,
  configLabel,
  configOptions,
  confirmIndustry,
  credChannel,
  credDraft,
  credSaved,
  currentCredFields,
  doRetryPrint,
  formatMoney,
  gatewayModes,
  importBill,
  industries,
  industry,
  license,
  licenseStatusLabel,
  loadReconDiffs,
  lockCode,
  lockCodeDraft,
  pendingIndustry,
  pointsConfigs,
  printQueue,
  reconDate,
  reconDiffs,
  reconSummary,
  reconText,
  refreshLicense,
  removeCredential,
  saveConfig,
  saveCredential,
  saveLockCode,
  saveStoreName,
  search,
  settingsTab,
  storeNameDraft,
  storeProfile,
  systemConfigGroups,
  theme,
  toggleChannel,
  toggleConfigSwitch,
  toggleUser,
  uForm,
  users,
  visibleSettingsTabs,
} = ctx
</script>

<template>
      <main v-else class="admin-shell">
        <nav class="admin-side">
          <div class="admin-side-title">设置中心</div>
          <button
            v-for="t in visibleSettingsTabs"
            :key="t.id"
            :class="['settings-nav-btn', { active: settingsTab === t.id }]"
            @click="settingsTab = t.id"
          >
            <Icon :name="t.icon" :size="ICON_SIZE.md" />{{ t.label }}
          </button>
        </nav>

        <div class="admin-main settings-content">
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
              <p class="muted small">当前：{{ storeProfile?.industry_name || '未设置' }} · {{ storeProfile?.store_name || '未命名门店' }}。切换行业会新增模板分类并隐藏旧行业里没有商品的空分类；有商品的分类会保留。</p>
            </div>
            <div class="card wide">
              <h3>授权状态</h3>
              <div class="license-info">
                <p>授权状态：<strong>{{ licenseStatusLabel }}</strong></p>
                <p class="muted small">{{ license?.valid_until ? `有效期至 ${license.valid_until}` : '买断许可' }}</p>
                <p v-if="license?.trial_ends_at" class="muted small">试用期至 {{ license.trial_ends_at }}</p>
                <p class="muted small">授权状态以激活服务为准，可联网刷新。</p>
                <button class="btn" @click="refreshLicense"><Icon name="refresh" :size="ICON_SIZE.md" />刷新授权状态</button>
              </div>
            </div>
          </template>

          <!-- 外观 -->
          <div v-else-if="settingsTab === 'appearance'" class="card wide">
            <h3>外观主题</h3>
            <p class="muted small">主题切换入口仅在设置页，不占主界面。</p>
            <div class="theme-row">
              <button :class="['btn', { primary: theme === 'light' }]" @click="applyTheme('light')">浅色</button>
              <button :class="['btn', { primary: theme === 'dark' }]" @click="applyTheme('dark')">深色</button>
            </div>
          </div>

          <!-- 员工管理 -->
          <div v-else-if="settingsTab === 'staff'" class="card wide">
            <h3>员工管理</h3>
            <p class="muted small">仅管理员可见可用（user.manage 权限）。</p>
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

          <!-- 系统配置：按功能分区，同组同卡片；支付渠道归 payment 页、积分归 points 页，不重复 -->
          <template v-else-if="settingsTab === 'system'">
            <div v-for="g in systemConfigGroups" :key="g.title" class="card wide">
              <h3>{{ g.title }}</h3>
              <p class="muted small">{{ g.hint }}</p>
              <div v-for="c in g.items" :key="c.config_key" class="config-row config-row-stacked">
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
                  />
                  <button v-if="c.kind !== 'switch'" class="btn btn-sm" @click="saveConfig(c.config_key)">保存</button>
                </div>
              </div>
            </div>
            <div class="card wide">
              <h3>安全</h3>
              <p class="muted small">离岗锁屏、关闭窗口隐藏到托盘后需输入此码解锁（仅存本机）。</p>
              <div class="config-row config-row-stacked">
                <div class="config-label">
                  <strong>锁机码</strong>
                  <span class="muted small">设置后本机生效，不上传服务器；仅保存摘要，但本机文件仍可被读取，不能替代系统账户权限</span>
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
            <p class="muted small">收银台可用的支付方式；关闭后该渠道在收款区隐藏。</p>
            <div class="config-row" v-for="(v, k) in channelDraft" :key="k">
              <span class="config-key">{{ configLabel(k) }}</span>
              <Switch :model-value="v === '1'" :label="configLabel(k)" @update:model-value="toggleChannel(k, v)" />
            </div>
            <h4>商户凭据（存 Windows 凭据库，不落库不回显）</h4>
            <div class="search-row">
              <button :class="['btn', { primary: credChannel === 'WECHAT' }]" @click="credChannel = 'WECHAT'">微信支付</button>
              <button :class="['btn', { primary: credChannel === 'ALIPAY' }]" @click="credChannel = 'ALIPAY'">支付宝</button>
            </div>
            <div class="form-grid">
              <label v-for="f in currentCredFields" :key="f.key">
                {{ f.label }}
                <textarea v-if="f.multiline" v-model="credDraft[f.key]" class="input" rows="3" :placeholder="credSaved[f.key] ? '已配置（写入新值将覆盖）' : '粘贴 PEM 或密钥'"></textarea>
                <input v-else v-model="credDraft[f.key]" class="input" type="password" :placeholder="credSaved[f.key] ? '已配置（写入新值将覆盖）' : '输入后保存'" />
                <span class="cred-actions">
                  <button class="btn btn-sm primary" @click="saveCredential(f.key)">保存</button>
                  <button class="btn btn-sm ghost" @click="removeCredential(f.key)">删除</button>
                </span>
              </label>
            </div>
          </div>

          <!-- 积分配置 -->
          <div v-else-if="settingsTab === 'points'" class="card wide">
            <h3>积分配置</h3>
            <p class="muted small">识别会员收款后自动累计积分：按实付整元数 × 每元赠送积分计算，赠送金抵扣部分不计分；退款时按原单积分扣回。修改后对新订单生效。</p>
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
                <button v-if="c.kind !== 'switch'" class="btn btn-sm" @click="saveConfig(c.config_key)">保存</button>
              </div>
            </div>
            <div class="points-summary">
              <Icon name="star" :size="ICON_SIZE.md" />
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
            <p class="muted small">微信粘贴交易账单 CSV；支付宝粘贴对账单 CSV。差异只告警冻结，绝不自动抹平。</p>
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

          <!-- 打印设置：厨打档口路由 + 重试队列；不占收银首屏 -->
          <template v-else-if="settingsTab === 'print'">
            <div v-if="industry === 'RESTAURANT'" class="card wide">
              <h3>厨打档口路由</h3>
              <p class="muted small">按菜品分类把厨房小票分发到指定档口打印机；未配置的分类走系统配置里的主打印机。仅店长及以上可修改。</p>
              <KitchenRoutePanel :can-manage="atLeast('MANAGER')" />
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
        </div>
      </main>
</template>
