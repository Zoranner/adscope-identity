<script setup lang="ts">
import { X } from 'lucide-vue-next'

defineProps<{
  open: boolean
  title: string
  width?: 'medium' | 'wide'
}>()

defineEmits<{
  close: []
}>()
</script>

<template>
  <Teleport to="body">
    <div v-if="open" class="modal-backdrop" @click.self="$emit('close')">
      <section class="modal-panel" :class="width ?? 'medium'" role="dialog" aria-modal="true">
        <header class="modal-header">
          <h2>{{ title }}</h2>
          <button class="icon-button" title="关闭" @click="$emit('close')">
            <X :size="18" />
          </button>
        </header>
        <div class="modal-body">
          <slot />
        </div>
      </section>
    </div>
  </Teleport>
</template>
