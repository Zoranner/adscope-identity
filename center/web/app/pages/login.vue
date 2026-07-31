<script setup lang="ts">
import { Database, LogIn } from 'lucide-vue-next'

const { loading, loadToken, loadMe, login } = useUserApi()

const username = ref('')
const password = ref('')

onMounted(async () => {
  const storedToken = loadToken()
  if (!storedToken) {
    return
  }
  const valid = await loadMe(false)
  if (valid) {
    await navigateTo('/me', { replace: true })
  }
})

async function submitLogin() {
  const ok = await login(username.value, password.value)
  if (ok) {
    password.value = ''
    await navigateTo('/me', { replace: true })
  }
}
</script>

<template>
  <div class="app-shell">
    <UserStatusLine />

    <header class="topbar">
      <NuxtLink class="brand" to="/login">
        <span class="brand-mark">
          <Database :size="22" />
        </span>
        <span class="brand-copy">
          <span class="brand-title">ADSS Center</span>
          <span class="brand-subtitle">用户自助入口</span>
        </span>
      </NuxtLink>
    </header>

    <main class="credential-screen">
      <form class="credential-card" @submit.prevent="submitLogin">
        <div>
          <h1>用户登录</h1>
          <p>使用中心账号登录，维护本人资料和密码。</p>
        </div>
        <div class="field">
          <label>用户名</label>
          <input v-model="username" autocomplete="username" autofocus />
        </div>
        <div class="field">
          <label>密码</label>
          <input v-model="password" type="password" autocomplete="current-password" />
        </div>
        <button class="primary-button" :disabled="loading || !username.trim() || !password">
          <LogIn :size="16" />
          登录
        </button>
        <NuxtLink class="text-link" to="/admin">管理入口</NuxtLink>
      </form>
    </main>
  </div>
</template>
