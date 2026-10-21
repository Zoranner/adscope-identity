<script setup lang="ts">
import { Avatar, Badge, Button, Card, Icon, Input, Modal, Spinner } from '@stellar/ui'
import PageHeader from '~/components/shell/PageHeader.vue'
import { nullable } from '~/utils/forms'

const { profile, loadToken, loadMe, updateContact, changePassword } = useUserApi()

const displayName = computed(() => profile.value?.display_name || profile.value?.username || '')
const contactFields = computed(() => [
  {
    icon: 'ph:envelope-simple',
    label: '邮箱',
    value: profile.value?.email || '未填写',
    empty: !profile.value?.email,
  },
  {
    icon: 'ph:device-mobile',
    label: '手机',
    value: profile.value?.mobile || '未填写',
    empty: !profile.value?.mobile,
  },
])

const contactOpen = ref(false)
const passwordOpen = ref(false)
const saving = ref(false)
const contactForm = reactive({ email: '', mobile: '' })
const passwordForm = reactive({ currentPassword: '', newPassword: '', confirmPassword: '' })
const passwordMismatch = computed(
  () =>
    passwordForm.newPassword.length > 0 &&
    passwordForm.confirmPassword.length > 0 &&
    passwordForm.newPassword !== passwordForm.confirmPassword,
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

function openContact() {
  contactForm.email = profile.value?.email ?? ''
  contactForm.mobile = profile.value?.mobile ?? ''
  contactOpen.value = true
}

function openPassword() {
  passwordForm.currentPassword = ''
  passwordForm.newPassword = ''
  passwordForm.confirmPassword = ''
  passwordOpen.value = true
}

async function saveContact() {
  if (saving.value) {
    return
  }
  saving.value = true
  try {
    if (await updateContact(nullable(contactForm.email), nullable(contactForm.mobile))) {
      contactOpen.value = false
    }
  } finally {
    saving.value = false
  }
}

async function savePassword() {
  if (saving.value || passwordMismatch.value) {
    return
  }
  saving.value = true
  try {
    if (await changePassword(passwordForm.currentPassword, passwordForm.newPassword)) {
      passwordOpen.value = false
    }
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <UserShell title="Adscope Portal">
    <main class="w-full">
      <div
        class="mx-auto flex w-full max-w-[var(--max-width-4xl)] flex-col gap-[var(--spacing-lg)] px-[var(--spacing-xl)] py-[var(--spacing-2xl)]"
      >
        <PageHeader title="我的账号" />

        <div
          v-if="!profile"
          class="grid min-h-[16rem] place-items-center rounded-[var(--radius-md)] border border-[var(--st-border)] bg-[var(--st-bg-elevated)]"
        >
          <Spinner centered vertical size="lg" text="正在加载账号信息" semantic="primary" />
        </div>

        <template v-else>
          <Card :hoverable="false" :padded="false">
            <div
              class="flex flex-wrap items-center justify-between gap-[var(--spacing-lg)] p-[var(--spacing-xl)]"
            >
              <div class="flex min-w-0 items-center gap-[var(--spacing-md)]">
                <Avatar :text="displayName" size="xlarge" shape="circle" gradient />
                <div class="min-w-0">
                  <h2 class="m-0 min-w-0 truncate text-[var(--text-xl)] font-semibold leading-7">
                    {{ displayName }}
                  </h2>
                  <p
                    class="m-0 mt-[var(--spacing-2xs)] min-w-0 truncate text-[var(--text-sm)] text-[var(--st-text-muted)]"
                    :title="profile.username"
                  >
                    {{ profile.username }}
                  </p>
                </div>
              </div>
              <div
                class="flex min-w-0 items-center gap-[var(--spacing-sm)] md:border-l md:border-[var(--st-border-divider)] md:pl-[var(--spacing-xl)]"
              >
                <span
                  class="flex h-8 w-8 shrink-0 items-center justify-center rounded-[var(--radius-md)] bg-[var(--st-bg-surface)] text-[var(--st-text-muted)]"
                >
                  <Icon icon="ph:buildings" class="h-4 w-4" />
                </span>
                <div class="min-w-0">
                  <p class="m-0 text-[var(--text-xs)] text-[var(--st-text-muted)]">所属组织</p>
                  <p
                    class="m-0 mt-[var(--spacing-2xs)] truncate text-[var(--text-sm)] font-medium"
                    :title="profile.organizational_unit_id"
                  >
                    {{ profile.organizational_unit_id }}
                  </p>
                </div>
              </div>
            </div>
          </Card>

          <Card title="联系方式" :hoverable="false">
            <template #actions>
              <Button variant="text" size="small" icon="ph:pencil-simple" @click="openContact">
                编辑
              </Button>
            </template>
            <dl class="m-0 flex flex-col divide-y divide-[var(--st-border-divider)]">
              <div
                v-for="field in contactFields"
                :key="field.label"
                class="flex items-center justify-between gap-[var(--spacing-md)] py-[var(--spacing-sm)] first:pt-0 last:pb-0"
              >
                <dt
                  class="flex shrink-0 items-center gap-[var(--spacing-xs)] text-[var(--text-sm)] text-[var(--st-text-muted)]"
                >
                  <Icon :icon="field.icon" class="h-4 w-4 shrink-0" />
                  {{ field.label }}
                </dt>
                <dd
                  class="m-0 min-w-0 flex-1 break-words text-right text-[var(--text-sm)]"
                  :class="field.empty ? 'text-[var(--st-text-muted)]' : ''"
                >
                  {{ field.value }}
                </dd>
              </div>
            </dl>
          </Card>

          <Card title="账号安全" :hoverable="false">
            <div class="flex flex-wrap items-center justify-between gap-[var(--spacing-md)]">
              <div class="flex min-w-0 items-start gap-[var(--spacing-md)]">
                <span
                  class="flex h-10 w-10 shrink-0 items-center justify-center rounded-full bg-[var(--st-primary)]/10 text-[var(--st-text-accent)]"
                >
                  <Icon icon="ph:lock-key" class="h-5 w-5" />
                </span>
                <div class="min-w-0">
                  <div class="flex flex-wrap items-center gap-[var(--spacing-sm)]">
                    <p class="m-0 text-[var(--text-sm)] font-medium">登录密码</p>
                    <Badge class="shrink-0 whitespace-nowrap" text="已设置" semantic="success" />
                  </div>
                  <p class="m-0 mt-[var(--spacing-2xs)] text-[var(--text-sm)] text-[var(--st-text-muted)]">
                    用于登录 Adscope Portal。
                  </p>
                </div>
              </div>
              <Button class="ml-auto shrink-0" size="small" icon="ph:key" @click="openPassword">
                修改密码
              </Button>
            </div>
          </Card>

          <p class="m-0 text-[var(--text-xs)] text-[var(--st-text-muted)]">
            用户名和所属组织单元由管理员维护；联系方式可自行修改。
          </p>
        </template>
      </div>
    </main>

    <Modal
      v-model:visible="contactOpen"
      title="编辑联系方式"
      :blocked="saving"
      :disabled="saving"
    >
      <form id="contact-form" class="grid gap-[var(--spacing-lg)]" @submit.prevent="saveContact">
        <Input v-model="contactForm.email" type="email" label="邮箱" autocomplete="email" />
        <Input v-model="contactForm.mobile" type="tel" label="手机" autocomplete="tel" />
      </form>
      <template #footer>
        <Button :disabled="saving" @click="contactOpen = false">取消</Button>
        <Button
          form="contact-form"
          type="submit"
          variant="solid"
          semantic="primary"
          icon="ph:floppy-disk"
          :disabled="saving"
        >
          保存
        </Button>
      </template>
    </Modal>

    <Modal
      v-model:visible="passwordOpen"
      title="修改密码"
      :blocked="saving"
      :disabled="saving"
    >
      <form id="password-form" class="grid gap-[var(--spacing-lg)]" @submit.prevent="savePassword">
        <Input
          v-model="passwordForm.currentPassword"
          label="当前密码"
          type="password"
          autocomplete="current-password"
        />
        <Input
          v-model="passwordForm.newPassword"
          label="新密码"
          type="password"
          autocomplete="new-password"
        />
        <Input
          v-model="passwordForm.confirmPassword"
          label="确认新密码"
          type="password"
          autocomplete="new-password"
          :error="passwordMismatch ? '两次输入的新密码不一致' : undefined"
        />
      </form>
      <template #footer>
        <Button :disabled="saving" @click="passwordOpen = false">取消</Button>
        <Button
          form="password-form"
          type="submit"
          variant="solid"
          semantic="primary"
          :disabled="saving || passwordMismatch"
        >
          确认修改
        </Button>
      </template>
    </Modal>
  </UserShell>
</template>
