<script setup lang="ts">
import { Save } from 'lucide-vue-next'
import { Button, Input, Select } from '@stellar/ui'
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
        <Input v-model="form.id" label="域标识" :disabled="!!editingId" required />
      </div>
      <div class="field">
        <label>域名称</label>
        <Input v-model="form.name" label="域名称" required />
      </div>
    </div>
    <div class="field">
      <label>UPN 后缀</label>
      <Input v-model="form.upn_suffix" label="UPN 后缀" required />
    </div>
    <div class="field">
      <label>镜像根 DN</label>
      <Input v-model="form.mirror_root_dn" label="镜像根 DN" required />
    </div>
    <div class="field">
      <label>隔离 OU DN</label>
      <Input v-model="form.quarantine_ou_dn" label="隔离 OU DN" required />
    </div>
    <div class="form-row">
      <div class="field">
        <label>工号属性</label>
        <Input v-model="form.employee_id_attribute" label="工号属性" required />
      </div>
      <div class="field">
        <label>受管组标识属性</label>
        <Input v-model="form.managed_group_id_attribute" label="受管组标识属性" required />
      </div>
    </div>
    <div class="field">
      <label>状态</label>
      <Select v-model="form.enabled" label="状态" :options="[
        { value: true, label: '启用' },
        { value: false, label: '停用' },
      ]" />
    </div>
    <div class="form-actions">
      <Button type="submit" variant="solid" :disabled="loading || disabled">
        <Save :size="17" />
        保存
      </Button>
      <Button
        type="button"
        variant="outline"
        :disabled="loading || disabled"
        @click="$emit('reset')"
      >
        清空
      </Button>
    </div>
  </form>
</template>
