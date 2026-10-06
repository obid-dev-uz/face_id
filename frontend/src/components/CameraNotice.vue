<script setup lang="ts">
import type { CameraStatus } from "../composables/useCamera";

/** Kamera ishlamayotgan holatlar (ruxsat yo'q, kamera yo'q, xato). Boshqa holatda hech narsa chiqmaydi. */
defineProps<{ status: CameraStatus; error: string | null }>();
const emit = defineEmits<{ (e: "retry"): void; (e: "settings"): void }>();
</script>

<template>
  <div v-if="status === 'denied'" class="notice">
    <h2>{{ $t("camera.deniedTitle") }}</h2>
    <p class="muted">{{ $t("camera.deniedText") }}</p>
    <button type="button" class="btn" @click="emit('retry')">{{ $t("camera.retry") }}</button>
    <button type="button" class="btn spaced" @click="emit('settings')">{{ $t("camera.openSettings") }}</button>
  </div>
  <div v-else-if="status === 'unsupported'" class="notice">
    <p class="muted">{{ $t("camera.unsupported") }}</p>
  </div>
  <div v-else-if="status === 'noCamera'" class="notice">
    <p class="muted">{{ $t("camera.noCamera") }}</p>
  </div>
  <div v-else-if="status === 'error'" class="notice">
    <p class="error wrap">{{ error }}</p>
    <button type="button" class="btn" @click="emit('retry')">{{ $t("camera.retry") }}</button>
  </div>
</template>

<style scoped>
.notice { padding: 24px 0; }
.notice h2 { font-size: 18px; margin: 0 0 8px; }
.spaced { margin-top: 8px; }
</style>
