<script setup lang="ts">
import { Copy, Trash2 } from 'lucide-vue-next'
import { onBeforeRouteLeave } from 'vue-router'
import OAuthClientEditor from '~/components/oauth/OAuthClientEditor.vue'
import OAuthClientTable from '~/components/oauth/OAuthClientTable.vue'
import type {
  CreateOAuthClientRequest,
  OAuthClient,
  OAuthClientSecretResponse,
  OAuthClientType,
  OAuthScope,
  UpdateOAuthClientRequest,
} from '~/types/admin'

type ModalView = 'editor' | 'secret' | 'delete'
type ClientOperation = 'save' | 'toggle' | 'delete' | 'secret'

interface OAuthClientEditorModel {
  name: string
  client_type: OAuthClientType
  redirect_uris: string
  allowed_scopes: OAuthScope[]
  enabled: boolean
}

const oauthScopes: OAuthScope[] = ['openid', 'profile', 'email', 'phone']
const {
  tokenReady,
  loading,
  oauthClients,
  loadOAuthClients,
  createOAuthClient,
  updateOAuthClient,
  deleteOAuthClient,
  regenerateOAuthClientSecret,
  runAction,
} = useAdminApi()
const { setStatus } = useAdminStatus()

const modalView = ref<ModalView | null>(null)
const selectedClientId = ref<string | null>(null)
const operation = ref<ClientOperation | null>(null)
const oneTimeSecret = ref<OAuthClientSecretResponse | null>(null)
const secretInput = ref<HTMLInputElement | null>(null)
const editorForm = reactive<OAuthClientEditorModel>(blankEditorForm())
let modalContextGeneration = 0
let tableActionGeneration = 0

const modalOpen = computed(() => modalView.value !== null)
const operationBusy = computed(() => operation.value !== null)
const selectedClient = computed(
  () => oauthClients.value.find((client) => client.client_id === selectedClientId.value) ?? null,
)
const modalTitle = computed(() => {
  if (modalView.value === 'secret') {
    return '客户端密钥'
  }
  if (modalView.value === 'delete') {
    return '删除客户端'
  }
  return selectedClientId.value ? '编辑登录客户端' : '创建登录客户端'
})

function blankEditorForm(): OAuthClientEditorModel {
  return {
    name: '',
    client_type: 'web',
    redirect_uris: '',
    allowed_scopes: ['openid'],
    enabled: true,
  }
}

function assignEditorForm(source: OAuthClientEditorModel) {
  Object.assign(editorForm, source)
}

function resetModalContext() {
  modalContextGeneration += 1
  modalView.value = null
  selectedClientId.value = null
  oneTimeSecret.value = null
}

function closeModal() {
  if (operationBusy.value) {
    return
  }
  resetModalContext()
}

function newClient() {
  if (operationBusy.value) {
    return
  }
  modalContextGeneration += 1
  selectedClientId.value = null
  oneTimeSecret.value = null
  assignEditorForm(blankEditorForm())
  modalView.value = 'editor'
}

function editClient(client: OAuthClient) {
  if (operationBusy.value) {
    return
  }
  modalContextGeneration += 1
  selectedClientId.value = client.client_id
  oneTimeSecret.value = null
  assignEditorForm({
    name: client.name,
    client_type: client.client_type,
    redirect_uris: client.redirect_uris.join('\n'),
    allowed_scopes: [...client.allowed_scopes],
    enabled: client.enabled,
  })
  modalView.value = 'editor'
}

function requestDelete(client: OAuthClient) {
  if (operationBusy.value) {
    return
  }
  modalContextGeneration += 1
  selectedClientId.value = client.client_id
  oneTimeSecret.value = null
  modalView.value = 'delete'
}

function modalContextMatches(generation: number, view: ModalView, clientId: string | null): boolean {
  return (
    modalContextGeneration === generation &&
    modalView.value === view &&
    selectedClientId.value === clientId
  )
}

function redirectUris(): string[] {
  return editorForm.redirect_uris
    .split(/\r?\n/)
    .map((uri) => uri.trim())
    .filter(Boolean)
}

function allowedScopes(): OAuthScope[] {
  return oauthScopes.filter((scope) => editorForm.allowed_scopes.includes(scope))
}

function updateRequest(enabled = editorForm.enabled): UpdateOAuthClientRequest {
  return {
    name: editorForm.name.trim(),
    redirect_uris: redirectUris(),
    allowed_scopes: allowedScopes(),
    enabled,
  }
}

function errorMessage(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : fallback
}

async function focusSecret(generation: number) {
  await nextTick()
  if (
    modalContextGeneration === generation &&
    modalView.value === 'secret' &&
    oneTimeSecret.value
  ) {
    secretInput.value?.focus()
  }
}

async function saveClient() {
  if (operationBusy.value || modalView.value !== 'editor') {
    return
  }
  const request = updateRequest()
  if (!request.name) {
    setStatus('请输入客户端名称', true)
    return
  }
  if (request.redirect_uris.length === 0) {
    setStatus('请至少输入一个 Redirect URI', true)
    return
  }
  const clientId = selectedClientId.value
  const generation = ++modalContextGeneration
  operation.value = 'save'

  try {
    if (clientId) {
      await updateOAuthClient(clientId, request)
      if (modalContextMatches(generation, 'editor', clientId)) {
        resetModalContext()
        setStatus('登录客户端已更新')
      }
      return
    }

    const createRequest: CreateOAuthClientRequest = {
      ...request,
      client_type: editorForm.client_type,
    }
    const response = await createOAuthClient(createRequest)
    if (!modalContextMatches(generation, 'editor', null)) {
      return
    }

    if (response.client_secret) {
      selectedClientId.value = response.client.client_id
      oneTimeSecret.value = {
        client_id: response.client.client_id,
        client_secret: response.client_secret,
      }
      modalView.value = 'secret'
      setStatus('Web 客户端已创建，请保存一次性密钥')
      await focusSecret(generation)
    } else {
      resetModalContext()
      setStatus('Desktop 客户端已创建')
    }
  } catch (error) {
    if (modalContextGeneration === generation) {
      setStatus(errorMessage(error, clientId ? '更新登录客户端失败' : '创建登录客户端失败'), true)
    }
  } finally {
    operation.value = null
  }
}

async function toggleClient(client: OAuthClient) {
  if (operationBusy.value) {
    return
  }
  const generation = ++tableActionGeneration
  operation.value = 'toggle'
  const enabled = !client.enabled

  try {
    await updateOAuthClient(client.client_id, {
      name: client.name,
      redirect_uris: [...client.redirect_uris],
      allowed_scopes: [...client.allowed_scopes],
      enabled,
    })
    if (tableActionGeneration === generation) {
      setStatus(enabled ? '登录客户端已启用' : '登录客户端已停用')
    }
  } catch (error) {
    if (tableActionGeneration === generation) {
      setStatus(errorMessage(error, '更新客户端状态失败'), true)
    }
  } finally {
    operation.value = null
  }
}

async function confirmDelete() {
  const clientId = selectedClientId.value
  if (operationBusy.value || modalView.value !== 'delete' || !clientId) {
    return
  }
  const generation = ++modalContextGeneration
  operation.value = 'delete'

  try {
    await deleteOAuthClient(clientId)
    if (modalContextMatches(generation, 'delete', clientId)) {
      resetModalContext()
      setStatus('登录客户端已删除')
    }
  } catch (error) {
    if (modalContextGeneration === generation) {
      setStatus(errorMessage(error, '删除登录客户端失败'), true)
    }
  } finally {
    operation.value = null
  }
}

async function regenerateSecret() {
  const clientId = selectedClientId.value
  if (operationBusy.value || modalView.value !== 'editor' || !clientId) {
    return
  }
  const generation = ++modalContextGeneration
  operation.value = 'secret'
  oneTimeSecret.value = null

  try {
    const response = await regenerateOAuthClientSecret(clientId)
    if (!modalContextMatches(generation, 'editor', clientId)) {
      return
    }
    oneTimeSecret.value = response
    modalView.value = 'secret'
    setStatus('客户端密钥已重新生成，请立即保存')
    await focusSecret(generation)
  } catch (error) {
    if (modalContextGeneration === generation) {
      setStatus(errorMessage(error, '重新生成客户端密钥失败'), true)
    }
  } finally {
    operation.value = null
  }
}

async function copySecret() {
  const secret = oneTimeSecret.value?.client_secret
  const generation = modalContextGeneration
  if (!secret || !import.meta.client || !navigator.clipboard) {
    setStatus('无法访问剪贴板，请手动复制客户端密钥', true)
    return
  }

  try {
    await navigator.clipboard.writeText(secret)
    if (modalContextGeneration === generation && modalView.value === 'secret') {
      setStatus('客户端密钥已复制')
    }
  } catch {
    if (modalContextGeneration === generation && modalView.value === 'secret') {
      setStatus('复制失败，请手动复制客户端密钥', true)
    }
  }
}

async function refreshClients() {
  await runAction(async () => {
    await loadOAuthClients()
  }, { successMessage: '登录客户端已刷新' })
}

function handleBeforeUnload(event: BeforeUnloadEvent) {
  if (operationBusy.value) {
    event.preventDefault()
    event.returnValue = ''
  }
}

onBeforeRouteLeave(() => {
  if (operationBusy.value) {
    return false
  }
})

onMounted(() => {
  window.addEventListener('beforeunload', handleBeforeUnload)
})

onBeforeUnmount(() => {
  modalContextGeneration += 1
  tableActionGeneration += 1
  window.removeEventListener('beforeunload', handleBeforeUnload)
})

watch(tokenReady, (ready) => {
  if (!ready) {
    resetModalContext()
  }
})
</script>

<template>
  <AdminShell :busy="operationBusy">
    <AdminPageHeader
      title="登录客户端"
      description="维护 OIDC 登录客户端、回调地址和授权范围。"
      :loading="loading || operationBusy"
      :disabled="!tokenReady"
      @refresh="refreshClients"
    />

    <section class="single-column-page">
      <OAuthClientTable
        :clients="oauthClients"
        :busy="loading || operationBusy"
        @create="newClient"
        @edit="editClient"
        @toggle="toggleClient"
        @delete="requestDelete"
      />
    </section>

    <AdminModal
      :open="modalOpen"
      :title="modalTitle"
      :close-disabled="operationBusy"
      width="wide"
      @close="closeModal"
    >
      <OAuthClientEditor
        v-if="modalView === 'editor'"
        v-model="editorForm"
        :editing-id="selectedClientId"
        :loading="operationBusy"
        :disabled="!tokenReady"
        @save="saveClient"
        @regenerate-secret="regenerateSecret"
      />

      <section v-else-if="modalView === 'secret' && oneTimeSecret" class="form connector-key-result">
        <div role="status" aria-live="polite" aria-atomic="true">
          <h3>客户端密钥已生成</h3>
          <p id="oauth-client-secret-notice">密钥仅显示一次，关闭后无法再次查看。</p>
        </div>
        <div class="field">
          <label for="oauth-client-secret-id">Client ID</label>
          <input
            id="oauth-client-secret-id"
            :value="oneTimeSecret.client_id"
            readonly
            spellcheck="false"
          />
        </div>
        <div class="field">
          <label for="oauth-client-secret">Client Secret</label>
          <div class="connector-key-row">
            <input
              id="oauth-client-secret"
              ref="secretInput"
              :value="oneTimeSecret.client_secret"
              aria-describedby="oauth-client-secret-notice"
              readonly
              spellcheck="false"
            />
            <button
              type="button"
              class="icon-button"
              title="复制 Client Secret"
              aria-label="复制 Client Secret"
              @click="copySecret"
            >
              <Copy :size="17" />
            </button>
          </div>
        </div>
        <div class="form-actions">
          <button type="button" class="secondary-button" @click="closeModal">关闭</button>
        </div>
      </section>

      <section v-else-if="modalView === 'delete' && selectedClient" class="form connector-key-result">
        <div>
          <h3>确认删除 {{ selectedClient.name }}？</h3>
          <p>{{ selectedClient.client_id }}</p>
        </div>
        <div class="form-actions">
          <button
            type="button"
            class="danger-button"
            :disabled="operationBusy"
            @click="confirmDelete"
          >
            <Trash2 :size="16" />
            删除
          </button>
          <button
            type="button"
            class="secondary-button"
            :disabled="operationBusy"
            @click="closeModal"
          >
            取消
          </button>
        </div>
      </section>
    </AdminModal>
  </AdminShell>
</template>
