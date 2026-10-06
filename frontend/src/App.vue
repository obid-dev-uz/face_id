<script setup lang="ts">
import { onMounted } from "vue";
import { useRoute, useRouter } from "vue-router";
import { useI18n } from "vue-i18n";
import { installBackButton } from "./composables/useBack";
import WebViewNotice from "./components/WebViewNotice.vue";
import AdminBar from "./components/AdminBar.vue";
import PinDialog from "./components/PinDialog.vue";
import { adminApi } from "./api";
import { isTauri } from "./platform";
import { toast, toastText } from "./composables/useToast";
import { adminState, initAdmin, onSessionEnd, remainingMs, sessionWarn, touch } from "./composables/useAdmin";

const router = useRouter();
const route = useRoute();
const { t } = useI18n();
onMounted(() => {
  installBackButton(router, () => t("app.pressAgainToExit"));
  // FLAG_SECURE oynaga qo'yiladi: har safar ilova ochilganda qayta qo'llanadi (admin rejimida doim).
  if (isTauri) adminApi.syncSecureScreen().catch(() => {});
  initAdmin();
});

// Sessiya yopilganda: hamma narsa yopiladi, faqat tanish ekrani qoladi.
onSessionEnd((reason) => {
  adminState.pinOpen = false;
  router.replace("/live");
  if (reason !== "reset") toast(t(reason === "logout" ? "admin.loggedOut" : "admin.expired"), 2500);
});

function onPinDone() {
  adminState.pinOpen = false;
  toast(t("admin.welcome"));
}

const tabs = [
  { to: "/live", key: "live", icon: "M4 7h3l2-3h6l2 3h3v13H4zM12 17a4 4 0 1 0 0-8 4 4 0 0 0 0 8z" },
  { to: "/persons", key: "persons", icon: "M9 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8zM2 21v-1a7 7 0 0 1 14 0v1M17 11a3 3 0 1 0 0-6M22 21v-1a5 5 0 0 0-4-4.9" },
  { to: "/enroll", key: "enroll", icon: "M12 5v14M5 12h14" },
  { to: "/logs", key: "logs", icon: "M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01" },
  { to: "/settings", key: "settings", icon: "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z" },
];
</script>

<template>
  <!-- Mehmon rejimida pastki menyu yo'q: faqat tanish ekrani -->
  <div class="shell" :class="{ guest: !adminState.active }">
    <main class="content">
      <!-- Kamera sahifasida tugma kamera paneli ichida (flip yonida) -->
      <AdminBar v-if="adminState.active && route.name !== 'live'" class="admin-float" />
      <!-- fullPath: /enroll va /enroll?person=… bir xil komponent, lekin holati alohida bo'lishi kerak -->
      <WebViewNotice />
      <router-view :key="$route.fullPath" />
    </main>
    <nav v-if="adminState.active" class="tabbar">
      <router-link v-for="t in tabs" :key="t.key" :to="t.to" class="tab">
        <svg viewBox="0 0 24 24" class="tab-icon" aria-hidden="true"><path :d="t.icon" /></svg>
        <span class="tab-label">{{ $t("nav." + t.key) }}</span>
      </router-link>
    </nav>
    <div v-if="sessionWarn" class="session-warn" role="alert">
      <span>{{ $t("admin.warn", { s: Math.ceil(remainingMs / 1000) }) }}</span>
      <button type="button" @click="touch(true)">{{ $t("admin.stay") }}</button>
    </div>
    <PinDialog
      v-if="adminState.pinOpen"
      :mode="adminState.configured ? 'unlock' : 'setup'"
      @close="adminState.pinOpen = false"
      @done="onPinDone"
    />
    <div v-if="toastText" class="toast" role="status">{{ toastText }}</div>
  </div>
</template>
