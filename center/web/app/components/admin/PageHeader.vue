<script setup lang="ts">
import { RefreshCw } from 'lucide-vue-next'

defineProps<{
  title: string
  description: string
  loading?: boolean
  disabled?: boolean
}>()

defineEmits<{
  refresh: []
}>()
</script>

<template>
  <section class="view-header">
    <div>
      <h1>{{ title }}</h1>
      <p>{{ description }}</p>
    </div>
    <button class="primary-button" :disabled="loading || disabled" @click="$emit('refresh')">
      <RefreshCw :size="17" />
      刷新
    </button>
  </section>
</template>
