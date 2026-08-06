<script setup lang="ts">
import { KeyRound, Pencil, Plus, Power, PowerOff, Search, Trash2 } from 'lucide-vue-next'
import type { OAuthClient } from '~/types/admin'

const props = defineProps<{
  clients: OAuthClient[]
  busy?: boolean
}>()

defineEmits<{
  create: []
  edit: [client: OAuthClient]
  toggle: [client: OAuthClient]
  delete: [client: OAuthClient]
}>()

const query = ref('')
const filteredClients = computed(() => {
  const value = query.value.trim().toLowerCase()
  if (!value) {
    return props.clients
  }
  return props.clients.filter((client) =>
    [
      client.name,
      client.client_id,
      client.client_type,
      ...client.redirect_uris,
      ...client.allowed_scopes,
    ].some((field) => field.toLowerCase().includes(value)),
  )
})
const pagination = usePagination(filteredClients, 10)

watch(query, () => pagination.resetPage())

function redirectSummary(client: OAuthClient): string {
  const first = client.redirect_uris[0]
  if (!first) {
    return '-'
  }
  const remaining = client.redirect_uris.length - 1
  return remaining > 0 ? `${first}（另有 ${remaining} 个）` : first
}
</script>

<template>
  <section class="panel">
    <div class="panel-header">
      <div class="panel-title">
        <h2>登录客户端</h2>
        <AdminStatusBadge>{{ filteredClients.length }}</AdminStatusBadge>
      </div>
      <button
        class="primary-button compact-button"
        :disabled="busy"
        @click="$emit('create')"
      >
        <Plus :size="15" />
        新建
      </button>
    </div>

    <div class="table-toolbar">
      <Search :size="16" />
      <input
        v-model="query"
        :disabled="busy"
        placeholder="搜索名称、Client ID、类型、Redirect URI 或 scope"
      />
    </div>

    <div class="table-wrap">
      <table v-if="filteredClients.length">
        <thead>
          <tr>
            <th>名称</th>
            <th>Client ID</th>
            <th>类型</th>
            <th>状态</th>
            <th>Redirect URI</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="client in pagination.pageItems.value" :key="client.client_id">
            <td class="oauth-client-name">
              <strong>{{ client.name }}</strong>
              <span class="muted-cell oauth-client-scopes">
                {{ client.allowed_scopes.join(' ') }}
              </span>
            </td>
            <td class="oauth-client-id" :title="client.client_id">{{ client.client_id }}</td>
            <td>
              <AdminStatusBadge :tone="client.client_type === 'desktop' ? 'warn' : 'default'">
                {{ client.client_type === 'web' ? 'Web' : 'Desktop' }}
              </AdminStatusBadge>
            </td>
            <td>
              <AdminStatusBadge :tone="client.enabled ? 'default' : 'warn'">
                {{ client.enabled ? '启用' : '停用' }}
              </AdminStatusBadge>
            </td>
            <td class="oauth-client-redirect" :title="client.redirect_uris.join('\n')">
              {{ redirectSummary(client) }}
            </td>
            <td class="actions-cell">
              <div class="row-actions">
                <button
                  class="icon-button"
                  title="编辑客户端"
                  aria-label="编辑客户端"
                  :disabled="busy"
                  @click="$emit('edit', client)"
                >
                  <Pencil :size="16" />
                </button>
                <button
                  class="icon-button"
                  :title="client.enabled ? '停用客户端' : '启用客户端'"
                  :aria-label="client.enabled ? '停用客户端' : '启用客户端'"
                  :disabled="busy"
                  @click="$emit('toggle', client)"
                >
                  <PowerOff v-if="client.enabled" :size="16" />
                  <Power v-else :size="16" />
                </button>
                <button
                  v-if="client.client_type === 'web'"
                  class="icon-button"
                  title="编辑并管理密钥"
                  aria-label="编辑并管理密钥"
                  :disabled="busy"
                  @click="$emit('edit', client)"
                >
                  <KeyRound :size="16" />
                </button>
                <button
                  class="danger-button compact-button"
                  title="删除客户端"
                  aria-label="删除客户端"
                  :disabled="busy"
                  @click="$emit('delete', client)"
                >
                  <Trash2 :size="16" />
                </button>
              </div>
            </td>
          </tr>
        </tbody>
      </table>
      <AdminEmptyState v-else title="暂无登录客户端" />
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
