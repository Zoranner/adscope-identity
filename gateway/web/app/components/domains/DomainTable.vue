<script setup lang="ts">
import { Badge, Button, Table } from '@stellar/ui'
import type { Domain } from '~/types/admin'

const props = defineProps<{
  domains: Domain[]
  loading?: boolean
  busy?: boolean
}>()

const emit = defineEmits<{
  edit: [domain: Domain]
}>()

const columns = [
  { key: 'domain', title: '域', minWidth: 200, ellipsis: true },
  { key: 'upn_suffix', title: 'UPN 后缀', width: 180, ellipsis: true },
  { key: 'mirror_root_dn', title: '镜像根 DN', minWidth: 220, ellipsis: true },
  { key: 'enabled', title: '状态', width: 96 },
  { key: 'applied_directory_revision', title: '目录版本', width: 108, align: 'right' as const },
  { key: 'applied_credential_revision', title: '凭据版本', width: 108, align: 'right' as const },
  { key: 'actions', title: '操作', width: 72, align: 'right' as const, fixed: 'right' as const },
]

const rows = computed(() => props.domains as unknown as Record<string, unknown>[])

function domainRow(row: Record<string, unknown>): Domain {
  return row as unknown as Domain
}

function openEditor(row: Record<string, unknown>) {
  emit('edit', domainRow(row))
}
</script>

<template>
  <Table
    :columns="columns"
    :data="rows"
    :loading="loading"
    empty-text="还没有配置 AD 域"
    layout="fixed"
    fixed-header
    row-key="id"
    @row-click="openEditor"
  >
    <template #cell-domain="{ row }">
      <div class="grid min-w-0 gap-[var(--spacing-2xs)]">
        <strong>{{ domainRow(row).name }}</strong>
        <span class="text-[var(--text-sm)] text-[var(--st-text-muted)]">{{ domainRow(row).id }}</span>
      </div>
    </template>
    <template #cell-enabled="{ row }">
      <Badge :semantic="domainRow(row).enabled ? 'success' : 'warning'">
        {{ domainRow(row).enabled ? '启用' : '停用' }}
      </Badge>
    </template>
    <template #cell-actions="{ row }">
      <div class="flex justify-end">
        <Button
          size="small"
          square
          icon="ph:pencil-simple"
          title="编辑域"
          aria-label="编辑域"
          :disabled="busy"
          @click.stop="emit('edit', domainRow(row))"
        />
      </div>
    </template>
  </Table>
</template>
