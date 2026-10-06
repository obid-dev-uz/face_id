<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref } from "vue";
import { listen } from "@tauri-apps/api/event";
import { useI18n } from "vue-i18n";
import { api, type LogEntry } from "../api";
import { isTauri } from "../platform";
import { useBackHandler } from "../composables/useBack";
import { toast } from "../composables/useToast";

const { t } = useI18n();
const PAGE = 30;
const logs = ref<LogEntry[]>([]);
const snaps = reactive<Record<number, string>>({});
const more = ref(false);
const busy = ref(false);
const error = ref<string | null>(null);

/** Tanlash rejimi: bir nechta yozuvni belgilab, birdan o'chirish. */
const selecting = ref(false);
const selected = reactive<Record<number, boolean>>({});
const selectedIds = computed(() => logs.value.filter((l) => selected[l.id]).map((l) => l.id));
const allSelected = computed(() => logs.value.length > 0 && selectedIds.value.length === logs.value.length);

/** Rasmlar ketma-ket olinadi: eski telefonda bir vaqtda o'nlab so'rov UI'ni qotiradi. */
async function loadSnapshots(items: LogEntry[]) {
  for (const l of items) {
    if (!l.hasSnapshot || snaps[l.id]) continue;
    const url = await api.logSnapshot(l.id).catch(() => null);
    if (url) snaps[l.id] = url;
  }
}

async function load(reset: boolean) {
  if (!isTauri || busy.value) return;
  busy.value = true;
  try {
    const last = logs.value[logs.value.length - 1];
    const page = await api.listLogs(reset || !last ? undefined : last.id, PAGE);
    logs.value = reset ? page : logs.value.concat(page);
    more.value = page.length === PAGE;
    error.value = null;
    loadSnapshots(page);
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

/** SQLite `datetime('now')` — UTC, "YYYY-MM-DD HH:MM:SS". */
function when(s: string) {
  const d = new Date(s.replace(" ", "T") + "Z");
  return isNaN(d.getTime()) ? s : d.toLocaleString();
}

function setSelecting(on: boolean) {
  selecting.value = on;
  for (const k of Object.keys(selected)) delete selected[Number(k)];
}

function toggleAll() {
  const on = !allSelected.value;
  for (const l of logs.value) selected[l.id] = on;
}

/** O'chirilganlar ro'yxatdan darhol olib tashlanadi (qayta yuklamasdan). */
async function remove(ids: number[], confirmText: string) {
  if (!ids.length || !window.confirm(confirmText)) return;
  busy.value = true;
  try {
    const n = await api.deleteLogs(ids);
    const gone: Record<number, boolean> = {};
    ids.forEach((id) => (gone[id] = true));
    logs.value = logs.value.filter((l) => !gone[l.id]);
    ids.forEach((id) => {
      delete snaps[id];
      delete selected[id];
    });
    toast(t("logs.deleted", { n }));
    if (!logs.value.length) selecting.value = false;
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

const removeOne = (l: LogEntry) => remove([l.id], t("logs.confirmDeleteOne"));
const removeSelected = () => remove(selectedIds.value, t("logs.confirmDeleteSelected", { n: selectedIds.value.length }));

async function clearAll() {
  if (!window.confirm(t("logs.confirmClear"))) return;
  busy.value = true;
  try {
    await api.clearLogs();
    logs.value = [];
    more.value = false;
    setSelecting(false);
    toast(t("logs.cleared"));
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

/** Tanlash rejimida qatorni bosish — belgilash. */
function onRow(l: LogEntry) {
  if (selecting.value) selected[l.id] = !selected[l.id];
}

useBackHandler(() => {
  if (!selecting.value) return false;
  setSelecting(false);
  return true;
});

let unlisten: (() => void) | null = null;
onMounted(async () => {
  await load(true);
  // Tanlash paytida ro'yxat o'zgarib ketmasin: yangi yozuvlar rejimdan chiqqanda ko'rinadi.
  if (isTauri) unlisten = await listen("log-added", () => !selecting.value && load(true));
});
onBeforeUnmount(() => {
  if (unlisten) unlisten();
});
</script>

<template>
  <section class="page">
    <div class="head">
      <h1 class="page-title">{{ $t("nav.logs") }}</h1>
      <button v-if="logs.length" type="button" class="link" @click="setSelecting(!selecting)">
        {{ selecting ? $t("logs.done") : $t("logs.select") }}
      </button>
    </div>
    <p v-if="error" class="error">{{ error }}</p>
    <p v-else-if="!logs.length && !busy" class="muted">{{ $t("logs.empty") }}</p>

    <div v-if="selecting" class="selbar">
      <label class="check">
        <input type="checkbox" :checked="allSelected" @change="toggleAll" />
        <span>{{ $t("logs.selectAll") }}</span>
      </label>
      <button type="button" class="del-sel" :disabled="busy || !selectedIds.length" @click="removeSelected">
        {{ $t("logs.deleteSelected", { n: selectedIds.length }) }}
      </button>
    </div>

    <ul v-if="logs.length" class="list">
      <li v-for="l in logs" :key="l.id" class="row" :class="{ sel: selecting && selected[l.id] }" @click="onRow(l)">
        <input v-if="selecting" type="checkbox" class="row-check" :checked="!!selected[l.id]" @click.stop="onRow(l)" />
        <img v-if="snaps[l.id]" :src="snaps[l.id]" class="snap" alt="" />
        <span v-else class="snap" />
        <div class="row-main">
          <!-- Tanilgan shaxs: sahifasiga o'tib, uni tizimdan butunlay o'chirish mumkin -->
          <router-link v-if="l.matched && l.personId !== null && !selecting" :to="'/persons/' + l.personId" class="name">
            {{ l.personName || "#" + l.personId }}
          </router-link>
          <div v-else class="name" :class="{ unknown: !l.matched }">
            {{ l.matched ? l.personName || "#" + l.personId : $t("logs.unknown") }}
          </div>
          <div class="muted small">{{ when(l.createdAt) }} · {{ l.score.toFixed(2) }}</div>
        </div>
        <button v-if="!selecting" type="button" class="del" :aria-label="$t('logs.delete')" :disabled="busy" @click.stop="removeOne(l)">×</button>
      </li>
    </ul>
    <button v-if="more" type="button" class="btn" :disabled="busy" @click="load(false)">{{ $t("logs.loadMore") }}</button>
    <button v-if="logs.length && !selecting" type="button" class="btn danger spaced" :disabled="busy" @click="clearAll">
      {{ $t("logs.clear") }}
    </button>
  </section>
</template>

<style scoped>
.head { display: flex; align-items: center; }
.head .page-title { flex: 1 1 auto; }
.link { border: 0; background: transparent; color: var(--accent); min-height: 44px; padding: 0 4px; cursor: pointer; }
.selbar {
  display: flex; align-items: center; padding: 6px 12px; margin-bottom: 8px;
  background: var(--surface); border: 1px solid var(--border); border-radius: 10px;
}
.check { flex: 1 1 auto; display: flex; align-items: center; min-height: 44px; }
.check input, .row-check { width: 22px; height: 22px; margin: 0 10px 0 0; flex: 0 0 auto; }
.del-sel { border: 0; background: transparent; color: var(--danger); min-height: 44px; padding: 0 4px; cursor: pointer; font-weight: 600; }
.del-sel:disabled { opacity: .4; }
.list { list-style: none; margin: 0 0 16px; padding: 0; background: var(--surface); border: 1px solid var(--border); border-radius: 10px; }
.row { display: flex; align-items: center; padding: 10px 14px; border-top: 1px solid var(--border); }
.row:first-child { border-top: 0; }
.row.sel { background: rgba(31, 111, 235, .08); }
.snap { flex: 0 0 auto; display: block; width: 48px; height: 48px; margin-right: 12px; border-radius: 8px; background: var(--border); }
.row-main { flex: 1 1 auto; min-width: 0; }
.name { display: block; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: inherit; text-decoration: none; }
.name.unknown { color: var(--danger); }
.small { font-size: 13px; }
.del {
  flex: 0 0 auto; width: 44px; height: 44px; margin-right: -10px; border: 0; background: transparent;
  color: var(--muted); font-size: 22px; cursor: pointer;
}
.danger { color: var(--danger); }
.spaced { margin-top: 8px; }
</style>
