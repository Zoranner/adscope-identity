<script setup lang="ts">
defineProps<{
  formId: string
}>()

defineEmits<{
  submit: []
}>()
</script>

<template>
  <form :id="formId" class="grid gap-[var(--spacing-lg)]" @submit.prevent="$emit('submit')">
    <slot />
  </form>
</template>
