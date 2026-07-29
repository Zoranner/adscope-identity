<script setup lang="ts">
import SyncTable from '~/components/sync/SyncTable.vue'

const { tokenReady, loading, syncDomains, loadSyncDomains, runAction } = useAdminApi()

async function refreshSync() {
  await runAction(async () => {
    await loadSyncDomains()
  }, { successMessage: '同步状态已刷新' })
}
</script>

<template>
  <AdminShell>
    <AdminPageHeader
      title="同步状态"
      description="查看各域已确认 revision 和当前滞后，不引入中心主动推送。"
      :loading="loading"
      :disabled="!tokenReady"
      @refresh="refreshSync"
    />

    <SyncTable :domains="syncDomains" />
  </AdminShell>
</template>
