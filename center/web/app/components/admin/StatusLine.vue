<script setup lang="ts">
const { message, isError } = useAdminStatus()
</script>

<template>
  <p class="status-line" :class="{ error: isError }">{{ message }}</p>
</template>
