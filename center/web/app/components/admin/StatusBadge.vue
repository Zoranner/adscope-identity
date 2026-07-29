<script setup lang="ts">
defineProps<{
  tone?: 'default' | 'warn' | 'danger'
}>()
</script>

<template>
  <span class="badge" :class="tone">
    <slot />
  </span>
</template>
