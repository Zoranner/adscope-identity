<script setup lang="ts">
import { Save } from 'lucide-vue-next'
import { Button, Input, Select } from '@stellar/ui'
import type { OuForm, OuTreeItem } from '~/types/admin'

const form = defineModel<OuForm>({ required: true })

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
      <h2>{{ editingId ? '编辑 OU' : '创建 OU' }}</h2>
    </div>
    <div class="field">
      <label>OU 标识</label>
      <Input v-model="form.id" label="OU 标识" :disabled="!!editingId" required />
    </div>
    <div class="field">
      <label>名称</label>
      <Input v-model="form.name" label="名称" required />
    </div>
    <div class="field">
      <label>父 OU</label>
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
    </div>
    <div class="form-actions">
      <Button type="submit" variant="solid" :disabled="loading || disabled">
        保存
      </Button>
      <Button type="button" variant="outline" @click="$emit('reset')">清空</Button>
    </div>
  </form>
</template>
