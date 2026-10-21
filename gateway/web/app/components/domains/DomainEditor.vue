<script setup lang="ts">
import { Input, Select } from '@stellar/ui'
import type { DomainAttributeDefaults, DomainForm } from '~/types/admin'
import EditorForm from '~/components/shell/EditorForm.vue'

const form = defineModel<DomainForm>({ required: true })

const props = defineProps<{
  editingId: string | null
  saving?: boolean
  attributeDefaults: DomainAttributeDefaults
}>()

defineEmits<{
  save: []
}>()

const enabledOptions = [
  { value: true, label: '启用' },
  { value: false, label: '停用' },
]
</script>

<template>
  <EditorForm form-id="domain-form" @submit="$emit('save')">
    <div class="grid grid-cols-2 gap-[var(--spacing-lg)] max-[720px]:grid-cols-1">
      <Input
        v-model="form.id"
        label="域标识"
        :disabled="!!editingId"
        :help-text="editingId ? '标识创建后不可修改' : 'Connector 匹配该域时使用'"
        required
      />
      <Input v-model="form.name" label="域名称" required />
    </div>

    <div class="grid grid-cols-2 gap-[var(--spacing-lg)] max-[720px]:grid-cols-1">
      <Input v-model="form.upn_suffix" label="UPN 后缀" help-text="例如 example.com" required />
      <Select v-model="form.enabled" label="状态" :options="enabledOptions" />
    </div>

    <Input v-model="form.mirror_root_dn" label="镜像根 DN" required />
    <Input v-model="form.quarantine_ou_dn" label="隔离 OU DN" required />

    <div class="grid grid-cols-2 gap-[var(--spacing-lg)] max-[720px]:grid-cols-1">
      <Input
        v-model="form.employee_id_attribute"
        label="工号属性"
        :placeholder="props.attributeDefaults.employee_id_attribute"
      />
      <Input
        v-model="form.managed_group_id_attribute"
        label="受管组标识属性"
        :placeholder="props.attributeDefaults.managed_group_id_attribute"
      />
    </div>
  </EditorForm>
</template>
