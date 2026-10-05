<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useAppStore } from "../stores/app";
import { LOCALES, setLocale, type Locale } from "../i18n";
import { api } from "../api";
import { isTauri } from "../platform";
import EngineDiagnostics from "../components/EngineDiagnostics.vue";

const { locale } = useI18n();
const app = useAppStore();
const names: Record<Locale, string> = { uz: "O'zbekcha", ru: "Русский", en: "English" };
const webview = (navigator.userAgent.match(/Chrome\/([\d.]+)/) || [])[1] || navigator.userAgent;

onMounted(app.loadInfo);

const backupBusy = ref(false);
const backupResult = ref<{ ok: boolean; text: string } | null>(null);

async function makeBackup() {
  backupBusy.value = true;
  try {
    backupResult.value = { ok: true, text: await api.backupDatabase() };
  } catch (e) {
    backupResult.value = { ok: false, text: String(e) };
  } finally {
    backupBusy.value = false;
  }
}
</script>

<template>
  <section class="page">
    <h1 class="page-title">{{ $t("nav.settings") }}</h1>

    <h2 class="section-title">{{ $t("settings.language") }}</h2>
    <div class="segmented">
      <button
        v-for="l in LOCALES"
        :key="l"
        type="button"
        :class="{ on: locale === l }"
        @click="setLocale(l)"
      >
        {{ names[l] }}
      </button>
    </div>

    <h2 class="section-title">{{ $t("settings.data") }}</h2>
    <button type="button" class="btn" :disabled="!isTauri || backupBusy" @click="makeBackup">
      {{ backupBusy ? $t("settings.loading") : $t("settings.backup") }}
    </button>
    <p v-if="backupResult" :class="backupResult.ok ? 'muted wrap' : 'error'">
      <template v-if="backupResult.ok">{{ $t("settings.backupDone") }}: </template>{{ backupResult.text }}
    </p>

    <h2 class="section-title">{{ $t("diag.title") }}</h2>
    <EngineDiagnostics />

    <h2 class="section-title">{{ $t("settings.about") }}</h2>
    <p v-if="app.error" class="error">{{ app.error }}</p>
    <dl v-else-if="app.info" class="kv">
      <dt>{{ $t("info.version") }}</dt><dd>{{ app.info.version }}</dd>
      <dt>{{ $t("info.os") }}</dt><dd>{{ app.info.os }}</dd>
      <dt>{{ $t("info.arch") }}</dt><dd>{{ app.info.arch }}</dd>
      <dt>{{ $t("info.sqlite") }}</dt><dd>{{ app.info.sqliteVersion }}</dd>
      <dt>{{ $t("info.schema") }}</dt><dd>v{{ app.info.schemaVersion }}</dd>
      <dt>{{ $t("info.dataDir") }}</dt><dd class="wrap">{{ app.info.dataDir }}</dd>
      <dt>{{ $t("info.webview") }}</dt><dd class="wrap">{{ webview }}</dd>
    </dl>
    <p v-else class="muted">{{ $t("settings.loading") }}</p>
  </section>
</template>
