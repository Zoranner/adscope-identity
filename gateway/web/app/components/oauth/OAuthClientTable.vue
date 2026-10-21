<script setup lang="ts">
import { Badge, Button, Table, Tag } from '@stellar/ui'
import type { OAuthClient } from '~/types/admin'

const props = defineProps<{
  clients: OAuthClient[]
  loading?: boolean
  busy?: boolean
}>()

const emit = defineEmits<{
  edit: [client: OAuthClient]
  toggle: [client: OAuthClient]
  remove: [client: OAuthClient]
}>()

const columns = [
  { key: 'name', title: '名称', minWidth: 200, ellipsis: true },
  { key: 'client_id', title: 'Client ID', minWidth: 200, ellipsis: true },
  { key: 'client_type', title: '类型', width: 96 },
  { key: 'enabled', title: '状态', width: 96 },
  { key: 'redirect', title: 'Redirect URI', minWidth: 240, ellipsis: true },
  { key: 'actions', title: '操作', width: 112, align: 'right' as const, fixed: 'right' as const },
]

const rows = computed(() => props.clients as unknown as Record<string, unknown>[])

function clientRow(row: Record<string, unknown>): OAuthClient {
  return row as unknown as OAuthClient
}

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
  <Table
    :columns="columns"
    :data="rows"
    :loading="loading"
    empty-text="还没有接入应用"
    layout="fixed"
    fixed-header
    row-key="client_id"
  >
    <template #cell-name="{ row }">
      <div class="grid min-w-0 gap-[var(--spacing-2xs)]">
        <strong>{{ clientRow(row).name }}</strong>
        <span class="flex flex-wrap gap-[var(--spacing-xs)]">
          <Tag v-for="scope in clientRow(row).allowed_scopes" :key="scope" size="small">
            {{ scope }}
          </Tag>
        </span>
      </div>
    </template>
    <template #cell-client_type="{ row }">
      {{ clientRow(row).client_type === 'web' ? 'Web' : 'Desktop' }}
    </template>
    <template #cell-enabled="{ row }">
      <Badge :semantic="clientRow(row).enabled ? 'success' : 'warning'">
        {{ clientRow(row).enabled ? '启用' : '停用' }}
      </Badge>
    </template>
    <template #cell-redirect="{ row }">
      <span :title="clientRow(row).redirect_uris.join('\n')">
        {{ redirectSummary(clientRow(row)) }}
      </span>
    </template>
    <template #cell-actions="{ row }">
      <div class="flex justify-end gap-[var(--spacing-xs)]">
        <Button
          size="small"
          square
          icon="ph:pencil-simple"
          title="编辑应用"
          aria-label="编辑应用"
          :disabled="busy"
          @click="emit('edit', clientRow(row))"
        />
        <Button
          size="small"
          square
          icon="ph:power"
          :title="clientRow(row).enabled ? '停用应用' : '启用应用'"
          :aria-label="clientRow(row).enabled ? '停用应用' : '启用应用'"
          :disabled="busy"
          @click="emit('toggle', clientRow(row))"
        />
        <Button
          size="small"
          square
          variant="solid"
          semantic="error"
          icon="ph:trash"
          title="删除应用"
          aria-label="删除应用"
          :disabled="busy"
          @click="emit('remove', clientRow(row))"
        />
      </div>
    </template>
  </Table>
</template>
