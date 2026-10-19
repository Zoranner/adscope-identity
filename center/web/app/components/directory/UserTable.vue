<script setup lang="ts">
import { Badge, Button, Table } from '@stellar/ui'
import type { UserRecord } from '~/types/admin'

const props = defineProps<{
  users: UserRecord[]
  loading?: boolean
  busy?: boolean
}>()

const emit = defineEmits<{
  edit: [user: UserRecord]
  resetPassword: [user: UserRecord]
  remove: [user: UserRecord]
}>()

const columns = [
  { key: 'employee_id', title: '工号', width: 116, ellipsis: true },
  { key: 'username', title: '登录名', width: 132, ellipsis: true },
  { key: 'display_name', title: '显示名', minWidth: 132, ellipsis: true },
  { key: 'email', title: '邮箱', minWidth: 160, ellipsis: true },
  { key: 'mobile', title: '手机', width: 104, ellipsis: true },
  { key: 'status', title: '状态', width: 84 },
  { key: 'actions', title: '操作', width: 112, align: 'right' as const, fixed: 'right' as const },
]

const rows = computed(() => props.users as unknown as Record<string, unknown>[])

function userRow(row: Record<string, unknown>): UserRecord {
  return row as unknown as UserRecord
}

function openEditor(row: Record<string, unknown>) {
  emit('edit', userRow(row))
}
</script>

<template>
  <Table
    :columns="columns"
    :data="rows"
    :loading="loading"
    empty-text="当前 OU 下还没有用户"
    layout="fixed"
    fixed-header
    row-key="employee_id"
    @row-click="openEditor"
  >
    <template #cell-status="{ row }">
      <Badge :semantic="userRow(row).status === 'active' ? 'success' : 'error'">
        {{ userRow(row).status === 'active' ? '启用' : '禁用' }}
      </Badge>
    </template>
    <template #cell-actions="{ row }">
      <div class="flex justify-end gap-[var(--spacing-xs)]">
        <Button
          size="small"
          square
          icon="ph:pencil-simple"
          title="编辑用户"
          aria-label="编辑用户"
          :disabled="busy"
          @click.stop="emit('edit', userRow(row))"
        />
        <Button
          size="small"
          square
          icon="ph:key"
          title="重置"
          aria-label="重置"
          :disabled="busy"
          @click.stop="emit('resetPassword', userRow(row))"
        />
        <Button
          size="small"
          square
          variant="solid"
          semantic="error"
          icon="ph:trash"
          title="删除"
          aria-label="删除"
          :disabled="busy"
          @click.stop="emit('remove', userRow(row))"
        />
      </div>
    </template>
  </Table>
</template>
