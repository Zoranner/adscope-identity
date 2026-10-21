<script setup lang="ts">
import { Alert, Button, Drawer, Input, useNotification } from '@stellar/ui'
import DomainEditor from '~/components/domains/DomainEditor.vue'
import DomainTable from '~/components/domains/DomainTable.vue'
import type { Domain, DomainForm, DomainMutationResponse } from '~/types/admin'
import { blankDomainForm } from '~/utils/forms'

const {
  domains,
  domainAttributeDefaults,
  domainsLoading,
  pending,
  adminFetch,
  runAction,
  loadDomains,
  loadSyncDomains,
} = useAdminApi()
const notifications = useNotification()

const selectedDomainId = ref<string | null>(null)
const drawerOpen = ref(false)
const saving = ref(false)
const syncerKey = ref<string | null>(null)
const domainForm = reactive<DomainForm>(blankDomainForm())

const busy = computed(() => saving.value || pending.value > 0)
const drawerTitle = computed(() => {
  if (syncerKey.value) {
    return 'Syncer key'
  }
  return selectedDomainId.value ? '编辑域' : '新建域'
})

function openEditor(domain?: Domain) {
  selectedDomainId.value = domain?.id ?? null
  syncerKey.value = null
  Object.assign(domainForm, {
    ...blankDomainForm(),
    ...(domain
      ? {
          id: domain.id,
          name: domain.name,
          enabled: domain.enabled,
          mirror_root_dn: domain.mirror_root_dn,
          quarantine_ou_dn: domain.quarantine_ou_dn,
          upn_suffix: domain.upn_suffix,
          employee_id_attribute: domain.employee_id_attribute,
          managed_group_id_attribute: domain.managed_group_id_attribute,
        }
      : {}),
  })
  drawerOpen.value = true
}

async function saveDomain() {
  if (saving.value) {
    return
  }
  saving.value = true
  const editingId = selectedDomainId.value
  try {
    await runAction(
      async () => {
        const payload = {
          name: domainForm.name,
          enabled: domainForm.enabled,
          mirror_root_dn: domainForm.mirror_root_dn,
          quarantine_ou_dn: domainForm.quarantine_ou_dn,
          upn_suffix: domainForm.upn_suffix,
          employee_id_attribute: domainForm.employee_id_attribute,
          managed_group_id_attribute: domainForm.managed_group_id_attribute,
        }
        const response: DomainMutationResponse = editingId
          ? await adminFetch<DomainMutationResponse>(
              `/api/admin/domains/${encodeURIComponent(editingId)}`,
              { method: 'PATCH', body: JSON.stringify(payload) },
            )
          : await adminFetch<DomainMutationResponse>('/api/admin/domains', {
              method: 'POST',
              body: JSON.stringify({ id: domainForm.id, ...payload }),
            })
        selectedDomainId.value = response.domain.id
        syncerKey.value = response.syncer_key
        await Promise.all([loadDomains(), loadSyncDomains()])
      },
      { successMessage: editingId ? '域配置已更新' : '域配置已创建' },
    )
  } finally {
    saving.value = false
  }
}

async function copySyncerKey() {
  if (!syncerKey.value || !import.meta.client || !navigator.clipboard) {
    notifications.error('无法访问剪贴板，请手动复制 Syncer key')
    return
  }
  try {
    await navigator.clipboard.writeText(syncerKey.value)
    notifications.success('Syncer key 已复制')
  } catch {
    notifications.error('复制失败，请手动复制 Syncer key')
  }
}

async function refreshDomains() {
  await runAction(
    async () => {
      await Promise.all([loadDomains(), loadSyncDomains()])
    },
    { successMessage: '域数据已刷新' },
  )
}
</script>

<template>
  <AdminShell title="域">
    <template #actions>
      <Button icon="ph:arrows-clockwise" @click="refreshDomains">刷新</Button>
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

    <DomainTable
      :domains="domains"
      :loading="domainsLoading"
      :busy="busy"
      @edit="openEditor"
    />

    <Drawer
      v-model:visible="drawerOpen"
      :title="drawerTitle"
      :blocked="saving"
      :disabled="saving"
      size="large"
    >
      <div v-if="syncerKey" class="grid gap-[var(--spacing-lg)]">
        <Alert semantic="warning" title="Syncer key 仅显示一次">
          关闭后需要重新保存域配置才能生成新的 key。
        </Alert>
        <Input label="SYNCER_ACCESS_KEY" :model-value="syncerKey" readonly spellcheck="false" />
        <div class="flex gap-[var(--spacing-sm)]">
          <Button icon="ph:copy" @click="copySyncerKey">复制 key</Button>
        </div>
      </div>

      <DomainEditor
        v-else
        v-model="domainForm"
        :editing-id="selectedDomainId"
        :saving="saving"
        :attribute-defaults="domainAttributeDefaults"
        @save="saveDomain"
      />

      <template #footer>
        <Button :disabled="saving" @click="drawerOpen = false">
          {{ syncerKey ? '关闭' : '取消' }}
        </Button>
        <Button
          v-if="!syncerKey"
          form="domain-form"
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
