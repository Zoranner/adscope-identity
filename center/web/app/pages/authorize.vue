<script setup lang="ts">
import { Database, LogIn, ShieldCheck, UserRound, X } from 'lucide-vue-next'
import { Button, Spinner } from '@stellar/ui'
import type { OidcAuthorizationContext } from '~/types/oidc'
import { authorizationFormFields } from '~/utils/oidc'

const route = useRoute()
const { logout } = useUserApi()

const context = ref<OidcAuthorizationContext | null>(null)
const loading = ref(true)
const switchingAccount = ref(false)
const errorMessage = ref('')
let requestGeneration = 0

const formFields = computed(() =>
  context.value ? authorizationFormFields(context.value.authorization) : [],
)
const claimEntries = computed(() => Object.entries(context.value?.claims ?? {}))

function authorizationPath(): string {
  return `/oauth2/authorize${window.location.search}`
}

function contextPath(): string {
  return `/api/oauth2/authorize/context${window.location.search}`
}

function authorizationErrorMessage(status: number, error?: string): string {
  if (error === 'invalid_request') {
    return '授权请求无效，请返回客户端重新发起。'
  }
  if (error === 'server_error') {
    return '授权服务暂时不可用，请稍后重试。'
  }
  return `无法读取授权请求（${status}）`
}

function claimValue(value: unknown): string {
  if (typeof value === 'string') {
    return value
  }
  return JSON.stringify(value) ?? String(value)
}

async function loadAuthorizationContext() {
  const generation = ++requestGeneration
  const requestPath = contextPath()
  const continuePath = authorizationPath()
  loading.value = true
  context.value = null
  errorMessage.value = ''
  try {
    const response = await fetch(requestPath, {
      headers: { accept: 'application/json' },
    })
    if (generation !== requestGeneration) {
      return
    }
    if (response.status === 401) {
      await navigateTo(
        {
          path: '/login',
          query: { continue: continuePath },
        },
        { replace: true },
      )
      return
    }
    if (!response.ok) {
      const payload = (await response.json().catch(() => null)) as { error?: string } | null
      if (generation !== requestGeneration) {
        return
      }
      errorMessage.value = authorizationErrorMessage(response.status, payload?.error)
      return
    }
    const payload = (await response.json()) as OidcAuthorizationContext
    if (generation !== requestGeneration) {
      return
    }
    context.value = payload
  } catch {
    if (generation === requestGeneration) {
      errorMessage.value = '无法连接授权服务，请检查网络后重试。'
    }
  } finally {
    if (generation === requestGeneration) {
      loading.value = false
    }
  }
}

async function switchAccount() {
  switchingAccount.value = true
  await logout(false)
  window.location.assign(authorizationPath())
}

onMounted(loadAuthorizationContext)

watch(
  () => route.fullPath,
  () => {
    if (route.path === '/authorize') {
      void loadAuthorizationContext()
    }
  },
)
</script>

<template>
  <div class="app-shell">
    <header class="topbar">
      <NuxtLink class="brand" to="/login">
        <span class="brand-mark">
          <Database :size="22" />
        </span>
        <span class="brand-copy">
          <span class="brand-title">Adscope Center</span>
          <span class="brand-subtitle">身份授权</span>
        </span>
      </NuxtLink>
    </header>

    <main class="credential-screen">
      <section v-if="loading" class="credential-card" aria-live="polite">
        <Spinner />
        <div>
          <h1>正在读取授权请求</h1>
          <p>请稍候。</p>
        </div>
      </section>

      <section v-else-if="errorMessage" class="credential-card" role="alert">
        <div>
          <h1>无法继续授权</h1>
          <p class="form-error">{{ errorMessage }}</p>
        </div>
        <Button variant="outline" type="button" @click="loadAuthorizationContext">
          <template #icon><LogIn :size="16" /></template>
          重新读取
        </Button>
      </section>

      <form
        v-else-if="context"
        class="credential-card authorization-card"
        method="post"
        action="/oauth2/authorize"
      >
        <div>
          <h1>{{ context.client_name }} 请求访问</h1>
          <p>确认后，该应用将获得下列账号信息。</p>
        </div>

        <section class="authorization-section authorization-account">
          <div class="panel-title">
            <UserRound :size="18" />
            <h2>当前账号</h2>
          </div>
          <dl class="profile-list">
            <div>
              <dt>显示名</dt>
              <dd>{{ context.user.display_name }}</dd>
            </div>
            <div>
              <dt>用户名</dt>
              <dd>{{ context.user.username }}</dd>
            </div>
            <div>
              <dt>工号</dt>
              <dd>{{ context.user.employee_id }}</dd>
            </div>
          </dl>
        </section>

        <section class="authorization-section authorization-claims">
          <div class="panel-title">
            <ShieldCheck :size="18" />
            <h2>提供的信息</h2>
          </div>
          <dl class="profile-list authorization-claims-list">
            <div v-for="[name, value] in claimEntries" :key="name">
              <dt>{{ name }}</dt>
              <dd>{{ claimValue(value) }}</dd>
            </div>
          </dl>
        </section>

        <input
          v-for="field in formFields"
          :key="field.name"
          type="hidden"
          :name="field.name"
          :value="field.value"
        />
        <input type="hidden" name="csrf_token" :value="context.csrf_token" />

        <div class="form-actions">
          <Button
            variant="solid"
            type="submit"
            name="decision"
            value="approve"
            :disabled="switchingAccount"
          >
            <template #icon><ShieldCheck :size="16" /></template>
            确认授权
          </Button>
          <Button
            variant="outline"
            type="submit"
            name="decision"
            value="cancel"
            :disabled="switchingAccount"
          >
            <template #icon><X :size="16" /></template>
            取消
          </Button>
          <Button
            variant="text"
            type="button"
            :disabled="switchingAccount"
            @click="switchAccount"
          >
            <template #icon><LogIn :size="16" /></template>
            切换账号
          </Button>
        </div>
      </form>
    </main>
  </div>
</template>
