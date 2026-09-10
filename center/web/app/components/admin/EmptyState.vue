<script setup lang="ts">
import { Button } from '@stellar/ui'
defineProps<{
  title: string
  actionLabel?: string
}>()

defineEmits<{
  action: []
}>()
</script>

<template>
  <div class="empty-state">
    <p>{{ title }}</p>
    <Button v-if="actionLabel" variant="solid" @click="$emit('action')">
      {{ actionLabel }}
    </Button>
  </div>
</template>
