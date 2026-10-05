<script setup lang="ts">
defineProps<{ title: string }>();
</script>

<template>
  <section class="page">
    <h1 class="page-title">{{ title }}</h1>
    <p class="muted">{{ $t("placeholder") }}</p>
  </section>
</template>
