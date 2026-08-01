<script setup lang="ts">
import { Building2, Database, FolderTree, LogOut, RefreshCw, Workflow } from 'lucide-vue-next'

const props = defineProps<{
  busy?: boolean
}>()

const route = useRoute()
const {
  tokenReady,
  loading,
  loadToken,
  clearToken,
  authenticateToken,
  refreshAll,
} = useAdminApi()
const credentialDraft = ref('')

const navItems = [
  { to: '/admin', label: '目录', icon: FolderTree },
  { to: '/admin/domains', label: '域', icon: Building2 },
  { to: '/admin/sync', label: '同步', icon: Workflow },
]

const activeTitle = computed(
  () => navItems.find((item) => item.to === route.path)?.label ?? '目录',
)

onMounted(() => {
  const storedToken = loadToken()
  if (storedToken) {
    void authenticateToken(storedToken, false)
  }
})

async function submitCredential() {
  const token = credentialDraft.value.trim()
  if (!token) {
    return
  }
  await authenticateToken(token)
}

function exitManagement() {
  if (props.busy) {
    return
  }
  credentialDraft.value = ''
  clearToken()
}

function preventBusyNavigation(event: MouseEvent) {
  if (props.busy) {
    event.preventDefault()
  }
}
</script>

<template>
  <div class="app-shell">
    <AdminStatusLine />

    <header class="topbar">
      <NuxtLink
        class="brand"
        to="/admin"
        :aria-disabled="busy ? 'true' : undefined"
        :tabindex="busy ? -1 : undefined"
        @click="preventBusyNavigation"
      >
        <span class="brand-mark">
          <Database :size="22" />
        </span>
        <span class="brand-copy">
          <span class="brand-title">ADSS Center</span>
          <span class="brand-subtitle">中心事实源管理工作台</span>
        </span>
      </NuxtLink>

      <div v-if="tokenReady" class="topbar-actions">
        <button
          class="icon-button"
          title="刷新"
          :disabled="loading || !tokenReady || busy"
          @click="refreshAll"
        >
          <RefreshCw :size="17" />
        </button>
        <button class="secondary-button" :disabled="busy" @click="exitManagement">
          <LogOut :size="16" />
          退出
        </button>
      </div>
    </header>

    <main v-if="!tokenReady" class="credential-screen">
      <form class="credential-card" @submit.prevent="submitCredential">
        <div>
          <h1>管理入口</h1>
          <p>输入中心服务的管理凭证后进入控制台。</p>
        </div>
        <div class="field">
          <label>管理凭证</label>
          <input v-model="credentialDraft" type="password" autocomplete="current-password" autofocus />
        </div>
        <button class="primary-button" :disabled="!credentialDraft.trim() || loading">
          进入
        </button>
      </form>
    </main>

    <div v-else class="layout">
      <aside class="sidebar">
        <nav class="nav-list">
          <NuxtLink
            v-for="item in navItems"
            :key="item.to"
            class="nav-button"
            :class="{ active: route.path === item.to }"
            :to="item.to"
            :aria-disabled="busy ? 'true' : undefined"
            :tabindex="busy ? -1 : undefined"
            @click="preventBusyNavigation"
          >
            <component :is="item.icon" :size="18" />
            <span>{{ item.label }}</span>
          </NuxtLink>
        </nav>
      </aside>

      <main class="content">
        <slot :title="activeTitle" />
      </main>
    </div>
  </div>
</template>
