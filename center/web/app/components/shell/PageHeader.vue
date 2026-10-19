<script setup lang="ts">
defineProps<{
  title?: string
}>()
</script>

<template>
  <header
    class="flex flex-none items-center justify-between gap-[var(--spacing-md)] border-b border-[var(--st-border-divider)] pb-[var(--spacing-sm)]"
  >
    <h1 v-if="title" class="m-0 text-[var(--text-xl)] font-semibold leading-7">{{ title }}</h1>
    <div class="flex items-center gap-[var(--spacing-sm)]">
      <slot name="actions" />
    </div>
  </header>
</template>
