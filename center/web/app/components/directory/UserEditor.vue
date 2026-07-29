<script setup lang="ts">
import { KeyRound, Save } from 'lucide-vue-next'
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
        <input v-model="form.employee_id" :disabled="!!editingId" required />
      </div>
      <div class="field">
        <label>登录名</label>
        <input v-model="form.username" required />
      </div>
    </div>
    <div class="field">
      <label>显示名</label>
      <input v-model="form.display_name" required />
    </div>
    <div class="field">
      <label>所属 OU</label>
      <select v-model="form.organizational_unit_id" required>
        <option v-for="item in items" :key="item.ou.id" :value="item.ou.id">
          {{ `${'　'.repeat(item.depth)}${item.ou.name}` }}
        </option>
      </select>
    </div>
    <div class="form-row">
      <div class="field">
        <label>邮箱</label>
        <input v-model="form.email" type="email" />
      </div>
      <div class="field">
        <label>手机</label>
        <input v-model="form.mobile" />
      </div>
    </div>
    <div class="form-row">
      <div class="field">
        <label>电话</label>
        <input v-model="form.telephone" />
      </div>
      <div class="field">
        <label>状态</label>
        <select v-model="form.status">
          <option value="active">启用</option>
          <option value="disabled">禁用</option>
        </select>
      </div>
    </div>
    <div v-if="!editingId" class="field">
      <label>初始密码</label>
      <input v-model="form.initial_password" type="password" required />
    </div>
    <div v-else class="field">
      <label>重置密码</label>
      <input v-model="form.reset_password" type="password" />
    </div>
    <div class="form-actions">
      <button class="primary-button" :disabled="loading || disabled">
        <Save :size="17" />
        保存
      </button>
      <button
        v-if="editingId"
        type="button"
        class="secondary-button"
        :disabled="loading || disabled || !form.reset_password"
        @click="$emit('resetPassword')"
      >
        <KeyRound :size="16" />
        重置密码
      </button>
      <button
        v-if="editingId"
        type="button"
        class="secondary-button"
        :disabled="loading || disabled"
        @click="$emit('enable')"
      >
        启用
      </button>
      <button
        v-if="editingId"
        type="button"
        class="danger-button"
        :disabled="loading || disabled"
        @click="$emit('disable')"
      >
        禁用
      </button>
      <button type="button" class="secondary-button" @click="$emit('reset')">清空</button>
    </div>
  </form>
</template>
