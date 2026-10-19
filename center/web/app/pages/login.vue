<script setup lang="ts">
import { Button, Card, Input } from '@stellar/ui'
import { resolveAuthorizationContinue } from '~/utils/oidc'

const { loading, loadToken, loadMe, login } = useUserApi()
const route = useRoute()

const username = ref('')
const password = ref('')
const authorizationContinue = computed(() => {
  if (!import.meta.client) {
    return null
  }
  return resolveAuthorizationContinue(route.query.continue, window.location.origin)
})

onMounted(async () => {
  if (authorizationContinue.value) {
    return
  }
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
  if (!ok) {
    return
  }
  password.value = ''
  if (authorizationContinue.value) {
    window.location.assign(authorizationContinue.value)
    return
  }
  await navigateTo('/me', { replace: true })
}
</script>

<template>
  <UserShell title="Adscope Center">
    <main class="grid place-items-center p-[var(--spacing-xl)]">
      <div class="w-full max-w-[var(--max-width-md)]">
        <Card title="用户登录" :hoverable="false">
        <form class="grid gap-[var(--spacing-lg)]" @submit.prevent="submitLogin">
          <p class="m-0 text-[var(--text-sm)] text-[var(--st-text-muted)]">
            使用中心账号登录，查看本人资料、维护联系方式和密码。
          </p>
          <Input v-model="username" label="用户名" autocomplete="username" autofocus required />
          <Input
            v-model="password"
            label="密码"
            type="password"
            autocomplete="current-password"
            required
          />
          <Button
            type="submit"
            variant="solid"
            semantic="primary"
            icon="ph:sign-in"
            :disabled="loading || !username.trim() || !password"
          >
            登录
          </Button>
        </form>
        </Card>
      </div>
    </main>
  </UserShell>
</template>
