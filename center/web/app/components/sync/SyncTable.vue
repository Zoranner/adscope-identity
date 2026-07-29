<script setup lang="ts">
import { Search } from 'lucide-vue-next'
import type { SyncDomain } from '~/types/admin'

const props = defineProps<{
  domains: SyncDomain[]
}>()

const query = ref('')
const filteredDomains = computed(() => {
  const value = query.value.trim().toLowerCase()
  if (!value) {
    return props.domains
  }
  return props.domains.filter((domain) => domain.domain_id.toLowerCase().includes(value))
})
const directoryLag = computed(() =>
  props.domains.reduce((sum, domain) => sum + domain.directory_lag, 0),
)
const credentialLag = computed(() =>
  props.domains.reduce((sum, domain) => sum + domain.credential_lag, 0),
)
const pagination = usePagination(filteredDomains, 12)

watch(query, () => pagination.resetPage())
</script>

<template>
  <section class="panel">
    <div class="panel-header">
      <div class="panel-title">
        <h2>域同步状态</h2>
        <AdminStatusBadge>目录滞后 {{ directoryLag }}</AdminStatusBadge>
        <AdminStatusBadge :tone="credentialLag > 0 ? 'warn' : 'default'">
          凭据滞后 {{ credentialLag }}
        </AdminStatusBadge>
      </div>
    </div>

    <div class="table-toolbar">
      <Search :size="16" />
      <input v-model="query" placeholder="搜索域标识" />
    </div>

    <div class="table-wrap">
      <table v-if="filteredDomains.length">
        <thead>
          <tr>
            <th>域</th>
            <th>状态</th>
            <th>已确认目录</th>
            <th>已确认凭据</th>
            <th>目录滞后</th>
            <th>凭据滞后</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="domain in pagination.pageItems.value" :key="domain.domain_id">
            <td>{{ domain.domain_id }}</td>
            <td>
              <AdminStatusBadge :tone="domain.enabled ? 'default' : 'warn'">
                {{ domain.enabled ? '启用' : '停用' }}
              </AdminStatusBadge>
            </td>
            <td>{{ domain.applied_directory_revision }}</td>
            <td>{{ domain.applied_credential_revision }}</td>
            <td>{{ domain.directory_lag }}</td>
            <td>{{ domain.credential_lag }}</td>
          </tr>
        </tbody>
      </table>
      <AdminEmptyState v-else title="暂无同步状态" />
    </div>

    <AdminPaginationBar
      :page="pagination.page.value"
      :page-count="pagination.pageCount.value"
      :total="pagination.total.value"
      :start="pagination.start.value"
      :end="pagination.end.value"
      @previous="pagination.previousPage"
      @next="pagination.nextPage"
    />
  </section>
</template>
