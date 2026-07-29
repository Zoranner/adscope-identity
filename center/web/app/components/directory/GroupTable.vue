<script setup lang="ts">
import { Search, ShieldPlus } from 'lucide-vue-next'
import type { GroupRecord } from '~/types/admin'

const props = defineProps<{
  groups: GroupRecord[]
  disabled?: boolean
}>()

defineEmits<{
  create: []
  edit: [group: GroupRecord]
}>()

const query = ref('')
const filteredGroups = computed(() => {
  const value = query.value.trim().toLowerCase()
  if (!value) {
    return props.groups
  }
  return props.groups.filter((group) =>
    [group.id, group.name, group.member_employee_ids.join(',')]
      .some((field) => field.toLowerCase().includes(value)),
  )
})
const pagination = usePagination(filteredGroups, 8)

watch(query, () => pagination.resetPage())
</script>

<template>
  <section class="panel">
    <div class="panel-header">
      <div class="panel-title">
        <h2>安全组</h2>
        <AdminStatusBadge>{{ filteredGroups.length }}</AdminStatusBadge>
      </div>
      <button class="primary-button compact-button" :disabled="disabled" @click="$emit('create')">
        <ShieldPlus :size="15" />
        新建
      </button>
    </div>

    <div class="table-toolbar">
      <Search :size="16" />
      <input v-model="query" placeholder="搜索组名、组标识或成员工号" />
    </div>

    <div class="table-wrap">
      <table v-if="filteredGroups.length">
        <thead>
          <tr>
            <th>组</th>
            <th>成员数</th>
            <th>Revision</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="group in pagination.pageItems.value" :key="group.id">
            <td>
              <strong>{{ group.name }}</strong>
              <span class="muted-cell">{{ group.id }}</span>
            </td>
            <td>{{ group.member_employee_ids.length }}</td>
            <td>{{ group.changed_revision }}</td>
            <td class="actions-cell">
              <button class="secondary-button compact-button" @click="$emit('edit', group)">编辑</button>
            </td>
          </tr>
        </tbody>
      </table>
      <AdminEmptyState v-else title="该 OU 下暂无安全组" />
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
