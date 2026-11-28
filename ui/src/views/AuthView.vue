<script setup lang="ts">
import Icon from '../components/Icon.vue'
import SetupSteps from '../components/SetupSteps.vue'
import { useAppContext } from '../composables/useAppContext'
const ctx = useAppContext()
const {
  ICON_SIZE, view, onboarding, authStep, needAdmin, storeProfile, industries,
  pendingIndustry, storeNameDraft, activateToken, activateError,
  adminUser, adminPass,
  loginUser, loginPass, loginError, needsActivation,
  finishIndustrySetup, submitActivateOnline, openOfficialSite, enterTrial,
  submitAdmin, submitLogin,
} = ctx
</script>

<template>
    <div v-if="view === 'boot'" class="center-screen">
      <div class="spinner"></div>
      <p>正在初始化系统…</p>
    </div>

    <!-- 行业选择与门店设置（首次启动第 3 步） -->
    <div v-else-if="view === 'industry'" class="center-screen">
      <div class="card auth-card industry-card">
        <SetupSteps v-if="onboarding" :step="authStep" />
        <div class="brand">
          <img src="/logo.png" class="auth-logo" alt="贝来" />
          <div>
            <h1>贝来</h1>
          </div>
        </div>
        <h2>选择经营行业</h2>
        <div class="industry-list">
          <button
            v-for="ind in industries"
            :key="ind.id"
            :class="['industry-item', { active: pendingIndustry === ind.id }]"
            :aria-pressed="pendingIndustry === ind.id"
            @click="pendingIndustry = ind.id"
          >
            <strong>{{ ind.name }}</strong>
            <span class="muted small">{{ ind.description }}</span>
            <span class="muted small">默认单位：{{ ind.default_unit }}</span>
          </button>
        </div>
        <label class="field-label">
          店名
          <input v-model="storeNameDraft" class="input" placeholder="如 城南生活超市" maxlength="30" />
        </label>
        <button class="btn primary btn-lg" @click="finishIndustrySetup">
          {{ needsActivation ? '下一步：激活设备' : '进入收银' }}
        </button>
      </div>
    </div>

    <!-- 激活（首次启动第 4 步）：一个激活码输入框 + 试用入口 -->
    <div v-else-if="view === 'activate'" class="center-screen">
      <div class="card auth-card">
        <SetupSteps v-if="onboarding" :step="authStep" />
        <div class="brand">
          <img src="/logo.png" class="auth-logo" alt="贝来" />
          <div>
            <h1>贝来</h1>
          </div>
        </div>
        <h2>激活设备</h2>
        <p v-if="storeProfile?.industry_name" class="activate-context">
          当前行业：<strong>{{ storeProfile.industry_name }}</strong>
        </p>
        <label class="field-label">
          激活码
          <div class="activate-code-row">
            <input
              v-model="activateToken"
              class="input mono"
              placeholder="输入激活码后回车激活"
              autocapitalize="characters"
              @keyup.enter="submitActivateOnline"
            />
            <button class="btn primary" @click="submitActivateOnline">激活</button>
          </div>
        </label>
        <p v-if="activateError" class="error">{{ activateError }}</p>
        <div class="activate-actions">
          <button class="btn" @click="enterTrial">试用15天</button>
          <button class="btn" @click="openOfficialSite">免费获取激活码</button>
        </div>
        <button class="link setup-back" @click="view = 'industry'">返回上一步，重新选择行业</button>
      </div>
    </div>

    <!-- 登录：首次显示「设置管理员账号」，建号后回到登录页 -->
    <div v-else-if="view === 'login'" class="center-screen">
      <div class="card auth-card">
        <SetupSteps v-if="onboarding" :step="authStep" />
        <div class="auth-brand">
          <img src="/logo.png" class="auth-logo" alt="贝来" />
          <h1>{{ needAdmin ? '设置管理员账号' : onboarding ? '登录' : '欢迎回来' }}</h1>
        </div>
        <template v-if="needAdmin">
          <label class="field">
            <span class="field-label">用户名</span>
            <input v-model="adminUser" class="input input-lg" placeholder="默认 admin" @keyup.enter="submitAdmin" />
          </label>
          <label class="field">
            <span class="field-label">密码</span>
            <input v-model="adminPass" type="password" class="input input-lg" placeholder="至少 8 位，含字母和数字" @keyup.enter="submitAdmin" />
          </label>
          <p v-if="loginError" class="form-error"><Icon name="alert" :size="ICON_SIZE.sm" />{{ loginError }}</p>
          <button class="btn primary btn-lg" @click="submitAdmin">创建管理员账号</button>
        </template>
        <template v-else>
          <label class="field">
            <span class="field-label">用户名</span>
            <input v-model="loginUser" class="input input-lg" placeholder="用户名" @keyup.enter="submitLogin" />
          </label>
          <label class="field">
            <span class="field-label">密码</span>
            <input v-model="loginPass" type="password" class="input input-lg" placeholder="密码" @keyup.enter="submitLogin" />
          </label>
          <p v-if="loginError" class="form-error"><Icon name="alert" :size="ICON_SIZE.sm" />{{ loginError }}</p>
          <button class="btn primary btn-lg" @click="submitLogin">登录</button>
        </template>
      </div>
    </div>
</template>
