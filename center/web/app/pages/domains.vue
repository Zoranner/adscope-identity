<script setup lang="ts">
import DomainEditor from '~/components/domains/DomainEditor.vue'
import DomainTable from '~/components/domains/DomainTable.vue'
import type { Domain, DomainForm } from '~/types/admin'
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
const domainForm = reactive<DomainForm>(blankDomainForm())

function assignForm<T extends object>(target: T, source: T) {
  Object.assign(target, source)
}

function newDomain() {
  selectedDomainId.value = null
  assignForm(domainForm, blankDomainForm())
}

function editDomain(domain: Domain) {
  selectedDomainId.value = domain.id
  assignForm(domainForm, {
    id: domain.id,
    name: domain.name,
    enabled: domain.enabled,
    mirror_root_dn: domain.mirror_root_dn,
    quarantine_ou_dn: domain.quarantine_ou_dn,
    upn_suffix: domain.upn_suffix,
    employee_id_attribute: domain.employee_id_attribute,
    managed_group_id_attribute: domain.managed_group_id_attribute,
    connector_key: '',
  })
}

async function refreshDomains() {
  await runAction(async () => {
    await Promise.all([loadDomains(), loadSyncDomains()])
  }, { successMessage: '域数据已刷新' })
}

async function saveDomain() {
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
    if (selectedDomainId.value) {
      await adminFetch(`/api/admin/domains/${encodeURIComponent(selectedDomainId.value)}`, {
        method: 'PATCH',
        body: JSON.stringify(payload),
      })
    } else {
      await adminFetch('/api/admin/domains', {
        method: 'POST',
        body: JSON.stringify({
          id: domainForm.id,
          connector_key: domainForm.connector_key,
          ...payload,
        }),
      })
    }
    await Promise.all([loadDomains(), loadSyncDomains()])
  }, { successMessage: selectedDomainId.value ? '域配置已更新' : '域配置已创建' })
}
</script>

<template>
  <AdminShell>
    <AdminPageHeader
      title="域管理"
      description="维护独立 AD 域配置和 Connector 读取边界。"
      :loading="loading"
      :disabled="!tokenReady"
      @refresh="refreshDomains"
    />
    <AdminStatusLine />

    <section class="workspace-grid">
      <DomainTable :domains="domains" @create="newDomain" @edit="editDomain" />
      <DomainEditor
        v-model="domainForm"
        :editing-id="selectedDomainId"
        :loading="loading"
        :disabled="!tokenReady"
        @save="saveDomain"
        @reset="newDomain"
      />
    </section>
  </AdminShell>
</template>
