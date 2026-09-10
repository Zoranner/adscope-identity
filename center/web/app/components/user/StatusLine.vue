<script setup lang="ts">
import { X } from 'lucide-vue-next'
import { Button } from '@stellar/ui'

const { message, isError, visible, clearStatus } = useUserStatus()
</script>

<template>
  <Teleport to="body">
    <div v-if="visible" class="toast-stack">
      <div class="toast" :class="{ error: isError }" role="status">
        <span>{{ message }}</span>
        <Button variant="text" square aria-label="关闭通知" @click="clearStatus">
          <X :size="15" />
        </Button>
      </div>
    </div>
  </Teleport>
</template>
