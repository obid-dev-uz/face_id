<script setup lang="ts">
import { onMounted, reactive, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import { useI18n } from "vue-i18n";
import FaceCapture from "../components/FaceCapture.vue";
import { api, enrollApi, type EnrollState } from "../api";
import { useBackHandler } from "../composables/useBack";
import { toast } from "../composables/useToast";

// Rozilik sahifasi yo'q: ma'lumotdan keyin darhol suratga olish (foydalanuvchi talabi).
type Step = "info" | "capture" | "review" | "done";

const { t } = useI18n();
const router = useRouter();
const route = useRoute();
/** Shaxs sahifasidan kelingan bo'lsa: mavjud shaxsga namuna qo'shish (ma'lumot va rozilik qadamlari yo'q). */
const personId = route.query.person ? Number(route.query.person) : null;
const step = ref<Step>("info");
const form = reactive({ fullName: "", externalId: "", note: "" });
const state = ref<EnrollState | null>(null);
const busy = ref(false);
const error = ref<string | null>(null);
const capture = ref<InstanceType<typeof FaceCapture> | null>(null);

async function refresh() {
  state.value = await enrollApi.state();
}

async function toCapture() {
  error.value = null;
  await enrollApi.start(personId === null ? undefined : personId);
  await refresh();
  step.value = "capture";
}

/** Hammasi olinganda yakun animatsiyasi (yashil doira) ko'rinib tursin, keyin ko'rib chiqish. */
const SUCCESS_MS = 1400;

async function onCaptured() {
  await refresh();
  if (state.value && state.value.complete) {
    setTimeout(() => {
      if (step.value === "capture") toReview();
    }, SUCCESS_MS);
  }
}

async function toReview() {
  if (capture.value) await capture.value.stop();
  await refresh();
  step.value = "review";
}

async function removeSample(i: number) {
  await enrollApi.removeSample(i);
  await refresh();
}

async function save(allowDuplicate = false) {
  busy.value = true;
  error.value = null;
  try {
    await enrollApi.commit(
      {
        fullName: form.fullName,
        externalId: form.externalId || null,
        note: form.note || null,
        // Rozilik ilovada so'ralmaydi — sana qayd etilmaydi (shaxs sahifasida "qayd etilmagan").
        consentAt: null,
      },
      allowDuplicate,
    );
    if (personId !== null) {
      toast(t("person.samplesAdded", { n: state.value ? state.value.samples.length : 0 }));
      router.replace("/persons/" + personId);
      return;
    }
    step.value = "done";
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

async function cancel() {
  if (capture.value) await capture.value.stop();
  await enrollApi.cancel().catch(() => {});
  if (personId !== null) {
    router.replace("/persons/" + personId);
    return;
  }
  form.fullName = form.externalId = form.note = "";
  state.value = null;
  step.value = "info";
}

/** Suratga olish/ko'rib chiqishdan orqaga: olingan namunalar yo'qoladi, shuning uchun so'raladi. */
async function stepBack() {
  const n = state.value ? state.value.samples.length : 0;
  if (n && !window.confirm(t("enroll.confirmDiscard", { n }))) return;
  if (personId !== null) return cancel();
  if (capture.value) await capture.value.stop();
  await enrollApi.cancel().catch(() => {});
  state.value = null;
  step.value = "info";
}

useBackHandler(() => {
  if (step.value === "capture" || step.value === "review") {
    stepBack();
    return true;
  }
  return false;
});

onMounted(async () => {
  if (personId === null) return;
  try {
    const p = await api.getPerson(personId);
    form.fullName = p.fullName;
    form.externalId = p.externalId || "";
    form.note = p.note || "";
    await toCapture();
  } catch (e) {
    error.value = String(e);
  }
});

const poseLabel = (p: string | null) => (p ? t("enroll.poseShort." + p) : "");
</script>

<template>
  <section class="page">
    <!-- Suratga olishda sarlavha yo'q: ekran to'liq Face ID paneliga beriladi -->
    <h1 v-if="step !== 'capture'" class="page-title">{{ personId !== null ? $t("person.addSamplesTitle", { name: form.fullName }) : $t("enroll.title") }}</h1>
    <p v-if="error && step !== 'review'" class="error wrap">{{ error }}</p>

    <!-- 1. Ma'lumot -->
    <form v-if="step === 'info' && personId === null" @submit.prevent="toCapture">
      <label class="field">
        <span>{{ $t("enroll.fullName") }} *</span>
        <input v-model="form.fullName" type="text" autocomplete="off" required />
      </label>
      <label class="field">
        <span>{{ $t("enroll.externalId") }}</span>
        <input v-model="form.externalId" type="text" autocomplete="off" />
      </label>
      <label class="field">
        <span>{{ $t("enroll.note") }}</span>
        <textarea v-model="form.note" rows="2" />
      </label>
      <button type="submit" class="btn primary" :disabled="!form.fullName.trim()">{{ $t("enroll.next") }}</button>
    </form>

    <!-- 2. Suratga olish -->
    <div v-else-if="step === 'capture' && state">
      <!-- Ko'rib chiqish halqa to'lganda o'zi ochiladi; bekor qilish — paneldagi ✕ -->
      <FaceCapture ref="capture" @captured="onCaptured" @close="stepBack" />
    </div>

    <!-- 3. Ko'rib chiqish -->
    <div v-else-if="step === 'review' && state">
      <p class="muted">{{ form.fullName }} · {{ $t("enroll.samples", { n: state.samples.length }) }}</p>
      <div class="thumbs">
        <div v-for="(s, i) in state.samples" :key="i" class="thumb">
          <img :src="s.thumb" alt="" />
          <span class="thumb-label">{{ poseLabel(s.pose) }}</span>
          <button type="button" class="thumb-del" :aria-label="$t('enroll.remove')" @click="removeSample(i)">×</button>
        </div>
      </div>
      <div v-if="state.duplicate" class="warn">
        {{ $t("enroll.duplicate", { name: state.duplicate.personName, score: state.duplicate.score.toFixed(2) }) }}
      </div>
      <p v-if="error" class="error wrap">{{ error }}</p>
      <button
        type="button"
        class="btn primary"
        :disabled="busy || !state.samples.length"
        @click="save(!!state.duplicate)"
      >
        {{ state.duplicate ? $t("enroll.saveAnyway") : $t("enroll.save") }}
      </button>
      <button type="button" class="btn spaced" @click="toCapture">{{ $t("enroll.retake") }}</button>
      <button type="button" class="btn spaced" @click="cancel">{{ $t("enroll.cancel") }}</button>
    </div>

    <!-- 4. Tayyor -->
    <div v-else-if="step === 'done'">
      <p class="success">{{ $t("enroll.saved", { name: form.fullName }) }}</p>
      <button type="button" class="btn primary" @click="router.push('/persons')">{{ $t("nav.persons") }}</button>
      <button type="button" class="btn spaced" @click="cancel">{{ $t("enroll.another") }}</button>
    </div>
  </section>
</template>

<style scoped>
.field { display: block; margin-bottom: 12px; }
.field span { display: block; font-size: 14px; color: var(--muted); margin-bottom: 4px; }
.field input, .field textarea {
  display: block; width: 100%; min-height: 44px; padding: 10px 12px; font: inherit; color: inherit;
  background: var(--surface); border: 1px solid var(--border); border-radius: 10px;
}
.spaced { margin-top: 8px; }
.thumbs { margin: 0 -4px 12px; }
.thumb { position: relative; display: inline-block; width: 30%; margin: 4px 1.5%; vertical-align: top; }
.thumb img { display: block; width: 100%; border-radius: 8px; }
.thumb-label { display: block; font-size: 12px; text-align: center; color: var(--muted); }
.thumb-del {
  position: absolute; right: 4px; top: 4px; width: 28px; height: 28px; border: 0; border-radius: 50%;
  background: rgba(0, 0, 0, .55); color: #fff; font-size: 18px; line-height: 28px; cursor: pointer;
}
.warn { padding: 10px 12px; margin-bottom: 12px; border-radius: 10px; background: #fef0c7; color: #93370d; }
.success { font-size: 18px; font-weight: 600; color: #12b76a; }
</style>
