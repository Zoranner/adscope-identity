<script setup lang="ts">
import { Save } from 'lucide-vue-next'
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
      <input v-model="form.id" :disabled="!!editingId" required />
    </div>
    <div class="field">
      <label>名称</label>
      <input v-model="form.name" required />
    </div>
    <div class="field">
      <label>父 OU</label>
      <select v-model="form.parent_id">
        <option value="">根 OU</option>
        <option
          v-for="item in items"
          :key="item.ou.id"
          :disabled="editingId === item.ou.id"
          :value="item.ou.id"
        >
          {{ `${'　'.repeat(item.depth)}${item.ou.name}` }}
        </option>
      </select>
    </div>
    <div class="form-actions">
      <button class="primary-button" :disabled="loading || disabled">
        <Save :size="17" />
        保存
      </button>
      <button type="button" class="secondary-button" @click="$emit('reset')">清空</button>
    </div>
  </form>
</template>
