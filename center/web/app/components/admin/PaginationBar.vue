<script setup lang="ts">
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
      <button class="secondary-button compact-button" :disabled="page <= 1" @click="$emit('previous')">
        上一页
      </button>
      <span class="page-indicator">{{ page }} / {{ pageCount }}</span>
      <button
        class="secondary-button compact-button"
        :disabled="page >= pageCount"
        @click="$emit('next')"
      >
        下一页
      </button>
    </div>
  </div>
</template>
