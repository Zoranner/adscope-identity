<script setup lang="ts">
import { Button, Card, Spinner } from '@stellar/ui'
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
  <UserShell title="Adscope Center">
    <main class="grid items-start justify-items-center p-[var(--spacing-xl)]">
      <Card
        v-if="loading"
        class="w-full max-w-[44rem]"
        title="正在读取授权请求"
        :hoverable="false"
        aria-live="polite"
      >
        <Spinner />
        <p class="m-0 text-[var(--text-sm)] text-[var(--st-text-muted)]">请稍候。</p>
      </Card>

      <Card
        v-else-if="errorMessage"
        class="w-full max-w-[44rem]"
        title="无法继续授权"
        :hoverable="false"
        role="alert"
      >
        <p class="m-0 text-[var(--text-sm)] text-[var(--st-text-muted)]">{{ errorMessage }}</p>
        <Button icon="ph:arrows-clockwise" type="button" @click="loadAuthorizationContext">
          重新读取
        </Button>
      </Card>

      <form
        v-else-if="context"
        class="w-full max-w-[44rem]"
        method="post"
        action="/oauth2/authorize"
      >
        <Card :title="`${context.client_name} 请求访问`" :hoverable="false">
          <div class="grid gap-[var(--spacing-lg)]">
            <p class="m-0 text-[var(--text-sm)] text-[var(--st-text-muted)]">
              确认后，该应用将获得下列账号信息。
            </p>

            <section class="grid gap-[var(--spacing-md)]">
              <h2 class="m-0 text-[15px] font-semibold">当前账号</h2>
              <dl class="m-0 grid gap-[var(--spacing-md)]">
                <div class="grid grid-cols-[minmax(6rem,9rem)_minmax(0,1fr)] gap-[var(--spacing-md)]">
                  <dt class="text-[var(--st-text-muted)]">显示名</dt>
                  <dd class="m-0 [overflow-wrap:anywhere]">{{ context.user.display_name }}</dd>
                </div>
                <div class="grid grid-cols-[minmax(6rem,9rem)_minmax(0,1fr)] gap-[var(--spacing-md)]">
                  <dt class="text-[var(--st-text-muted)]">用户名</dt>
                  <dd class="m-0 [overflow-wrap:anywhere]">{{ context.user.username }}</dd>
                </div>
                <div class="grid grid-cols-[minmax(6rem,9rem)_minmax(0,1fr)] gap-[var(--spacing-md)]">
                  <dt class="text-[var(--st-text-muted)]">工号</dt>
                  <dd class="m-0 [overflow-wrap:anywhere]">{{ context.user.employee_id }}</dd>
                </div>
              </dl>
            </section>

            <section class="grid gap-[var(--spacing-md)]">
              <h2 class="m-0 text-[15px] font-semibold">提供的信息</h2>
              <dl class="m-0 grid gap-[var(--spacing-md)]">
                <div
                  v-for="[name, value] in claimEntries"
                  :key="name"
                  class="grid grid-cols-[minmax(6rem,9rem)_minmax(0,1fr)] gap-[var(--spacing-md)]"
                >
                  <dt class="text-[var(--st-text-muted)]">{{ name }}</dt>
                  <dd class="m-0 [overflow-wrap:anywhere]">{{ claimValue(value) }}</dd>
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

            <div class="flex flex-wrap items-center gap-[var(--spacing-md)]">
              <Button
                variant="solid"
                semantic="primary"
                type="submit"
                name="decision"
                value="approve"
                icon="ph:shield-check"
                :disabled="switchingAccount"
              >
                确认授权
              </Button>
              <Button
                type="submit"
                name="decision"
                value="cancel"
                icon="ph:x"
                :disabled="switchingAccount"
              >
                取消
              </Button>
              <Button
                variant="text"
                type="button"
                icon="ph:sign-in"
                :disabled="switchingAccount"
                @click="switchAccount"
              >
                切换账号
              </Button>
            </div>
          </div>
        </Card>
      </form>
    </main>
  </UserShell>
</template>
