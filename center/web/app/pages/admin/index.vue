<script setup lang="ts">
onMounted(() => {
  void navigateTo('/admin/directory', { replace: true })
})
</script>

<template>
  <div role="status" class="sr-only">正在进入目录</div>
</template>
