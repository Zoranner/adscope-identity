<script setup lang="ts">
onMounted(() => {
  void navigateTo('/admin/sync', { replace: true })
})
</script>

<template>
  <main class="credential-screen">
    <section class="credential-card">
      <h1>正在进入同步状态</h1>
    </section>
  </main>
</template>
