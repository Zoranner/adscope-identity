<script setup lang="ts">
import {
  Avatar,
  Button,
  Card,
  DropdownMenu,
  Icon,
  Input,
  MenuItem,
  NavBar,
} from '@stellar/ui'
import PageHeader from '~/components/shell/PageHeader.vue'

const props = defineProps<{
  busy?: boolean
  title?: string
}>()

const route = useRoute()
const slots = useSlots()
const {
  tokenReady,
  pending,
  restoreSession,
  authenticateToken,
  logout,
} = useAdminApi()
const credentialDraft = ref('')
const { theme, toggleTheme } = useTheme()

const navigationItems = [
  { to: '/admin/directory', label: '目录', icon: 'ph:folders' },
  { to: '/admin/domains', label: '域', icon: 'ph:buildings' },
  { to: '/admin/clients', label: '应用接入', icon: 'ph:key' },
]

const shellBusy = computed(() => props.busy === true || pending.value > 0)
const themeLabel = computed(() => (theme.value === 'dark' ? '切换为浅色主题' : '切换为深色主题'))
const showPageHeader = computed(
  () =>
    Boolean(props.title) ||
    Boolean(slots['actions']),
)
const navigationBarItems = computed(() =>
  navigationItems.map((item) => ({
    label: item.label,
    icon: item.icon,
    active: isActive(item.to),
    disabled: shellBusy.value,
  })),
)

/** 静态目录会把 /admin 重定向成 /admin/，比较前先把末尾斜杠归一化。 */
function trimmedPath(path: string): string {
  return path.replace(/\/+$/, '') || '/'
}

function isActive(to: string): boolean {
  const current = trimmedPath(route.path)
  const target = trimmedPath(to)
  return current === target || current.startsWith(`${target}/`)
}

function onNavigationClick(item: { label: string }) {
  const target = navigationItems.find((navigation) => navigation.label === item.label)
  if (!target || shellBusy.value) {
    return
  }
  void navigateTo(target.to)
}

onMounted(() => {
  void restoreSession()
})

async function submitCredential() {
  const token = credentialDraft.value.trim()
  if (!token) {
    return
  }
  if (await authenticateToken(token)) {
    credentialDraft.value = ''
  }
}

async function exitManagement() {
  if (await logout()) {
    credentialDraft.value = ''
  }
}

function handleExitFromMenu(close: () => void) {
  close()
  void exitManagement()
}

</script>

<template>
  <main v-if="!tokenReady" class="grid min-h-screen place-items-center p-[var(--spacing-xl)]">
    <div class="w-full max-w-[var(--max-width-md)]">
      <Card title="管理入口" :hoverable="false">
      <form class="grid gap-[var(--spacing-lg)]" @submit.prevent="submitCredential">
        <p class="m-0 text-[var(--text-sm)] text-[var(--st-text-muted)]">
          输入 Core 部署时的 MANAGEMENT_TOKEN 后进入控制台。
        </p>
        <Input
          v-model="credentialDraft"
          label="管理凭证"
          type="password"
          autocomplete="current-password"
          required
          autofocus
        />
        <Button
          type="submit"
          variant="solid"
          semantic="primary"
          :disabled="!credentialDraft.trim() || shellBusy"
        >
          进入
        </Button>
      </form>
      </Card>
    </div>
  </main>

  <div v-else class="flex h-dvh min-h-0 flex-col">
    <NavBar
      title="Adscope Portal"
      variant="solid"
      :items="navigationBarItems"
      @item-click="onNavigationClick"
    >
      <template #actions>
        <Button
          variant="text"
          square
          :icon="theme === 'dark' ? 'ph:sun' : 'ph:moon'"
          :aria-label="themeLabel"
          :title="themeLabel"
          @click="toggleTheme"
        />
        <DropdownMenu placement="bottom" align="right">
          <template #trigger>
            <span
              class="flex items-center gap-[var(--spacing-sm)] rounded-[var(--radius-md)] px-[var(--spacing-xs)] py-[var(--spacing-xs)] hover:bg-[var(--st-bg-surface)]"
            >
              <Avatar icon="ph:user-circle" size="small" shape="circle" />
              <span class="text-[var(--text-sm)]">管理员</span>
              <Icon icon="ph:caret-down" class="h-4 w-4 text-[var(--st-text-secondary)]" />
            </span>
          </template>
          <template #default="{ close }">
            <MenuItem
              icon="ph:sign-out"
              danger
              label="退出管理"
              :disabled="shellBusy"
              @click="handleExitFromMenu(close)"
            />
          </template>
        </DropdownMenu>
      </template>
    </NavBar>

    <!-- 内容区四边内边距由布局统一给出：页面只往里放面板，不再自己加边距。 -->
    <main
      class="flex min-h-0 min-w-0 flex-1 flex-col gap-[var(--spacing-lg)] overflow-hidden px-[var(--spacing-2xl)] pt-[var(--spacing-lg)] pb-[var(--spacing-2xl)]"
    >
      <PageHeader v-if="showPageHeader" :title="title">
        <template #actions>
          <slot name="actions" />
        </template>
      </PageHeader>

      <slot />
    </main>
  </div>
</template>
