<script setup lang="ts">
import { useEngineStatus } from "../composables/useEngine";

/** Modellar yuklanmagan bo'lsa kamera tasviri chiqadi, lekin tanish/namuna olish ishlamaydi. */
const status = useEngineStatus();
</script>

<template>
  <div v-if="status && status.state === 'loading'" class="banner muted">{{ $t("diag.engineLoading") }}</div>
  <div v-else-if="status && status.state === 'failed'" class="banner error">
    <b>{{ $t("errors.engineFailed") }}</b>
    <div class="wrap small">{{ status.error }}</div>
  </div>
</template>

<style scoped>
.banner { padding: 10px 12px; margin-bottom: 12px; border-radius: 10px; background: var(--surface); border: 1px solid var(--border); }
.banner.error { border-color: var(--danger); }
.small { font-size: 13px; margin-top: 4px; }
</style>
