<script setup lang="ts">
import { onMounted, reactive, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import { useI18n } from "vue-i18n";
import { api, type Person, type PersonSample } from "../api";
import { isTauri } from "../platform";
import { toast } from "../composables/useToast";

const { t } = useI18n();
const route = useRoute();
const router = useRouter();
const id = Number(route.params.id);

const person = ref<Person | null>(null);
const samples = ref<PersonSample[]>([]);
const form = reactive({ fullName: "", externalId: "", note: "" });
const busy = ref(false);
const error = ref<string | null>(null);

async function load() {
  if (!isTauri) return;
  try {
    const p = await api.getPerson(id);
    person.value = p;
    form.fullName = p.fullName;
    form.externalId = p.externalId || "";
    form.note = p.note || "";
    samples.value = await api.listPersonSamples(id);
    error.value = null;
  } catch (e) {
    error.value = String(e);
  }
}

async function act(fn: () => Promise<unknown>, done?: string) {
  busy.value = true;
  try {
    await fn();
    if (done) toast(done);
    error.value = null;
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

const save = () =>
  act(async () => {
    person.value = await api.updatePerson(id, { fullName: form.fullName, externalId: form.externalId || null, note: form.note || null });
  }, t("person.saved"));

const toggleActive = () =>
  act(async () => {
    if (!person.value) return;
    await api.setPersonActive(id, !person.value.isActive);
    await load();
  });

async function removeSample(s: PersonSample) {
  // Oxirgi namuna o'chirilsa, shaxs umuman tanilmaydi — alohida ogohlantiramiz.
  const msg = samples.value.length === 1 ? t("person.confirmDeleteLast") : t("person.confirmDeleteSample");
  if (!window.confirm(msg)) return;
  await act(async () => {
    await api.deletePersonSample(s.id);
    await load();
  });
}

async function removePerson() {
  if (!person.value || !window.confirm(t("persons.confirmDelete", { name: person.value.fullName }))) return;
  await act(async () => {
    await api.deletePerson(id);
    router.replace("/persons");
  });
}

onMounted(load);
</script>

<template>
  <section class="page">
    <router-link to="/persons" class="back muted">‹ {{ $t("nav.persons") }}</router-link>
    <p v-if="error" class="error wrap">{{ error }}</p>
    <template v-if="person">
      <h1 class="page-title">{{ person.fullName }}</h1>
      <p v-if="!person.isActive" class="warn">{{ $t("person.inactiveHint") }}</p>

      <form @submit.prevent="save">
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
        <button type="submit" class="btn primary" :disabled="busy || !form.fullName.trim()">{{ $t("person.save") }}</button>
      </form>

      <label class="check">
        <input type="checkbox" :checked="person.isActive" :disabled="busy" @change="toggleActive" />
        <span>{{ $t("person.active") }}</span>
      </label>

      <h2 class="section-title">{{ $t("enroll.samples", { n: samples.length }) }}</h2>
      <p v-if="!samples.length" class="warn">{{ $t("person.noSamples") }}</p>
      <div class="thumbs">
        <div v-for="s in samples" :key="s.id" class="thumb">
          <img v-if="s.thumb" :src="s.thumb" alt="" />
          <button type="button" class="thumb-del" :aria-label="$t('enroll.remove')" :disabled="busy" @click="removeSample(s)">×</button>
        </div>
      </div>
      <router-link :to="{ path: '/enroll', query: { person: String(id) } }" class="btn add">{{ $t("person.addSamples") }}</router-link>

      <button type="button" class="btn danger spaced" :disabled="busy" @click="removePerson">{{ $t("persons.delete") }}</button>
    </template>
  </section>
</template>

<style scoped>
.back { display: inline-block; min-height: 44px; line-height: 44px; text-decoration: none; }
.field { display: block; margin-bottom: 12px; }
.field span { display: block; font-size: 14px; color: var(--muted); margin-bottom: 4px; }
.field input, .field textarea {
  display: block; width: 100%; min-height: 44px; padding: 10px 12px; font: inherit; color: inherit;
  background: var(--surface); border: 1px solid var(--border); border-radius: 10px;
}
.check { display: flex; align-items: center; margin: 16px 0; min-height: 44px; }
.check input { width: 22px; height: 22px; margin: 0 10px 0 0; flex: 0 0 auto; }
.thumbs { margin: 0 -4px 12px; }
.thumb { position: relative; display: inline-block; width: 22%; margin: 4px 1.5%; vertical-align: top; }
.thumb img { display: block; width: 100%; border-radius: 8px; }
.thumb-del {
  position: absolute; right: 2px; top: 2px; width: 28px; height: 28px; border: 0; border-radius: 50%;
  background: rgba(0, 0, 0, .55); color: #fff; font-size: 18px; line-height: 28px; cursor: pointer;
}
.add { text-align: center; text-decoration: none; }
.danger { color: var(--danger); }
.spaced { margin-top: 24px; }
.warn { padding: 10px 12px; margin: 0 0 12px; border-radius: 10px; background: #fef0c7; color: #93370d; }
</style>
