<script setup lang="ts">
onMounted(() => {
  void navigateTo('/login', { replace: true })
})
</script>

<template>
  <main class="credential-screen">
    <section class="credential-card">
      <h1>正在进入用户登录</h1>
    </section>
  </main>
</template>
