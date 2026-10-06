import { ref } from "vue";
import { listen } from "@tauri-apps/api/event";
import { engineApi, type EngineStatus } from "../api";
import { isTauri } from "../platform";

/** Modellar holati: ilova bo'ylab bitta, "engine-status" hodisasi bilan yangilanadi. */
const status = ref<EngineStatus | null>(null);
let started = false;

async function refresh() {
  status.value = await engineApi.status().catch(() => status.value);
}

export function useEngineStatus() {
  if (!started && isTauri) {
    started = true;
    refresh();
    listen("engine-status", refresh);
  }
  return status;
}
