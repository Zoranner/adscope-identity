<script setup lang="ts">
import { Button, Table } from '@stellar/ui'
import type { GroupRecord } from '~/types/admin'

const props = defineProps<{
  groups: GroupRecord[]
  loading?: boolean
  busy?: boolean
}>()

const emit = defineEmits<{
  edit: [group: GroupRecord]
}>()

const columns = [
  { key: 'id', title: '组标识', width: 200, ellipsis: true },
  { key: 'name', title: '组名称', minWidth: 200, ellipsis: true },
  { key: 'members', title: '成员', width: 96 },
  { key: 'actions', title: '操作', width: 72, align: 'right' as const, fixed: 'right' as const },
]

const rows = computed(() => props.groups as unknown as Record<string, unknown>[])

function groupRow(row: Record<string, unknown>): GroupRecord {
  return row as unknown as GroupRecord
}

function openEditor(row: Record<string, unknown>) {
  emit('edit', groupRow(row))
}
</script>

<template>
  <Table
    :columns="columns"
    :data="rows"
    :loading="loading"
    empty-text="当前 OU 下还没有安全组"
    layout="fixed"
    fixed-header
    row-key="id"
    @row-click="openEditor"
  >
    <template #cell-members="{ row }">
      {{ groupRow(row).member_employee_ids.length }}
    </template>
    <template #cell-actions="{ row }">
      <div class="flex justify-end">
        <Button
          size="small"
          square
          icon="ph:pencil-simple"
          title="编辑安全组"
          aria-label="编辑安全组"
          :disabled="busy"
          @click.stop="emit('edit', groupRow(row))"
        />
      </div>
    </template>
  </Table>
</template>
