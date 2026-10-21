<script setup lang="ts">
onMounted(() => {
  void navigateTo('/admin/domains', { replace: true })
})
</script>

<template>
  <div role="status" class="sr-only">正在进入域管理</div>
</template>
