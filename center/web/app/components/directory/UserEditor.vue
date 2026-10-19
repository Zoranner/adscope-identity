<script setup lang="ts">
import { Input, Select } from '@stellar/ui'
import type { OuTreeItem, UserForm } from '~/types/admin'
import EditorForm from '~/components/shell/EditorForm.vue'

const form = defineModel<UserForm>({ required: true })

const composedDisplayName = computed(
  () => `${form.value.surname.trim()}${form.value.given_name.trim()}`,
)

defineProps<{
  items: OuTreeItem[]
  editingId: string | null
}>()

defineEmits<{
  save: []
}>()

const statusOptions = [
  { value: 'active', label: '启用' },
  { value: 'disabled', label: '禁用' },
]
</script>

<template>
  <EditorForm form-id="user-form" @submit="$emit('save')">
    <Input
      v-model="form.employee_id"
      label="工号"
      disabled
      help-text="由系统分配，不可修改"
    />
    <Input v-model="form.username" label="登录名" required />

    <div class="grid grid-cols-2 gap-[var(--spacing-lg)] max-[720px]:grid-cols-1">
      <Input v-model="form.surname" label="姓" required />
      <Input v-model="form.given_name" label="名" required />
    </div>

    <Input
      v-model="form.display_name"
      label="显示名"
      :placeholder="composedDisplayName"
      help-text="留空则由姓 + 名组成"
    />

    <Select v-model="form.status" label="状态" :options="statusOptions" />

    <Select
      v-model="form.organizational_unit_id"
      label="所属 OU"
      :options="items.map((item) => ({
        value: item.ou.id,
        label: `${'　'.repeat(item.depth)}${item.ou.name}`,
      }))"
      required
    />

    <Input v-model="form.email" label="邮箱" type="email" />
    <Input v-model="form.mobile" label="手机" />

    <Input
      v-if="!editingId"
      v-model="form.initial_password"
      label="初始密码"
      type="password"
      help-text="创建后由用户本人登录修改"
      required
    />

  </EditorForm>
</template>
