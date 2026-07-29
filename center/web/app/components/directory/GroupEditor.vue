<script setup lang="ts">
import { Save } from 'lucide-vue-next'
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
      <input v-model="form.id" :disabled="!!editingId" required />
    </div>
    <div class="field">
      <label>组名称</label>
      <input v-model="form.name" required />
    </div>
    <div class="field">
      <label>所属 OU</label>
      <select v-model="form.organizational_unit_id" required>
        <option v-for="item in items" :key="item.ou.id" :value="item.ou.id">
          {{ `${'　'.repeat(item.depth)}${item.ou.name}` }}
        </option>
      </select>
    </div>
    <div class="field">
      <label>成员工号</label>
      <textarea v-model="form.member_employee_ids" placeholder="用英文逗号分隔"></textarea>
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
