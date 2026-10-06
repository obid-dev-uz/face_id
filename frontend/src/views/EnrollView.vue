<script setup lang="ts">
import { reactive, ref } from "vue";
import { useRouter } from "vue-router";
import { useI18n } from "vue-i18n";
import FaceCapture from "../components/FaceCapture.vue";
import { enrollApi, type EnrollState } from "../api";

type Step = "info" | "consent" | "capture" | "review" | "done";

const { t } = useI18n();
const router = useRouter();
const step = ref<Step>("info");
const form = reactive({ fullName: "", externalId: "", note: "" });
const consent = ref(false);
const state = ref<EnrollState | null>(null);
const busy = ref(false);
const error = ref<string | null>(null);
const notice = ref<string | null>(null);
const capture = ref<InstanceType<typeof FaceCapture> | null>(null);

async function refresh() {
  state.value = await enrollApi.state();
}

async function toCapture() {
  error.value = null;
  await enrollApi.start();
  await refresh();
  step.value = "capture";
}

async function onCaptured() {
  await refresh();
  if (state.value && state.value.samples.length >= state.value.required) await toReview();
}

async function onAddImage(bytes: Uint8Array) {
  try {
    const r = await enrollApi.addImage(bytes);
    notice.value = r.accepted ? t("enroll.imageAccepted") : t("enroll.hint." + (r.hint || "noFace"));
    await refresh();
  } catch (e) {
    notice.value = String(e);
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
        consentAt: new Date().toISOString(),
      },
      allowDuplicate,
    );
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
  form.fullName = form.externalId = form.note = "";
  consent.value = false;
  state.value = null;
  step.value = "info";
}

const poseLabel = (p: string | null) => (p ? t("enroll.poseShort." + p) : t("enroll.gallery"));
</script>

<template>
  <section class="page">
    <h1 class="page-title">{{ $t("enroll.title") }}</h1>

    <!-- 1. Ma'lumot -->
    <form v-if="step === 'info'" @submit.prevent="step = 'consent'">
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

    <!-- 2. Rozilik -->
    <div v-else-if="step === 'consent'">
      <p>{{ $t("enroll.consentText", { name: form.fullName.trim() }) }}</p>
      <label class="check">
        <input v-model="consent" type="checkbox" />
        <span>{{ $t("enroll.consentCheck") }}</span>
      </label>
      <button type="button" class="btn primary" :disabled="!consent" @click="toCapture">{{ $t("enroll.next") }}</button>
      <button type="button" class="btn spaced" @click="step = 'info'">{{ $t("enroll.back") }}</button>
    </div>

    <!-- 3. Suratga olish -->
    <div v-else-if="step === 'capture' && state">
      <FaceCapture ref="capture" :required="state.required" @captured="onCaptured" @add-image="onAddImage" />
      <p v-if="notice" class="muted center">{{ notice }}</p>
      <button type="button" class="btn spaced" :disabled="!state.samples.length" @click="toReview">
        {{ $t("enroll.reviewNow", { n: state.samples.length }) }}
      </button>
      <button type="button" class="btn spaced" @click="cancel">{{ $t("enroll.cancel") }}</button>
    </div>

    <!-- 4. Ko'rib chiqish -->
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

    <!-- 5. Tayyor -->
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
.check { display: flex; align-items: flex-start; margin: 16px 0; }
.check input { width: 22px; height: 22px; margin: 2px 10px 0 0; flex: 0 0 auto; }
.spaced { margin-top: 8px; }
.center { text-align: center; }
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
