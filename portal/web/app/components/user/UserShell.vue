<script setup lang="ts">
import {
  Avatar,
  Button,
  DropdownMenu,
  Icon,
  MenuItem,
  NavBar,
} from '@stellar/ui'

defineProps<{
  title?: string
}>()

const { theme, toggleTheme } = useTheme()
const { profile, logout } = useUserApi()
const themeLabel = computed(() => (theme.value === 'dark' ? '切换为浅色主题' : '切换为深色主题'))
const displayName = computed(() => profile.value?.display_name || profile.value?.username || '')

async function exitUser() {
  await logout()
  await navigateTo('/login', { replace: true })
}

function handleExit(close: () => void) {
  close()
  void exitUser()
}
</script>

<template>
  <div class="grid min-h-screen grid-rows-[auto_minmax(0,1fr)]">
    <NavBar
      :title="title ?? 'Adscope Portal'"
      variant="solid"
      bordered
      class="[&>div]:justify-between"
    >
      <template #center>
        <span />
      </template>
      <template #actions>
        <div class="flex items-center gap-[var(--spacing-sm)]">
          <Button
            variant="text"
            square
            :icon="theme === 'dark' ? 'ph:sun' : 'ph:moon'"
            :aria-label="themeLabel"
            :title="themeLabel"
            @click="toggleTheme"
          />
          <DropdownMenu v-if="profile" placement="bottom" align="right">
            <template #trigger>
              <span
                class="flex cursor-pointer items-center gap-[var(--spacing-sm)] rounded-[var(--radius-md)] px-[var(--spacing-xs)] py-[var(--spacing-xs)] hover:bg-[var(--st-bg-surface)]"
              >
                <Avatar :text="displayName" size="small" shape="circle" />
                <span class="text-[var(--text-sm)]">{{ displayName }}</span>
                <Icon icon="ph:caret-down" class="h-4 w-4 text-[var(--st-text-secondary)]" />
              </span>
            </template>
            <template #default="{ close }">
              <MenuItem icon="ph:sign-out" danger label="退出登录" @click="handleExit(close)" />
            </template>
          </DropdownMenu>
        </div>
      </template>
    </NavBar>

    <slot />
  </div>
</template>
