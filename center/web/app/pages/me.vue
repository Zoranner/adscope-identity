<script setup lang="ts">
import { Database, KeyRound, LogOut, RefreshCw, Save, UserRound } from 'lucide-vue-next'
import { nullable } from '~/utils/forms'

const {
  tokenReady,
  loading,
  profile,
  loadToken,
  loadMe,
  logout,
  updateContact,
  changePassword,
} = useUserApi()
const { setStatus } = useUserStatus()

const contactForm = reactive({
  email: '',
  mobile: '',
})
const passwordForm = reactive({
  currentPassword: '',
  newPassword: '',
  confirmPassword: '',
})

const statusLabel = computed(() => (profile.value?.status === 'active' ? '启用' : '禁用'))
const passwordMismatch = computed(
  () =>
    passwordForm.newPassword.length > 0 &&
    passwordForm.confirmPassword.length > 0 &&
    passwordForm.newPassword !== passwordForm.confirmPassword,
)

watch(
  profile,
  (nextProfile) => {
    contactForm.email = nextProfile?.email ?? ''
    contactForm.mobile = nextProfile?.mobile ?? ''
  },
  { immediate: true },
)

onMounted(async () => {
  const storedToken = loadToken()
  if (!storedToken) {
    await navigateTo('/login', { replace: true })
    return
  }
  const valid = await loadMe(false)
  if (!valid) {
    await navigateTo('/login', { replace: true })
  }
})

async function refreshProfile() {
  await loadMe()
}

async function saveContact() {
  await updateContact(nullable(contactForm.email), nullable(contactForm.mobile))
}

async function savePassword() {
  if (passwordMismatch.value) {
    setStatus('两次输入的新密码不一致', true)
    return
  }
  const ok = await changePassword(passwordForm.currentPassword, passwordForm.newPassword)
  if (ok) {
    passwordForm.currentPassword = ''
    passwordForm.newPassword = ''
    passwordForm.confirmPassword = ''
  }
}

async function exitUser() {
  await logout()
  await navigateTo('/login', { replace: true })
}
</script>

<template>
  <div class="app-shell">
    <UserStatusLine />

    <header class="topbar">
      <NuxtLink class="brand" to="/me">
        <span class="brand-mark">
          <Database :size="22" />
        </span>
        <span class="brand-copy">
          <span class="brand-title">ADSS Center</span>
          <span class="brand-subtitle">用户自助入口</span>
        </span>
      </NuxtLink>

      <div class="topbar-actions">
        <button class="icon-button" title="刷新" :disabled="loading || !tokenReady" @click="refreshProfile">
          <RefreshCw :size="17" />
        </button>
        <button class="secondary-button" @click="exitUser">
          <LogOut :size="16" />
          退出
        </button>
      </div>
    </header>

    <main class="content user-content">
      <section class="view-header">
        <div>
          <h1>我的账号</h1>
          <p>查看中心账号资料，维护邮箱、手机和登录密码。</p>
        </div>
      </section>

      <section v-if="profile" class="user-grid">
        <section class="panel">
          <div class="panel-header">
            <div class="panel-title">
              <UserRound :size="18" />
              <h2>账号资料</h2>
            </div>
          </div>
          <dl class="profile-list">
            <div>
              <dt>显示名</dt>
              <dd>
                <strong>{{ profile.display_name }}</strong>
                <span class="badge" :class="{ danger: profile.status === 'disabled' }">{{ statusLabel }}</span>
              </dd>
            </div>
            <div>
              <dt>工号</dt>
              <dd>{{ profile.employee_id }}</dd>
            </div>
            <div>
              <dt>用户名</dt>
              <dd>{{ profile.username }}</dd>
            </div>
            <div>
              <dt>所属 OU</dt>
              <dd>{{ profile.organizational_unit_id }}</dd>
            </div>
          </dl>
        </section>

        <section class="panel">
          <div class="panel-header">
            <div class="panel-title">
              <Save :size="18" />
              <h2>联系方式</h2>
            </div>
          </div>
          <form class="form" @submit.prevent="saveContact">
            <div class="form-row">
              <div class="field">
                <label>邮箱</label>
                <input v-model="contactForm.email" type="email" autocomplete="email" />
              </div>
              <div class="field">
                <label>手机</label>
                <input v-model="contactForm.mobile" autocomplete="tel" />
              </div>
            </div>
            <div class="form-actions">
              <button class="primary-button" :disabled="loading || !tokenReady">
                <Save :size="16" />
                保存联系方式
              </button>
            </div>
          </form>
        </section>

        <section class="panel">
          <div class="panel-header">
            <div class="panel-title">
              <KeyRound :size="18" />
              <h2>修改密码</h2>
            </div>
          </div>
          <form class="form" @submit.prevent="savePassword">
            <div class="form-row">
              <div class="field">
                <label>当前密码</label>
                <input
                  v-model="passwordForm.currentPassword"
                  type="password"
                  autocomplete="current-password"
                />
              </div>
              <div class="field">
                <label>新密码</label>
                <input
                  v-model="passwordForm.newPassword"
                  type="password"
                  autocomplete="new-password"
                />
              </div>
            </div>
            <div class="field">
              <label>确认新密码</label>
              <input
                v-model="passwordForm.confirmPassword"
                type="password"
                autocomplete="new-password"
              />
            </div>
            <p v-if="passwordMismatch" class="form-error">两次输入的新密码不一致</p>
            <div class="form-actions">
              <button
                class="primary-button"
                :disabled="loading || !tokenReady || passwordMismatch"
              >
                <KeyRound :size="16" />
                修改密码
              </button>
            </div>
          </form>
        </section>
      </section>
    </main>
  </div>
</template>
