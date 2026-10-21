<script setup lang="ts">
onMounted(() => {
  void navigateTo('/login', { replace: true })
})
</script>

<template>
  <div role="status" class="sr-only">正在进入用户登录</div>
</template>
