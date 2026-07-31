<script setup lang="ts">
onMounted(() => {
  void navigateTo('/admin/domains', { replace: true })
})
</script>

<template>
  <main class="credential-screen">
    <section class="credential-card">
      <h1>正在进入域管理</h1>
    </section>
  </main>
</template>
