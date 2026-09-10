<script setup lang="ts">
import { Save } from 'lucide-vue-next'
import { Button, Input, Select, Textarea } from '@stellar/ui'
import type { GroupForm, OuTreeItem } from '~/types/admin'

const form = defineModel<GroupForm>({ required: true })

defineProps<{
  items: OuTreeItem[]
  editingId: string | null
  loading?: boolean
  disabled?: boolean
}>()

defineEmits<{
  save: []
  reset: []
}>()
</script>

<template>
  <form class="panel form" @submit.prevent="$emit('save')">
    <div class="panel-header compact-header">
      <h2>{{ editingId ? '编辑组' : '创建组' }}</h2>
    </div>
    <div class="field">
      <label>组标识</label>
      <Input v-model="form.id" label="组标识" :disabled="!!editingId" required />
    </div>
    <div class="field">
      <label>组名称</label>
      <Input v-model="form.name" label="组名称" required />
    </div>
    <div class="field">
      <label>所属 OU</label>
      <Select
        v-model="form.organizational_unit_id"
        label="所属 OU"
        :options="items.map((item) => ({ value: item.ou.id, label: `${'　'.repeat(item.depth)}${item.ou.name}` }))"
      />
    </div>
    <div class="field">
      <label>成员工号</label>
      <Textarea v-model="form.member_employee_ids" label="成员工号" placeholder="用英文逗号分隔" />
    </div>
    <div class="form-actions">
      <Button type="submit" variant="solid" :disabled="loading || disabled">
        保存
      </Button>
      <Button type="button" variant="outline" @click="$emit('reset')">清空</Button>
    </div>
  </form>
</template>
