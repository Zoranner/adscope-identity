<script setup lang="ts">
import { Search, UserPlus } from 'lucide-vue-next'
import type { UserRecord } from '~/types/admin'

const props = defineProps<{
  users: UserRecord[]
  disabled?: boolean
}>()

defineEmits<{
  create: []
  edit: [user: UserRecord]
}>()

const query = ref('')
const filteredUsers = computed(() => {
  const value = query.value.trim().toLowerCase()
  if (!value) {
    return props.users
  }
  return props.users.filter((user) =>
    [user.employee_id, user.username, user.display_name, user.email, user.mobile, user.telephone]
      .filter(Boolean)
      .some((field) => field!.toLowerCase().includes(value)),
  )
})
const pagination = usePagination(filteredUsers, 8)

watch(query, () => pagination.resetPage())
</script>

<template>
  <section class="panel">
    <div class="panel-header">
      <div class="panel-title">
        <h2>用户</h2>
        <AdminStatusBadge>{{ filteredUsers.length }}</AdminStatusBadge>
      </div>
      <button class="primary-button compact-button" :disabled="disabled" @click="$emit('create')">
        <UserPlus :size="15" />
        新建
      </button>
    </div>

    <div class="table-toolbar">
      <Search :size="16" />
      <input v-model="query" placeholder="搜索工号、姓名、账号或联系方式" />
    </div>

    <div class="table-wrap">
      <table v-if="filteredUsers.length">
        <thead>
          <tr>
            <th>工号</th>
            <th>用户</th>
            <th>联系方式</th>
            <th>状态</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="user in pagination.pageItems.value" :key="user.employee_id">
            <td>{{ user.employee_id }}</td>
            <td>
              <strong>{{ user.display_name }}</strong>
              <span class="muted-cell">{{ user.username }}</span>
            </td>
            <td>{{ user.email ?? user.mobile ?? user.telephone ?? '-' }}</td>
            <td>
              <AdminStatusBadge :tone="user.status === 'disabled' ? 'warn' : 'default'">
                {{ user.status === 'active' ? '启用' : '禁用' }}
              </AdminStatusBadge>
            </td>
            <td class="actions-cell">
              <button class="secondary-button compact-button" @click="$emit('edit', user)">编辑</button>
            </td>
          </tr>
        </tbody>
      </table>
      <AdminEmptyState v-else title="该 OU 下暂无用户" />
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
