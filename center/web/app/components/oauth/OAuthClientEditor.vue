<script setup lang="ts">
import { KeyRound, Save } from 'lucide-vue-next'
import type { OAuthClientType, OAuthScope } from '~/types/admin'

interface OAuthClientEditorModel {
  name: string
  client_type: OAuthClientType
  redirect_uris: string
  allowed_scopes: OAuthScope[]
  enabled: boolean
}

const form = defineModel<OAuthClientEditorModel>({ required: true })

defineProps<{
  editingId: string | null
  loading?: boolean
  disabled?: boolean
}>()

defineEmits<{
  save: []
  regenerateSecret: []
}>()

const clientTypes: Array<{ value: OAuthClientType; label: string }> = [
  { value: 'web', label: 'Web' },
  { value: 'desktop', label: 'Desktop' },
]
const scopes: Array<{ value: OAuthScope; label: string }> = [
  { value: 'openid', label: 'OpenID' },
  { value: 'profile', label: '用户资料' },
  { value: 'email', label: '邮箱' },
  { value: 'phone', label: '电话' },
]
</script>

<template>
  <form class="form" @submit.prevent="$emit('save')">
    <div v-if="editingId" class="field">
      <label for="oauth-client-id">Client ID</label>
      <input id="oauth-client-id" :value="editingId" readonly spellcheck="false" />
    </div>

    <div class="field">
      <label for="oauth-client-name">名称</label>
      <input
        id="oauth-client-name"
        v-model="form.name"
        maxlength="100"
        :disabled="loading || disabled"
        required
      />
    </div>

    <div class="field">
      <label>类型</label>
      <div class="row-actions segmented-control" role="radiogroup" aria-label="客户端类型">
        <button
          v-for="clientType in clientTypes"
          :key="clientType.value"
          type="button"
          class="secondary-button segmented-option"
          :class="{ active: form.client_type === clientType.value }"
          role="radio"
          :aria-checked="form.client_type === clientType.value"
          :disabled="!!editingId || loading || disabled"
          @click="form.client_type = clientType.value"
        >
          {{ clientType.label }}
        </button>
      </div>
    </div>

    <div class="field">
      <label for="oauth-client-redirect-uris">Redirect URI</label>
      <textarea
        id="oauth-client-redirect-uris"
        v-model="form.redirect_uris"
        :disabled="loading || disabled"
        placeholder="每行一个完整 URI"
        spellcheck="false"
        required
      ></textarea>
    </div>

    <div class="field">
      <label>授权范围</label>
      <div class="row-actions checkbox-group">
        <label v-for="scope in scopes" :key="scope.value" class="checkbox-option">
          <input
            v-model="form.allowed_scopes"
            type="checkbox"
            :value="scope.value"
            :disabled="scope.value === 'openid' || loading || disabled"
          />
          {{ scope.label }}
        </label>
      </div>
    </div>

    <div class="field">
      <label class="checkbox-option">
        <input
          v-model="form.enabled"
          type="checkbox"
          :disabled="loading || disabled"
        />
        启用客户端
      </label>
    </div>

    <div class="form-actions">
      <button class="primary-button" :disabled="loading || disabled">
        <Save :size="17" />
        保存
      </button>
      <button
        v-if="editingId && form.client_type === 'web'"
        type="button"
        class="secondary-button"
        :disabled="loading || disabled"
        @click="$emit('regenerateSecret')"
      >
        <KeyRound :size="16" />
        重新生成密钥
      </button>
    </div>
  </form>
</template>
