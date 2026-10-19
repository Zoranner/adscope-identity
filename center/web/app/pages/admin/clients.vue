<script setup lang="ts">
import { Alert, Button, Drawer, Input, useNotification } from '@stellar/ui'
import OAuthClientEditor from '~/components/oauth/OAuthClientEditor.vue'
import OAuthClientTable from '~/components/oauth/OAuthClientTable.vue'
import type {
  CreateOAuthClientRequest,
  OAuthClient,
  OAuthClientSecretResponse,
  OAuthClientType,
  OAuthScope,
} from '~/types/admin'

type DrawerView = 'editor' | 'secret'

interface OAuthClientEditorModel {
  name: string
  client_type: OAuthClientType
  redirect_uris: string
  allowed_scopes: OAuthScope[]
  enabled: boolean
}

function blankEditorForm(): OAuthClientEditorModel {
  return {
    name: '',
    client_type: 'web',
    redirect_uris: '',
    allowed_scopes: ['openid'],
    enabled: true,
  }
}

const {
  oauthClients,
  clientsLoading,
  pending,
  createOAuthClient,
  updateOAuthClient,
  deleteOAuthClient,
  regenerateOAuthClientSecret,
  loadOAuthClients,
  runAction,
} = useAdminApi()
const notifications = useNotification()
const { confirm } = useConfirm()

const drawerOpen = ref(false)
const drawerView = ref<DrawerView>('editor')
const selectedClientId = ref<string | null>(null)
const saving = ref(false)
const secret = ref<OAuthClientSecretResponse | null>(null)
const editorForm = reactive<OAuthClientEditorModel>(blankEditorForm())

const busy = computed(() => saving.value || pending.value > 0)
const drawerTitle = computed(() => {
  if (drawerView.value === 'secret') {
    return '客户端密钥'
  }
  return selectedClientId.value ? '编辑接入应用' : '新建接入应用'
})

function openEditor(client?: OAuthClient) {
  selectedClientId.value = client?.client_id ?? null
  secret.value = null
  drawerView.value = 'editor'
  Object.assign(editorForm, {
    ...blankEditorForm(),
    ...(client
      ? {
          name: client.name,
          client_type: client.client_type,
          redirect_uris: client.redirect_uris.join('\n'),
          allowed_scopes: [...client.allowed_scopes],
          enabled: client.enabled,
        }
      : {}),
  })
  drawerOpen.value = true
}

function redirectUris(): string[] {
  return editorForm.redirect_uris
    .split(/\r?\n/)
    .map((uri) => uri.trim())
    .filter(Boolean)
}

async function saveClient() {
  if (saving.value) {
    return
  }
  if (!editorForm.name.trim()) {
    notifications.error('请输入应用名称')
    return
  }
  if (redirectUris().length === 0) {
    notifications.error('请至少填写一个 Redirect URI')
    return
  }

  saving.value = true
  const clientId = selectedClientId.value
  try {
    await runAction(
      async () => {
        const request = {
          name: editorForm.name.trim(),
          redirect_uris: redirectUris(),
          allowed_scopes: editorForm.allowed_scopes,
          enabled: editorForm.enabled,
        }
        if (clientId) {
          await updateOAuthClient(clientId, request)
          drawerOpen.value = false
          return
        }
        const createRequest: CreateOAuthClientRequest = {
          ...request,
          client_type: editorForm.client_type,
        }
        const response = await createOAuthClient(createRequest)
        selectedClientId.value = response.client.client_id
        if (response.client_secret) {
          secret.value = {
            client_id: response.client.client_id,
            client_secret: response.client_secret,
          }
          drawerView.value = 'secret'
          return
        }
        drawerOpen.value = false
      },
      { successMessage: clientId ? '接入应用已更新' : '接入应用已创建' },
    )
  } finally {
    saving.value = false
  }
}

async function toggleClient(client: OAuthClient) {
  const enabled = !client.enabled
  saving.value = true
  try {
    await runAction(
      async () => {
        await updateOAuthClient(client.client_id, {
          name: client.name,
          redirect_uris: [...client.redirect_uris],
          allowed_scopes: [...client.allowed_scopes],
          enabled,
        })
      },
      { successMessage: enabled ? '接入应用已启用' : '接入应用已停用' },
    )
  } finally {
    saving.value = false
  }
}

async function removeClient(client: OAuthClient) {
  const confirmed = await confirm({
    title: '删除接入应用',
    message: `删除“${client.name}”？`,
    description: '删除后该应用已签发的登录会话仍然有效，但无法再次发起授权。',
    confirmText: '删除',
    danger: true,
  })
  if (!confirmed) {
    return
  }

  saving.value = true
  try {
    await runAction(
      async () => {
        await deleteOAuthClient(client.client_id)
      },
      { successMessage: '接入应用已删除' },
    )
  } finally {
    saving.value = false
  }
}

async function regenerateSecret() {
  const clientId = selectedClientId.value
  if (!clientId) {
    return
  }
  const confirmed = await confirm({
    title: '重新生成密钥',
    message: `为“${editorForm.name}”生成新的客户端密钥？`,
    description: '旧密钥立即失效，使用旧密钥的应用需要更新配置。',
    confirmText: '重新生成',
    danger: true,
  })
  if (!confirmed) {
    return
  }

  saving.value = true
  try {
    await runAction(async () => {
      const response = await regenerateOAuthClientSecret(clientId)
      secret.value = response
      drawerView.value = 'secret'
    })
  } finally {
    saving.value = false
  }
}

async function copySecret() {
  const value = secret.value?.client_secret
  if (!value || !import.meta.client || !navigator.clipboard) {
    notifications.error('无法访问剪贴板，请手动复制客户端密钥')
    return
  }
  try {
    await navigator.clipboard.writeText(value)
    notifications.success('客户端密钥已复制')
  } catch {
    notifications.error('复制失败，请手动复制客户端密钥')
  }
}

async function refreshClients() {
  await runAction(
    async () => {
      await loadOAuthClients()
    },
    { successMessage: '接入应用已刷新' },
  )
}
</script>

<template>
  <AdminShell title="应用接入">
    <template #actions>
      <Button icon="ph:arrows-clockwise" @click="refreshClients">刷新</Button>
      <Button
        variant="solid"
        semantic="primary"
        icon="ph:plus"
        :disabled="busy"
        @click="openEditor()"
      >
        新建
      </Button>
    </template>

    <OAuthClientTable
      :clients="oauthClients"
      :loading="clientsLoading"
      :busy="busy"
      @edit="openEditor"
      @toggle="toggleClient"
      @remove="removeClient"
    />

    <Drawer
      v-model:visible="drawerOpen"
      :title="drawerTitle"
      :blocked="saving"
      :disabled="saving"
      size="large"
    >
      <div v-if="drawerView === 'secret' && secret" class="grid gap-[var(--spacing-lg)]">
        <Alert semantic="warning" title="客户端密钥仅显示一次">
          关闭后无法再次查看，需要重新生成密钥。
        </Alert>
        <Input label="Client ID" :model-value="secret.client_id" readonly spellcheck="false" />
        <Input label="Client Secret" :model-value="secret.client_secret" readonly spellcheck="false" />
        <div class="flex gap-[var(--spacing-sm)]">
          <Button icon="ph:copy" @click="copySecret">复制密钥</Button>
        </div>
      </div>

      <OAuthClientEditor
        v-else
        v-model="editorForm"
        :editing-id="selectedClientId"
        :saving="saving"
        @save="saveClient"
        @regenerate-secret="regenerateSecret"
      />

      <template #footer>
        <Button :disabled="saving" @click="drawerOpen = false">
          {{ drawerView === 'secret' ? '关闭' : '取消' }}
        </Button>
        <Button
          v-if="drawerView === 'editor'"
          form="oauth-client-form"
          type="submit"
          variant="solid"
          semantic="primary"
          icon="ph:floppy-disk"
          :disabled="saving"
        >
          保存
        </Button>
      </template>
    </Drawer>
  </AdminShell>
</template>
