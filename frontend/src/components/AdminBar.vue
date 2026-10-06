<script setup lang="ts">
// O'ng yuqori burchak: mehmon rejimida "Kirish", admin rejimida qolgan vaqt va "Chiqish".
import { adminState, fmtClock, logout, openPin, remainingMs, sessionWarn } from "../composables/useAdmin";
import { isTauri } from "../platform";
</script>

<template>
  <div v-if="isTauri" class="admin-bar">
    <button v-if="!adminState.active" type="button" class="ab-btn" @click="openPin">
      <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 11h12v10H6zM8 11V8a4 4 0 0 1 8 0v3" /></svg>
      {{ $t("admin.signIn") }}
    </button>
    <template v-else>
      <span class="ab-time" :class="{ warn: sessionWarn }" :title="$t('admin.sessionLeft')">{{ fmtClock(remainingMs) }}</span>
      <button type="button" class="ab-btn" @click="logout">
        <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M15 4h4v16h-4M10 16l4-4-4-4M14 12H3" /></svg>
        {{ $t("admin.signOut") }}
      </button>
    </template>
  </div>
</template>

<style scoped>
/* Kamera tasviri ustida ham, oddiy sahifada ham o'qiladi: yarim shaffof qora fon */
.admin-bar { display: -webkit-box; display: flex; -webkit-box-align: center; align-items: center; }
.ab-btn, .ab-time {
  display: -webkit-box; display: flex; -webkit-box-align: center; align-items: center;
  height: 40px; padding: 0 14px; border: 0; border-radius: 20px;
  background: rgba(0, 0, 0, .45); color: #fff; font-size: 14px; white-space: nowrap;
}
.ab-btn { cursor: pointer; opacity: .85; }
.ab-btn svg { width: 18px; height: 18px; margin-right: 6px; fill: none; stroke: #fff; stroke-width: 2; stroke-linecap: round; stroke-linejoin: round; }
.ab-time { margin-right: 6px; padding: 0 12px; font-variant-numeric: tabular-nums; }
.ab-time.warn { background: #dc6803; }
</style>
