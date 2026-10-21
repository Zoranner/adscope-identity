<script setup lang="ts">
import { Button, Checkbox, Input, RadioGroup, Textarea, Toggle } from '@stellar/ui'
import type { OAuthClientType, OAuthScope } from '~/types/admin'
import EditorForm from '~/components/shell/EditorForm.vue'

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
  saving?: boolean
}>()

defineEmits<{
  save: []
  regenerateSecret: []
}>()

const clientTypes = [
  { value: 'web', label: 'Web' },
  { value: 'desktop', label: 'Desktop' },
]
const scopes: Array<{ value: OAuthScope; label: string }> = [
  { value: 'openid', label: 'OpenID（必选）' },
  { value: 'profile', label: '用户资料' },
  { value: 'email', label: '邮箱' },
  { value: 'phone', label: '电话' },
]
</script>

<template>
  <EditorForm form-id="oauth-client-form" @submit="$emit('save')">
    <Input v-if="editingId" :model-value="editingId" label="Client ID" readonly />

    <Input v-model="form.name" label="名称" :maxlength="100" required />

    <RadioGroup
      v-model="form.client_type"
      label="类型"
      :options="clientTypes"
      :disabled="!!editingId"
    />

    <Textarea
      v-model="form.redirect_uris"
      label="Redirect URI"
      placeholder="每行一个完整 URI"
      :rows="4"
      spellcheck="false"
      required
    />

    <div class="flex flex-wrap items-center gap-[var(--spacing-lg)]">
      <Checkbox
        v-for="scopeOption in scopes"
        :key="scopeOption.value"
        v-model="form.allowed_scopes"
        :value="scopeOption.value"
        :label="scopeOption.label"
        :disabled="scopeOption.value === 'openid' || saving"
      />
    </div>

    <Toggle v-model="form.enabled" label="启用应用" :disabled="saving" />

    <section
      v-if="editingId && form.client_type === 'web'"
      class="flex flex-wrap items-center gap-[var(--spacing-md)] border-t border-[var(--st-border-divider)] pt-[var(--spacing-md)]"
    >
      <Button
        type="button"
        icon="ph:arrows-clockwise"
        :disabled="saving"
        @click="$emit('regenerateSecret')"
      >
        重新生成密钥
      </Button>
    </section>
  </EditorForm>
</template>
