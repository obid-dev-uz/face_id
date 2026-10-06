<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useAppStore } from "../stores/app";
import { LOCALES, setLocale, type Locale } from "../i18n";
import { adminApi, api, type AdminEvent } from "../api";
import { isTauri } from "../platform";
import EngineDiagnostics from "../components/EngineDiagnostics.vue";
import { toast } from "../composables/useToast";
import PinDialog from "../components/PinDialog.vue";
import { adminState } from "../composables/useAdmin";

const { locale, t } = useI18n();
const app = useAppStore();
const names: Record<Locale, string> = { uz: "O'zbekcha", ru: "Русский", en: "English" };
const webview = (navigator.userAgent.match(/Chrome\/([\d.]+)/) || [])[1] || navigator.userAgent;

onMounted(app.loadInfo);

/* Rust bilan bir xil standartlar: commands/engine.rs DEFAULT_THRESHOLD, recognize.rs log.cooldown_s */
const threshold = ref(0.363);
const cooldown = ref(60);
const COOLDOWNS = [10, 30, 60, 300, 900];
/* Rust: commands/logs.rs DEFAULT_RETENTION_DAYS; 0 — cheksiz */
const RETENTIONS = [7, 30, 90, 365, 0];
const retention = ref(30);
const secureScreen = ref(false);
const wipeBusy = ref(false);

onMounted(async () => {
  if (!isTauri) return;
  const s = await api.getSettings().catch(() => ({} as Record<string, string>));
  if (s["match.threshold"]) threshold.value = parseFloat(s["match.threshold"]);
  if (s["log.cooldown_s"]) cooldown.value = parseInt(s["log.cooldown_s"], 10);
  if (s["log.retention_days"]) retention.value = parseInt(s["log.retention_days"], 10);
  secureScreen.value = s["privacy.secure_screen"] === "1";
});

function saveThreshold() {
  api.setSetting("match.threshold", threshold.value.toFixed(3)).catch(() => {});
}

function setCooldown(n: number) {
  cooldown.value = n;
  api.setSetting("log.cooldown_s", String(n)).catch(() => {});
}

async function setRetention(days: number) {
  retention.value = days;
  await api.setSetting("log.retention_days", String(days)).catch(() => {});
  // Yangi muddat darhol qo'llanadi (keyingi safar ilova ochilganda ham).
  if (days > 0) await api.purgeLogs(days).catch(() => {});
}

async function toggleSecure() {
  const next = !secureScreen.value;
  try {
    await api.setSecureScreen(next);
    secureScreen.value = next;
  } catch (e) {
    toast(String(e));
  }
}

async function wipeAll() {
  // Ikki marta so'raladi: qaytarib bo'lmaydi (zaxira nusxadan tashqari).
  if (!window.confirm(t("privacy.wipeConfirm1")) || !window.confirm(t("privacy.wipeConfirm2"))) return;
  wipeBusy.value = true;
  try {
    await api.wipeAllData();
    toast(t("privacy.wiped"));
  } catch (e) {
    toast(String(e));
  } finally {
    wipeBusy.value = false;
  }
}

/* Rust: admin.rs SESSION_MIN_RANGE, DEFAULT_SESSION_MIN */
const SESSIONS = [1, 3, 5, 10];
const sessionMin = ref(Math.round(adminState.timeoutMs / 60000));
const changingPin = ref(false);

async function setSession(n: number) {
  try {
    await api.setSetting("security.session_min", String(n));
    sessionMin.value = n;
  } catch (e) {
    toast(String(e));
  }
}

function onPinChanged() {
  changingPin.value = false;
  toast(t("admin.pinChanged"));
}

const events = ref<AdminEvent[] | null>(null);
async function toggleJournal() {
  if (events.value) {
    events.value = null;
    return;
  }
  try {
    events.value = await adminApi.events(100);
  } catch (e) {
    toast(String(e));
  }
}
/** SQLite `datetime('now')` — UTC. */
function when(s: string) {
  const d = new Date(s.replace(" ", "T") + "Z");
  return isNaN(d.getTime()) ? s : d.toLocaleString();
}
const eventName = (k: string) => {
  const key = "admin.event." + k;
  const s = t(key);
  return s === key ? k : s;
};

const fmtDays = (n: number) => (n === 0 ? t("privacy.forever") : t("privacy.days", { n }));

const fmtSecs = (n: number) => (n < 60 ? t("settings.seconds", { n }) : t("settings.minutes", { n: n / 60 }));

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

    <h2 class="section-title">{{ $t("settings.recognition") }}</h2>
    <label class="slider">
      <span>{{ $t("settings.threshold") }}: <b>{{ threshold.toFixed(2) }}</b></span>
      <input v-model.number="threshold" type="range" min="0.25" max="0.6" step="0.01" :disabled="!isTauri" @change="saveThreshold" />
    </label>
    <p class="muted small">{{ $t("settings.thresholdHint") }}</p>
    <p class="small">{{ $t("settings.cooldown") }}</p>
    <div class="segmented">
      <button v-for="n in COOLDOWNS" :key="n" type="button" :class="{ on: cooldown === n }" :disabled="!isTauri" @click="setCooldown(n)">
        {{ fmtSecs(n) }}
      </button>
    </div>

    <h2 class="section-title">{{ $t("admin.section") }}</h2>
    <button type="button" class="btn" :disabled="!isTauri" @click="changingPin = true">{{ $t("admin.changePin") }}</button>
    <p class="small">{{ $t("admin.sessionLength") }}</p>
    <div class="segmented">
      <button v-for="n in SESSIONS" :key="n" type="button" :class="{ on: sessionMin === n }" :disabled="!isTauri" @click="setSession(n)">
        {{ $t("settings.minutes", { n }) }}
      </button>
    </div>
    <button type="button" class="btn journal-btn" :disabled="!isTauri" @click="toggleJournal">
      {{ events ? $t("admin.hideJournal") : $t("admin.showJournal") }}
    </button>
    <ul v-if="events" class="journal">
      <li v-if="!events.length" class="muted">{{ $t("admin.journalEmpty") }}</li>
      <li v-for="e in events" :key="e.id" :class="{ bad: e.kind === 'fail' || e.kind === 'lockout' }">
        <span>{{ eventName(e.kind) }}<template v-if="e.detail"> · {{ e.detail }}</template></span>
        <time class="muted">{{ when(e.createdAt) }}</time>
      </li>
    </ul>
    <PinDialog v-if="changingPin" mode="change" @close="changingPin = false" @done="onPinChanged" />

    <h2 class="section-title">{{ $t("privacy.title") }}</h2>
    <p class="small">{{ $t("privacy.retention") }}</p>
    <div class="segmented">
      <button v-for="n in RETENTIONS" :key="n" type="button" :class="{ on: retention === n }" :disabled="!isTauri" @click="setRetention(n)">
        {{ fmtDays(n) }}
      </button>
    </div>
    <label class="check">
      <input type="checkbox" :checked="secureScreen" :disabled="!isTauri" @change="toggleSecure" />
      <span>{{ $t("privacy.secureScreen") }}<br /><span class="muted small">{{ $t("privacy.secureScreenHint") }}</span></span>
    </label>
    <button type="button" class="btn danger" :disabled="!isTauri || wipeBusy" @click="wipeAll">{{ $t("privacy.wipe") }}</button>
    <p class="muted small">{{ $t("privacy.wipeHint") }}</p>

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
      <dt>{{ $t("info.ram") }}</dt>
      <dd>{{ app.info.totalRamMb !== null ? app.info.totalRamMb + " MB" : "—" }}</dd>
    </dl>
    <p v-else class="muted">{{ $t("settings.loading") }}</p>
  </section>
</template>

<style scoped>
.slider { display: block; }
.slider input { display: block; width: 100%; min-height: 44px; }
.small { font-size: 13px; margin: 4px 0 8px; }
.check { display: flex; align-items: flex-start; margin: 16px 0; }
.check input { width: 22px; height: 22px; margin: 2px 10px 0 0; flex: 0 0 auto; }
.danger { color: var(--danger); }
.journal-btn { margin-top: 12px; }
.journal { list-style: none; margin: 8px 0 0; padding: 4px 14px; background: var(--surface); border: 1px solid var(--border); border-radius: 10px; font-size: 14px; }
.journal li { padding: 8px 0; border-bottom: 1px solid var(--border); }
.journal li:last-child { border-bottom: 0; }
.journal li.bad span { color: var(--danger); }
.journal time { display: block; font-size: 12px; }
</style>
