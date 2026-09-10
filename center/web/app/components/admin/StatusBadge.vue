<script setup lang="ts">
import { Badge } from '@stellar/ui'

defineProps<{
  tone?: 'default' | 'warn' | 'danger'
}>()
</script>

<template>
  <Badge
    :semantic="tone === 'danger' ? 'error' : tone === 'warn' ? 'warning' : undefined"
    :variant="tone === 'default' ? 'soft' : 'solid'"
  >
    <slot />
  </Badge>
</template>
