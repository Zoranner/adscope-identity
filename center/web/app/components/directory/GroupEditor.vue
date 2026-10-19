<script setup lang="ts">
import { Input, Select, Textarea } from '@stellar/ui'
import type { GroupForm, OuTreeItem } from '~/types/admin'
import EditorForm from '~/components/shell/EditorForm.vue'

const form = defineModel<GroupForm>({ required: true })

defineProps<{
  items: OuTreeItem[]
  editingId: string | null
  saving?: boolean
}>()

defineEmits<{
  save: []
}>()
</script>

<template>
  <EditorForm form-id="group-form" @submit="$emit('save')">
    <Input
      v-model="form.id"
      label="组标识"
      :disabled="!!editingId"
      :help-text="editingId ? '标识创建后不可修改' : '同步到 AD 时使用的标识'"
      required
    />
    <Input v-model="form.name" label="组名称" required />
    <Select
      v-model="form.organizational_unit_id"
      label="所属 OU"
      :options="items.map((item) => ({
        value: item.ou.id,
        label: `${'　'.repeat(item.depth)}${item.ou.name}`,
      }))"
      required
    />
    <Textarea
      v-model="form.member_employee_ids"
      label="成员工号"
      placeholder="用英文逗号分隔，例如 1001, 1002"
      help-text="保存后成员关系会同步到该组所属 AD 域"
      :rows="4"
    />
  </EditorForm>
</template>
