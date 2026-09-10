<script setup lang="ts">
import { RefreshCw } from 'lucide-vue-next'
import { Button } from '@stellar/ui'

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
    <Button variant="solid" :disabled="loading || disabled" @click="$emit('refresh')">
      刷新
    </Button>
  </section>
</template>
