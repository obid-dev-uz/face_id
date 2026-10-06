<script setup lang="ts">
// PIN oynasi: kirish, birinchi o'rnatish, o'zgartirish va (unutilganda) tiklash.
// Tizim klaviaturasi emas, o'zimizning raqamlar: klaviatura PIN'ni eslab qolmaydi.
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { adminApi, type PinVerdict } from "../api";
import { adminState, afterUnlock, fmtClock, isWeakPin, PIN_LEN } from "../composables/useAdmin";
import { useBackHandler } from "../composables/useBack";

type Mode = "unlock" | "setup" | "change" | "reset";
type Step = "old" | "new" | "confirm" | "pin";

const props = defineProps<{ mode: Mode }>();
const emit = defineEmits<{ (e: "close"): void; (e: "done"): void }>();
const { t } = useI18n();

const mode = ref<Mode>(props.mode);
const step = ref<Step>(firstStep(props.mode));
const digits = ref("");
const firstPin = ref("");
const oldPin = ref("");
const message = ref<string | null>(null);
const busy = ref(false);
const shake = ref(false);
const resetText = ref("");
const now = ref(Date.now());
const timer = setInterval(() => (now.value = Date.now()), 500);
onBeforeUnmount(() => clearInterval(timer));

function firstStep(m: Mode): Step {
  return m === "unlock" ? "pin" : m === "change" ? "old" : "new";
}

const lockedMs = computed(() => Math.max(0, adminState.lockedUntil - now.value));
const locked = computed(() => lockedMs.value > 0 && step.value !== "new" && step.value !== "confirm");

const title = computed(() => {
  if (mode.value === "reset") return t("admin.resetTitle");
  if (step.value === "old") return t("admin.oldPin");
  if (step.value === "confirm") return t("admin.confirmPin");
  if (step.value === "new") return mode.value === "setup" ? t("admin.setupTitle") : t("admin.newPin");
  return t("admin.enterPin");
});

useBackHandler(() => {
  close();
  return true;
});
onMounted(() => document.addEventListener("keydown", onKey));
onBeforeUnmount(() => document.removeEventListener("keydown", onKey));

/** Desktop/emulyator klaviaturasi uchun. */
function onKey(e: KeyboardEvent) {
  if (mode.value === "reset") return;
  if (e.key >= "0" && e.key <= "9") press(e.key);
  else if (e.key === "Backspace") back();
  else if (e.key === "Escape") close();
}

function close() {
  if (!busy.value) emit("close");
}

function vibrate(ms: number | number[]) {
  try {
    if (navigator.vibrate) navigator.vibrate(ms);
  } catch (e) {
    /* qo'llab-quvvatlanmaydi */
  }
}

function fail(text: string) {
  message.value = text;
  digits.value = "";
  shake.value = true;
  vibrate([60, 40, 60]);
  setTimeout(() => (shake.value = false), 450);
}

function press(d: string) {
  if (busy.value || locked.value || digits.value.length >= PIN_LEN) return;
  digits.value += d;
  if (digits.value.length === PIN_LEN) submit(digits.value);
}

function back() {
  if (!busy.value) digits.value = digits.value.slice(0, -1);
}

function verdictText(v: PinVerdict): string | null {
  now.value = Date.now();
  if (v.result === "wrong") {
    if (v.lockedMs > 0) adminState.lockedUntil = Date.now() + v.lockedMs;
    return v.attemptsLeft > 0 && v.attemptsLeft <= 3 ? t("admin.attemptsLeft", { n: v.attemptsLeft }) : t("admin.wrong");
  }
  if (v.result === "locked") adminState.lockedUntil = Date.now() + v.lockedMs;
  return null;
}

async function submit(pin: string) {
  message.value = null;
  if (step.value === "old") {
    oldPin.value = pin;
    digits.value = "";
    step.value = "new";
    return;
  }
  if (step.value === "new") {
    if (isWeakPin(pin)) return fail(t("admin.weak"));
    if (mode.value === "change" && pin === oldPin.value) return fail(t("admin.samePin"));
    firstPin.value = pin;
    digits.value = "";
    step.value = "confirm";
    return;
  }
  if (step.value === "confirm" && pin !== firstPin.value) {
    step.value = "new";
    return fail(t("admin.mismatch"));
  }

  busy.value = true;
  try {
    if (mode.value === "setup") {
      await adminApi.setup(pin);
    } else {
      const v = mode.value === "change" ? await adminApi.changePin(oldPin.value, pin) : await adminApi.unlock(pin);
      if (v.result !== "ok") {
        busy.value = false;
        digits.value = "";
        if (v.result === "notConfigured") {
          adminState.configured = false;
          mode.value = "setup";
          step.value = "new";
          return;
        }
        if (mode.value === "change") step.value = "old";
        const text = verdictText(v);
        if (text) fail(text);
        else vibrate([60, 40, 60]);
        return;
      }
    }
    vibrate(30);
    await afterUnlock();
    emit("done");
  } catch (e) {
    busy.value = false;
    fail(String(e));
  }
}

const resetWord = computed(() => t("admin.resetWord"));

async function doReset() {
  if (resetText.value.trim().toUpperCase() !== resetWord.value.toUpperCase()) return;
  busy.value = true;
  try {
    await adminApi.reset();
    await afterUnlock();
    busy.value = false;
    mode.value = "setup";
    step.value = "new";
    digits.value = "";
    message.value = t("admin.resetDone");
  } catch (e) {
    busy.value = false;
    message.value = String(e);
  }
}

const KEYS = ["1", "2", "3", "4", "5", "6", "7", "8", "9"];
</script>

<template>
  <div class="pin-overlay" role="dialog" aria-modal="true" @click.self="close">
    <div class="pin-card">
      <button type="button" class="pin-x" :aria-label="$t('admin.cancel')" :disabled="busy" @click="close">✕</button>
      <svg class="pin-lock" viewBox="0 0 24 24" aria-hidden="true"><path d="M6 11h12v10H6zM8 11V8a4 4 0 0 1 8 0v3" /></svg>
      <h2 class="pin-title">{{ title }}</h2>

      <template v-if="mode === 'reset'">
        <p class="pin-text">{{ $t("admin.resetText") }}</p>
        <label class="pin-text">
          {{ $t("admin.resetPrompt", { word: resetWord }) }}
          <input v-model="resetText" class="pin-input" type="text" autocomplete="off" autocapitalize="characters" />
        </label>
        <p v-if="message" class="pin-msg">{{ message }}</p>
        <button
          type="button"
          class="btn pin-danger"
          :disabled="busy || resetText.trim().toUpperCase() !== resetWord.toUpperCase()"
          @click="doReset"
        >
          {{ $t("admin.resetDo") }}
        </button>
        <button type="button" class="pin-link" :disabled="busy" @click="mode = 'unlock'; step = 'pin'; message = null">
          {{ $t("admin.cancel") }}
        </button>
      </template>

      <template v-else>
        <p v-if="mode === 'setup' && step === 'new'" class="pin-text">{{ $t("admin.setupHint") }}</p>
        <div class="pin-dots" :class="{ shake, busy }" aria-hidden="true">
          <i v-for="i in PIN_LEN" :key="i" :class="{ on: i <= digits.length }" />
        </div>
        <p class="pin-msg" role="status" aria-live="polite">
          <template v-if="locked">{{ $t("admin.locked", { t: fmtClock(lockedMs) }) }}</template>
          <template v-else-if="busy">{{ $t("admin.checking") }}</template>
          <template v-else-if="message">{{ message }}</template>
        </p>
        <div class="pin-pad" :class="{ disabled: locked || busy }">
          <button v-for="k in KEYS" :key="k" type="button" class="pin-key" @click="press(k)">{{ k }}</button>
          <span class="pin-key blank" />
          <button type="button" class="pin-key" @click="press('0')">0</button>
          <button type="button" class="pin-key" :aria-label="$t('admin.erase')" @click="back">
            <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M21 5H9l-6 7 6 7h12zM12 9l6 6M18 9l-6 6" /></svg>
          </button>
        </div>
        <button v-if="mode === 'unlock'" type="button" class="pin-link" @click="mode = 'reset'; message = null">
          {{ $t("admin.forgot") }}
        </button>
      </template>
    </div>
  </div>
</template>

<style scoped>
/* Android 7 WebView: grid va flex gap yo'q — tugmalar flex-wrap bilan */
.pin-overlay {
  position: fixed; left: 0; top: 0; right: 0; bottom: 0; z-index: 70;
  display: -webkit-box; display: flex; -webkit-box-align: center; align-items: center; -webkit-box-pack: center; justify-content: center;
  background: rgba(0, 0, 0, .6); padding: 16px;
}
.pin-card {
  position: relative; width: 100%; max-width: 340px; max-height: 100%; overflow: auto;
  padding: 20px 20px 12px; border-radius: 18px; background: var(--surface); color: var(--text); text-align: center;
}
.pin-x { position: absolute; right: 6px; top: 6px; width: 44px; height: 44px; border: 0; background: transparent; font-size: 18px; color: var(--muted); cursor: pointer; }
.pin-lock { width: 32px; height: 32px; fill: none; stroke: var(--accent); stroke-width: 1.8; stroke-linecap: round; stroke-linejoin: round; }
.pin-title { font-size: 18px; margin: 6px 0 4px; }
.pin-text { display: block; font-size: 14px; color: var(--muted); margin: 8px 0; text-align: left; }
.pin-input { display: block; width: 100%; margin-top: 6px; min-height: 44px; padding: 8px 12px; border: 1px solid var(--border); border-radius: 10px; background: var(--bg); color: var(--text); font: inherit; letter-spacing: .1em; }
.pin-msg { min-height: 20px; margin: 4px 0 8px; font-size: 14px; color: var(--danger); }

.pin-dots { display: -webkit-box; display: flex; -webkit-box-pack: center; justify-content: center; margin: 14px 0 6px; }
.pin-dots i { width: 14px; height: 14px; margin: 0 7px; border-radius: 50%; border: 2px solid var(--muted); -webkit-transition: background .1s; transition: background .1s; }
.pin-dots i.on { background: var(--accent); border-color: var(--accent); }
.pin-dots.busy i { -webkit-animation: pulse .9s ease-in-out infinite; animation: pulse .9s ease-in-out infinite; }
.pin-dots.shake { -webkit-animation: shake .4s; animation: shake .4s; }
@-webkit-keyframes shake { 20%, 60% { -webkit-transform: translateX(-10px); } 40%, 80% { -webkit-transform: translateX(10px); } }
@keyframes shake { 20%, 60% { transform: translateX(-10px); } 40%, 80% { transform: translateX(10px); } }
@-webkit-keyframes pulse { 50% { opacity: .35; } }
@keyframes pulse { 50% { opacity: .35; } }

.pin-pad { display: -webkit-box; display: flex; flex-wrap: wrap; margin: 0 -4px; }
.pin-pad.disabled { opacity: .4; pointer-events: none; }
.pin-key {
  width: 33.333%; height: 60px; border: 0; border-left: 4px solid transparent; border-right: 4px solid transparent; border-bottom: 8px solid transparent;
  background-clip: padding-box; border-radius: 14px; background-color: var(--bg); font-size: 24px; cursor: pointer;
}
.pin-key:active { background-color: var(--border); }
.pin-key.blank { background-color: transparent; }
.pin-key svg { width: 26px; height: 26px; vertical-align: middle; fill: none; stroke: currentColor; stroke-width: 1.8; stroke-linecap: round; stroke-linejoin: round; }
.pin-link { display: inline-block; min-height: 44px; margin-top: 4px; padding: 8px 12px; border: 0; background: transparent; color: var(--accent); font-size: 14px; cursor: pointer; }
.pin-danger { color: var(--danger); margin-top: 8px; }
</style>
