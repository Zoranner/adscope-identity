<script setup lang="ts">
import { Save } from 'lucide-vue-next'
import type { DomainForm } from '~/types/admin'

const form = defineModel<DomainForm>({ required: true })

defineProps<{
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
      <h2>{{ editingId ? '编辑域' : '创建域' }}</h2>
    </div>
    <div class="form-row">
      <div class="field">
        <label>域标识</label>
        <input v-model="form.id" :disabled="!!editingId" required />
      </div>
      <div class="field">
        <label>域名称</label>
        <input v-model="form.name" required />
      </div>
    </div>
    <div class="field">
      <label>UPN 后缀</label>
      <input v-model="form.upn_suffix" required />
    </div>
    <div class="field">
      <label>镜像根 DN</label>
      <input v-model="form.mirror_root_dn" required />
    </div>
    <div class="field">
      <label>隔离 OU DN</label>
      <input v-model="form.quarantine_ou_dn" required />
    </div>
    <div class="form-row">
      <div class="field">
        <label>工号属性</label>
        <input v-model="form.employee_id_attribute" required />
      </div>
      <div class="field">
        <label>受管组标识属性</label>
        <input v-model="form.managed_group_id_attribute" required />
      </div>
    </div>
    <div class="field">
      <label>状态</label>
      <select v-model="form.enabled">
        <option :value="true">启用</option>
        <option :value="false">停用</option>
      </select>
    </div>
    <div class="form-actions">
      <button class="primary-button" :disabled="loading || disabled">
        <Save :size="17" />
        保存
      </button>
      <button
        type="button"
        class="secondary-button"
        :disabled="loading || disabled"
        @click="$emit('reset')"
      >
        清空
      </button>
    </div>
  </form>
</template>
