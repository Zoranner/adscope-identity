<script setup lang="ts">
import { Input, Select } from '@stellar/ui'
import type { OuForm, OuTreeItem } from '~/types/admin'
import EditorForm from '~/components/shell/EditorForm.vue'

const form = defineModel<OuForm>({ required: true })

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
  <EditorForm form-id="ou-form" @submit="$emit('save')">
    <Input
      v-model="form.id"
      label="OU 标识"
      :disabled="!!editingId"
      :help-text="editingId ? '标识创建后不可修改' : '同步到 AD 时使用的标识'"
      required
    />
    <Input v-model="form.name" label="名称" required />
    <Select
      v-model="form.parent_id"
      label="父 OU"
      :options="[
        { value: '', label: '根 OU' },
        ...items.map((item) => ({
          value: item.ou.id,
          label: `${'　'.repeat(item.depth)}${item.ou.name}`,
          disabled: editingId === item.ou.id,
        })),
      ]"
    />
  </EditorForm>
</template>
