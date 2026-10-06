<script setup lang="ts">
import { onBeforeUnmount, onMounted, reactive, ref } from "vue";
import { listen } from "@tauri-apps/api/event";
import { api, type LogEntry } from "../api";
import { isTauri } from "../platform";

const PAGE = 30;
const logs = ref<LogEntry[]>([]);
const snaps = reactive<Record<number, string>>({});
const more = ref(false);
const busy = ref(false);
const error = ref<string | null>(null);

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

let unlisten: (() => void) | null = null;
onMounted(async () => {
  await load(true);
  if (isTauri) unlisten = await listen("log-added", () => load(true));
});
onBeforeUnmount(() => {
  if (unlisten) unlisten();
});
</script>

<template>
  <section class="page">
    <h1 class="page-title">{{ $t("nav.logs") }}</h1>
    <p v-if="error" class="error">{{ error }}</p>
    <p v-else-if="!logs.length && !busy" class="muted">{{ $t("logs.empty") }}</p>
    <ul v-if="logs.length" class="list">
      <li v-for="l in logs" :key="l.id" class="row">
        <img v-if="snaps[l.id]" :src="snaps[l.id]" class="snap" alt="" />
        <span v-else class="snap" />
        <div class="row-main">
          <div class="name" :class="{ unknown: !l.matched }">
            {{ l.matched ? l.personName || "#" + l.personId : $t("logs.unknown") }}
          </div>
          <div class="muted small">{{ when(l.createdAt) }} · {{ l.score.toFixed(2) }}</div>
        </div>
      </li>
    </ul>
    <button v-if="more" type="button" class="btn" :disabled="busy" @click="load(false)">{{ $t("logs.loadMore") }}</button>
  </section>
</template>

<style scoped>
.list { list-style: none; margin: 0 0 16px; padding: 0; background: var(--surface); border: 1px solid var(--border); border-radius: 10px; }
.row { display: flex; align-items: center; padding: 10px 14px; border-top: 1px solid var(--border); }
.row:first-child { border-top: 0; }
.snap { flex: 0 0 auto; display: block; width: 48px; height: 48px; margin-right: 12px; border-radius: 8px; background: var(--border); }
.row-main { flex: 1 1 auto; min-width: 0; }
.name { font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.name.unknown { color: var(--danger); }
.small { font-size: 13px; }
</style>
