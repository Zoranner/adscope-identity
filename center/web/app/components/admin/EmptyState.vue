<script setup lang="ts">
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
    <button v-if="actionLabel" class="primary-button" @click="$emit('action')">
      {{ actionLabel }}
    </button>
  </div>
</template>
