<script setup lang="ts">
import { X } from 'lucide-vue-next'

const { message, isError, visible, clearStatus } = useAdminStatus()
</script>

<template>
  <Teleport to="body">
    <div v-if="visible" class="toast-stack">
      <div class="toast" :class="{ error: isError }" role="status">
        <span>{{ message }}</span>
        <button class="toast-close" title="关闭通知" @click="clearStatus">
          <X :size="15" />
        </button>
      </div>
    </div>
  </Teleport>
</template>
