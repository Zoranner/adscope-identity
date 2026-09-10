<script setup lang="ts">
import { KeyRound, Save } from 'lucide-vue-next'
import { Button, Input, Select } from '@stellar/ui'
import type { OuTreeItem, UserForm } from '~/types/admin'

const form = defineModel<UserForm>({ required: true })

defineProps<{
  items: OuTreeItem[]
  editingId: string | null
  loading?: boolean
  disabled?: boolean
}>()

defineEmits<{
  save: []
  reset: []
  enable: []
  disable: []
  resetPassword: []
}>()
</script>

<template>
  <form class="panel form" @submit.prevent="$emit('save')">
    <div class="panel-header compact-header">
      <h2>{{ editingId ? '编辑用户' : '创建用户' }}</h2>
    </div>
    <div class="form-row">
      <div class="field">
        <label>工号</label>
        <Input v-model="form.employee_id" label="工号" :disabled="!!editingId" required />
      </div>
      <div class="field">
        <label>登录名</label>
        <Input v-model="form.username" label="登录名" required />
      </div>
    </div>
    <div class="form-row">
      <div class="field">
        <label>显示名</label>
        <Input v-model="form.display_name" label="显示名" required />
      </div>
      <div class="field">
        <label>状态</label>
        <Select v-model="form.status" label="状态" :options="[{ value: 'active', label: '启用' }, { value: 'disabled', label: '禁用' }]" />
      </div>
    </div>
    <div class="field">
      <label>所属 OU</label>
      <Select
        v-model="form.organizational_unit_id"
        label="所属 OU"
        required
        :options="items.map((item) => ({ value: item.ou.id, label: `${'　'.repeat(item.depth)}${item.ou.name}` }))"
      />
    </div>
    <div class="form-row">
      <div class="field">
        <label>邮箱</label>
        <Input v-model="form.email" label="邮箱" type="email" />
      </div>
      <div class="field">
        <label>手机</label>
        <Input v-model="form.mobile" label="手机" />
      </div>
    </div>
    <div v-if="!editingId" class="field">
      <label>初始密码</label>
      <Input v-model="form.initial_password" label="初始密码" type="password" required />
    </div>
    <div v-else class="field">
      <label>重置密码</label>
      <Input v-model="form.reset_password" label="重置密码" type="password" />
    </div>
    <div class="form-actions">
      <Button type="submit" variant="solid" :disabled="loading || disabled">
        保存
      </Button>
      <Button
        v-if="editingId"
        type="button"
        class="secondary-button"
        :disabled="loading || disabled || !form.reset_password"
        @click="$emit('resetPassword')"
        variant="outline"
      >
        重置密码
      </Button>
      <Button
        v-if="editingId"
        type="button"
        class="secondary-button"
        :disabled="loading || disabled"
        @click="$emit('enable')"
        variant="outline"
      >
        启用
      </Button>
      <Button
        v-if="editingId"
        type="button"
        class="danger-button"
        :disabled="loading || disabled"
        @click="$emit('disable')"
        variant="solid"
        semantic="error"
      >
        禁用
      </Button>
      <Button type="button" variant="outline" @click="$emit('reset')">清空</Button>
    </div>
  </form>
</template>
