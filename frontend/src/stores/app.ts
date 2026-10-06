import { defineStore } from "pinia";
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "../platform";

export interface AppInfo {
  version: string;
  os: string;
  arch: string;
  dataDir: string;
  sqliteVersion: string;
  schemaVersion: number;
  totalRamMb: number | null;
}

export const useAppStore = defineStore("app", () => {
  const info = ref<AppInfo | null>(null);
  const error = ref<string | null>(null);

  async function loadInfo() {
    if (!isTauri) return;
    try {
      info.value = await invoke<AppInfo>("app_info");
      error.value = null;
    } catch (e) {
      error.value = String(e);
    }
  }

  return { info, error, loadInfo };
});
