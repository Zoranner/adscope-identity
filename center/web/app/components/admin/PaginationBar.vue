<script setup lang="ts">
import { Button } from '@stellar/ui'
defineProps<{
  page: number
  pageCount: number
  total: number
  start: number
  end: number
}>()

defineEmits<{
  previous: []
  next: []
}>()
</script>

<template>
  <div class="pagination-bar">
    <span>显示 {{ total === 0 ? 0 : start + 1 }}-{{ end }} / {{ total }}</span>
    <div class="row-actions">
      <Button variant="outline" size="small" :disabled="page <= 1" @click="$emit('previous')">
        上一页
      </Button>
      <span class="page-indicator">{{ page }} / {{ pageCount }}</span>
      <Button
        variant="outline"
        size="small"
        :disabled="page >= pageCount"
        @click="$emit('next')"
      >
        下一页
      </Button>
    </div>
  </div>
</template>
