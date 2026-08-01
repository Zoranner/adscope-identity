<script setup lang="ts">
import { Copy } from 'lucide-vue-next'
import { onBeforeRouteLeave } from 'vue-router'
import DomainEditor from '~/components/domains/DomainEditor.vue'
import DomainTable from '~/components/domains/DomainTable.vue'
import type { Domain, DomainForm, DomainMutationResponse } from '~/types/admin'
import { blankDomainForm } from '~/utils/forms'

const {
  tokenReady,
  loading,
  domains,
  adminFetch,
  runAction,
  loadDomains,
  loadSyncDomains,
} = useAdminApi()

const selectedDomainId = ref<string | null>(null)
const modalOpen = ref(false)
const saving = ref(false)
const connectorKey = ref<string | null>(null)
const connectorKeyInput = ref<HTMLInputElement | null>(null)
const domainForm = reactive<DomainForm>(blankDomainForm())
const { setStatus } = useAdminStatus()
let domainContextGeneration = 0

function handleBeforeUnload(event: BeforeUnloadEvent) {
  if (saving.value) {
    event.preventDefault()
    event.returnValue = ''
  }
}

onBeforeRouteLeave(() => {
  if (saving.value) {
    return false
  }
})

onMounted(() => {
  window.addEventListener('beforeunload', handleBeforeUnload)
})

onBeforeUnmount(() => {
  window.removeEventListener('beforeunload', handleBeforeUnload)
})

watch(tokenReady, (ready) => {
  if (!ready) {
    domainContextGeneration += 1
    modalOpen.value = false
    connectorKey.value = null
  }
})

function assignForm<T extends object>(target: T, source: T) {
  Object.assign(target, source)
}

function newDomain() {
  if (saving.value) {
    return
  }
  domainContextGeneration += 1
  selectedDomainId.value = null
  connectorKey.value = null
  assignForm(domainForm, blankDomainForm())
  modalOpen.value = true
}

function editDomain(domain: Domain) {
  if (saving.value) {
    return
  }
  domainContextGeneration += 1
  selectedDomainId.value = domain.id
  connectorKey.value = null
  modalOpen.value = true
  assignForm(domainForm, {
    id: domain.id,
    name: domain.name,
    enabled: domain.enabled,
    mirror_root_dn: domain.mirror_root_dn,
    quarantine_ou_dn: domain.quarantine_ou_dn,
    upn_suffix: domain.upn_suffix,
    employee_id_attribute: domain.employee_id_attribute,
    managed_group_id_attribute: domain.managed_group_id_attribute,
  })
}

function closeModal() {
  if (saving.value) {
    return
  }
  domainContextGeneration += 1
  modalOpen.value = false
  connectorKey.value = null
}

async function copyConnectorKey() {
  if (!connectorKey.value || !import.meta.client || !navigator.clipboard) {
    setStatus('无法访问剪贴板，请手动复制 Connector key', true)
    return
  }

  try {
    await navigator.clipboard.writeText(connectorKey.value)
    setStatus('Connector key 已复制')
  } catch {
    setStatus('复制失败，请手动复制 Connector key', true)
  }
}

async function refreshDomains() {
  await runAction(async () => {
    await Promise.all([loadDomains(), loadSyncDomains()])
  }, { successMessage: '域数据已刷新' })
}

async function saveDomain() {
  if (saving.value) {
    return
  }
  saving.value = true
  connectorKey.value = null
  const contextDomainId = selectedDomainId.value
  const contextGeneration = ++domainContextGeneration
  const editing = contextDomainId !== null
  try {
    await runAction(async () => {
      const payload = {
        name: domainForm.name,
        enabled: domainForm.enabled,
        mirror_root_dn: domainForm.mirror_root_dn,
        quarantine_ou_dn: domainForm.quarantine_ou_dn,
        upn_suffix: domainForm.upn_suffix,
        employee_id_attribute: domainForm.employee_id_attribute,
        managed_group_id_attribute: domainForm.managed_group_id_attribute,
      }
      let response: DomainMutationResponse
      if (contextDomainId) {
        response = await adminFetch<DomainMutationResponse>(
          `/api/admin/domains/${encodeURIComponent(contextDomainId)}`,
          {
            method: 'PATCH',
            body: JSON.stringify(payload),
          },
        )
      } else {
        response = await adminFetch<DomainMutationResponse>('/api/admin/domains', {
          method: 'POST',
          body: JSON.stringify({
            id: domainForm.id,
            ...payload,
          }),
        })
      }
      if (
        domainContextGeneration === contextGeneration &&
        modalOpen.value &&
        selectedDomainId.value === contextDomainId
      ) {
        selectedDomainId.value = response.domain.id
        connectorKey.value = response.connector_key
        await nextTick()
        if (domainContextGeneration === contextGeneration && modalOpen.value) {
          connectorKeyInput.value?.focus()
        }
      }
      await Promise.all([loadDomains(), loadSyncDomains()])
      if (domainContextGeneration === contextGeneration && modalOpen.value) {
        setStatus(editing ? '域配置已更新' : '域配置已创建')
      }
    })
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <AdminShell :busy="saving">
    <AdminPageHeader
      title="域管理"
      description="维护独立 AD 域配置和 Connector 读取边界。"
      :loading="loading"
      :disabled="!tokenReady"
      @refresh="refreshDomains"
    />

    <section class="single-column-page">
      <DomainTable :domains="domains" @create="newDomain" @edit="editDomain" />
    </section>

    <AdminModal
      :open="modalOpen"
      :title="connectorKey ? 'Connector key' : selectedDomainId ? '编辑域' : '创建域'"
      :close-disabled="saving"
      width="wide"
      @close="closeModal"
    >
      <DomainEditor
        v-if="!connectorKey"
        v-model="domainForm"
        :editing-id="selectedDomainId"
        :loading="loading || saving"
        :disabled="!tokenReady"
        @save="saveDomain"
        @reset="newDomain"
      />
      <section v-else class="form connector-key-result">
        <div role="status" aria-live="polite" aria-atomic="true">
          <h3>Connector key 已生成</h3>
          <p id="connector-key-notice">仅显示一次；关闭后需重新保存域配置才能生成新 key。</p>
        </div>
        <div class="field">
          <label for="generated-connector-key">ADSS_CONNECTOR_KEY</label>
          <div class="connector-key-row">
            <input
              id="generated-connector-key"
              ref="connectorKeyInput"
              :value="connectorKey"
              aria-describedby="connector-key-notice"
              readonly
              spellcheck="false"
            />
            <button
              type="button"
              class="icon-button"
              title="复制 ADSS_CONNECTOR_KEY"
              aria-label="复制 ADSS_CONNECTOR_KEY"
              @click="copyConnectorKey"
            >
              <Copy :size="17" />
            </button>
          </div>
        </div>
        <div class="form-actions">
          <button type="button" class="secondary-button" :disabled="saving" @click="closeModal">
            关闭
          </button>
        </div>
      </section>
    </AdminModal>
  </AdminShell>
</template>
