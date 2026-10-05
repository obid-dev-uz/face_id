// Rust buyruqlarining typed o'ramlari. Maydon nomlari Rust'dagi serde(camelCase) bilan bir xil.
import { invoke } from "@tauri-apps/api/core";

export interface Person {
  id: number;
  uuid: string;
  fullName: string;
  externalId: string | null;
  note: string | null;
  consentAt: string | null;
  isActive: boolean;
  sampleCount: number;
  createdAt: string;
  updatedAt: string;
}

export interface PersonInput {
  fullName: string;
  externalId?: string | null;
  note?: string | null;
  consentAt?: string | null;
}

export interface LogEntry {
  id: number;
  personId: number | null;
  personName: string | null;
  score: number;
  matched: boolean;
  source: string;
  hasSnapshot: boolean;
  createdAt: string;
}

export const api = {
  listPersons: (query?: string, includeInactive = false) =>
    invoke<Person[]>("list_persons", { query, includeInactive }),
  getPerson: (id: number) => invoke<Person>("get_person", { id }),
  createPerson: (input: PersonInput) => invoke<Person>("create_person", { input }),
  updatePerson: (id: number, input: PersonInput) => invoke<Person>("update_person", { id, input }),
  setPersonActive: (id: number, active: boolean) => invoke<void>("set_person_active", { id, active }),
  deletePerson: (id: number) => invoke<void>("delete_person", { id }),

  listLogs: (beforeId?: number, limit = 50) => invoke<LogEntry[]>("list_logs", { beforeId, limit }),
  purgeLogs: (olderThanDays: number) => invoke<number>("purge_logs", { olderThanDays }),

  getSettings: () => invoke<Record<string, string>>("get_settings"),
  setSetting: (key: string, value: string) => invoke<void>("set_setting", { key, value }),

  /** Yozilgan fayl yo'lini qaytaradi. */
  backupDatabase: (dest?: string) => invoke<string>("backup_database", { dest }),
  restoreDatabase: (src: string) => invoke<void>("restore_database", { src }),
};
