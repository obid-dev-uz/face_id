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

export interface EngineStatus {
  state: "loading" | "ready" | "failed";
  error: string | null;
  modelId: string;
  samples: number;
}

export type QualityIssue = "tooSmall" | "lowScore" | "blurry" | "notFrontal" | "tooDark" | "tooBright";

export interface FoundFace {
  face: { bbox: [number, number, number, number]; score: number; landmarks: [number, number][] };
  quality: { faceWidth: number; score: number; sharpness: number; yawDeg: number; brightness: number; issues: QualityIssue[] };
  personId: number | null;
  personName: string | null;
  score: number | null;
}

export interface Analysis {
  width: number;
  height: number;
  faces: FoundFace[];
  timings: { detectMs: number; alignMs: number; embedMs: number };
  decodeMs: number;
  matchMs: number;
}

export interface Stat {
  avgMs: number;
  minMs: number;
  maxMs: number;
}

export interface InferenceStats {
  threads: number;
  detect640: Stat;
  detect320: Stat;
  detect160: Stat;
  embed: Stat;
}

export interface Benchmark {
  iterations: number;
  facesFound: number;
  single: InferenceStats;
  multi: InferenceStats;
  align: Stat;
  match10k: Stat;
  arch: string;
}

export const engineApi = {
  status: () => invoke<EngineStatus>("engine_status"),
  /** Rasm baytlari xom holda yuboriladi (base64'siz). */
  analyzeImage: (bytes: Uint8Array) => invoke<Analysis>("analyze_image", bytes),
  benchmark: (bytes: Uint8Array | null, iterations = 10) =>
    invoke<Benchmark>("benchmark_engine", bytes || new Uint8Array(0), {
      headers: { "x-iterations": String(iterations) },
    }),
};

/** Chrome 53 (Android 7 WebView) da Blob.arrayBuffer() yo'q. */
export function readFile(file: Blob): Promise<Uint8Array> {
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(new Uint8Array(r.result as ArrayBuffer));
    r.onerror = () => reject(r.error);
    r.readAsArrayBuffer(file);
  });
}
