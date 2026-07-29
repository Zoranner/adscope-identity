<script setup lang="ts">
import { Search, ServerCog } from 'lucide-vue-next'
import type { Domain } from '~/types/admin'

const props = defineProps<{
  domains: Domain[]
}>()

defineEmits<{
  create: []
  edit: [domain: Domain]
}>()

const query = ref('')
const filteredDomains = computed(() => {
  const value = query.value.trim().toLowerCase()
  if (!value) {
    return props.domains
  }
  return props.domains.filter((domain) =>
    [domain.id, domain.name, domain.upn_suffix, domain.mirror_root_dn]
      .some((field) => field.toLowerCase().includes(value)),
  )
})
const pagination = usePagination(filteredDomains, 10)

watch(query, () => pagination.resetPage())
</script>

<template>
  <section class="panel">
    <div class="panel-header">
      <div class="panel-title">
        <h2>域列表</h2>
        <AdminStatusBadge>{{ filteredDomains.length }}</AdminStatusBadge>
      </div>
      <button class="primary-button compact-button" @click="$emit('create')">
        <ServerCog :size="15" />
        新建
      </button>
    </div>

    <div class="table-toolbar">
      <Search :size="16" />
      <input v-model="query" placeholder="搜索域、UPN 后缀或镜像根 DN" />
    </div>

    <div class="table-wrap">
      <table v-if="filteredDomains.length">
        <thead>
          <tr>
            <th>域</th>
            <th>UPN</th>
            <th>状态</th>
            <th>目录</th>
            <th>凭据</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="domain in pagination.pageItems.value" :key="domain.id">
            <td>
              <strong>{{ domain.name }}</strong>
              <span class="muted-cell">{{ domain.id }}</span>
            </td>
            <td>{{ domain.upn_suffix }}</td>
            <td>
              <AdminStatusBadge :tone="domain.enabled ? 'default' : 'warn'">
                {{ domain.enabled ? '启用' : '停用' }}
              </AdminStatusBadge>
            </td>
            <td>{{ domain.applied_directory_revision }}</td>
            <td>{{ domain.applied_credential_revision }}</td>
            <td class="actions-cell">
              <button class="secondary-button compact-button" @click="$emit('edit', domain)">编辑</button>
            </td>
          </tr>
        </tbody>
      </table>
      <AdminEmptyState v-else title="暂无域配置" />
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
