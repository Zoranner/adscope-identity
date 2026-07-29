<script setup lang="ts">
import { Building2, Database, FolderTree, KeyRound, RefreshCw, Workflow } from 'lucide-vue-next'

const route = useRoute()
const {
  managementToken,
  tokenReady,
  loading,
  loadToken,
  rememberToken,
  clearToken,
  refreshAll,
} = useAdminApi()

const navItems = [
  { to: '/', label: '目录', icon: FolderTree },
  { to: '/domains', label: '域', icon: Building2 },
  { to: '/sync', label: '同步', icon: Workflow },
]

const activeTitle = computed(
  () => navItems.find((item) => item.to === route.path)?.label ?? '目录',
)

onMounted(() => {
  loadToken()
  if (tokenReady.value) {
    void refreshAll()
  }
})
</script>

<template>
  <div class="app-shell">
    <header class="topbar">
      <NuxtLink class="brand" to="/">
        <span class="brand-mark">
          <Database :size="22" />
        </span>
        <span class="brand-copy">
          <span class="brand-title">ADSS Center</span>
          <span class="brand-subtitle">中心事实源管理工作台</span>
        </span>
      </NuxtLink>

      <div class="token-panel">
        <KeyRound :size="18" />
        <input v-model="managementToken" type="password" placeholder="管理凭证" />
        <button class="secondary-button" :disabled="!managementToken" @click="rememberToken">
          保存
        </button>
        <button class="icon-button" title="刷新" :disabled="loading || !tokenReady" @click="refreshAll">
          <RefreshCw :size="17" />
        </button>
        <button class="icon-button" title="清除凭证" @click="clearToken">
          <KeyRound :size="17" />
        </button>
      </div>
    </header>

    <div class="layout">
      <aside class="sidebar">
        <nav class="nav-list">
          <NuxtLink
            v-for="item in navItems"
            :key="item.to"
            class="nav-button"
            :class="{ active: route.path === item.to }"
            :to="item.to"
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
