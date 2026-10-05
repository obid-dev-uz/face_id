<script setup lang="ts">
import { onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { api, type Person } from "../api";
import { isTauri } from "../platform";

const { t } = useI18n();
const persons = ref<Person[]>([]);
const error = ref<string | null>(null);

async function load() {
  if (!isTauri) return;
  try {
    persons.value = await api.listPersons();
  } catch (e) {
    error.value = String(e);
  }
}

async function remove(p: Person) {
  if (!window.confirm(t("persons.confirmDelete", { name: p.fullName }))) return;
  await api.deletePerson(p.id);
  await load();
}

onMounted(load);
</script>

<template>
  <section class="page">
    <h1 class="page-title">{{ $t("nav.persons") }}</h1>
    <p v-if="error" class="error">{{ error }}</p>
    <p v-else-if="!persons.length" class="muted">{{ $t("persons.empty") }}</p>
    <ul v-else class="list">
      <li v-for="p in persons" :key="p.id" class="row">
        <div class="row-main">
          <div class="name">{{ p.fullName }}</div>
          <div class="muted small">{{ $t("enroll.samples", { n: p.sampleCount }) }}<template v-if="p.externalId"> · {{ p.externalId }}</template></div>
        </div>
        <button type="button" class="del" @click="remove(p)">{{ $t("persons.delete") }}</button>
      </li>
    </ul>
    <router-link to="/enroll" class="btn primary add">{{ $t("persons.add") }}</router-link>
  </section>
</template>

<style scoped>
.list { list-style: none; margin: 0 0 16px; padding: 0; background: var(--surface); border: 1px solid var(--border); border-radius: 10px; }
.row { display: flex; align-items: center; padding: 12px 14px; border-top: 1px solid var(--border); }
.row:first-child { border-top: 0; }
.row-main { flex: 1 1 auto; min-width: 0; }
.name { font-weight: 600; }
.small { font-size: 13px; }
.del { flex: 0 0 auto; margin-left: 8px; border: 0; background: transparent; color: var(--danger); min-height: 44px; padding: 0 8px; cursor: pointer; }
.add { text-align: center; text-decoration: none; }
</style>
