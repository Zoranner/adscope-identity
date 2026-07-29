<script setup lang="ts">
import { Search } from 'lucide-vue-next'
import type { GroupRecord, UserRecord } from '~/types/admin'

type DirectoryObjectRow =
  | {
      type: 'user'
      key: string
      id: string
      name: string
      detail: string
      state: string
      revision: string
      record: UserRecord
    }
  | {
      type: 'group'
      key: string
      id: string
      name: string
      detail: string
      state: string
      revision: string
      record: GroupRecord
    }

const props = defineProps<{
  users: UserRecord[]
  groups: GroupRecord[]
}>()

defineEmits<{
  editUser: [user: UserRecord]
  editGroup: [group: GroupRecord]
}>()

const query = ref('')
const rows = computed<DirectoryObjectRow[]>(() => [
  ...props.users.map((user) => ({
    type: 'user' as const,
    key: `user:${user.employee_id}`,
    id: user.employee_id,
    name: user.display_name,
    detail: [user.username, user.email, user.mobile].filter(Boolean).join(' / '),
    state: user.status === 'active' ? '启用' : '禁用',
    revision: '-',
    record: user,
  })),
  ...props.groups.map((group) => ({
    type: 'group' as const,
    key: `group:${group.id}`,
    id: group.id,
    name: group.name,
    detail: `${group.member_employee_ids.length} 个成员`,
    state: '安全组',
    revision: String(group.changed_revision),
    record: group,
  })),
])
const filteredRows = computed(() => {
  const value = query.value.trim().toLowerCase()
  if (!value) {
    return rows.value
  }
  return rows.value.filter((row) =>
    [row.type === 'user' ? '用户' : '安全组', row.id, row.name, row.detail, row.state]
      .some((field) => field.toLowerCase().includes(value)),
  )
})
const pagination = usePagination(filteredRows, 12)

watch(query, () => pagination.resetPage())
</script>

<template>
  <section class="panel">
    <div class="panel-header">
      <div class="panel-title">
        <h2>对象</h2>
        <AdminStatusBadge>{{ filteredRows.length }}</AdminStatusBadge>
      </div>
      <div class="panel-tools">
        <label class="inline-search">
          <Search :size="16" />
          <input v-model="query" placeholder="搜索类型、标识、名称或详情" />
        </label>
      </div>
    </div>

    <div class="table-wrap">
      <table v-if="filteredRows.length">
        <thead>
          <tr>
            <th>类型</th>
            <th>标识</th>
            <th>名称</th>
            <th>详情</th>
            <th>状态</th>
            <th>Revision</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="row in pagination.pageItems.value" :key="row.key">
            <td>
              <AdminStatusBadge :tone="row.type === 'group' ? 'warn' : 'default'">
                {{ row.type === 'user' ? '用户' : '安全组' }}
              </AdminStatusBadge>
            </td>
            <td>{{ row.id }}</td>
            <td>{{ row.name }}</td>
            <td>{{ row.detail || '-' }}</td>
            <td>{{ row.state }}</td>
            <td>{{ row.revision }}</td>
            <td class="actions-cell">
              <button
                v-if="row.type === 'user'"
                class="secondary-button compact-button"
                @click="$emit('editUser', row.record)"
              >
                编辑
              </button>
              <button
                v-else
                class="secondary-button compact-button"
                @click="$emit('editGroup', row.record)"
              >
                编辑
              </button>
            </td>
          </tr>
        </tbody>
      </table>
      <AdminEmptyState v-else title="当前 OU 下暂无对象" />
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
