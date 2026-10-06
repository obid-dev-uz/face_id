// Administrator rejimi. Haqiqiy holat va taymer Rust'da (admin.rs): bu yerda faqat aks etadi,
// ekranga tegishlar backend'ga "amal" sifatida yuboriladi va sessiya yopilganda xabar beriladi.
import { computed, reactive, ref } from "vue";
import { adminApi, type AdminSessionEvent, type AdminStatus } from "../api";
import { isTauri } from "../platform";

/** Shuncha qolganda ogohlantirish chiqadi. */
export const WARN_MS = 30_000;
/** Ekranga tegishlar backend'ga ko'pi bilan shu oraliqda yuboriladi. */
const TOUCH_EVERY_MS = 10_000;
export const PIN_LEN = 6;

type EndReason = "logout" | "expire" | "reset";

export const adminState = reactive({
  /** Brauzerda (backend yo'q) UI ustida ishlash uchun hammasi ochiq. */
  configured: !isTauri,
  active: !isTauri,
  /** `Date.now()` shkalasida. */
  deadline: 0,
  timeoutMs: 180_000,
  lockedUntil: 0,
  pinOpen: false,
});

const now = ref(Date.now());
export const remainingMs = computed(() => (adminState.active ? Math.max(0, adminState.deadline - now.value) : 0));
export const sessionWarn = computed(() => adminState.active && isTauri && remainingMs.value <= WARN_MS);

let ticker: ReturnType<typeof setInterval> | null = null;
let lastTouch = 0;
const endListeners: ((reason: EndReason) => void)[] = [];

function tick() {
  now.value = Date.now();
  // Backend o'zi yopadi va xabar beradi; bu — xabar yo'qolgan holat uchun zaxira.
  if (adminState.active && now.value > adminState.deadline + 2000) refresh();
}

function apply(s: { active: boolean; remainingMs: number; timeoutMs: number }, reason: EndReason = "expire") {
  const was = adminState.active;
  adminState.active = s.active;
  adminState.deadline = Date.now() + s.remainingMs;
  adminState.timeoutMs = s.timeoutMs;
  now.value = Date.now();
  if (s.active && !ticker) ticker = setInterval(tick, 1000);
  if (!s.active && ticker) {
    clearInterval(ticker);
    ticker = null;
  }
  if (was && !s.active) endListeners.forEach((f) => f(reason));
}

function applyStatus(s: AdminStatus) {
  adminState.configured = s.configured;
  adminState.lockedUntil = s.lockedMs > 0 ? Date.now() + s.lockedMs : 0;
  apply(s);
}

export async function refresh() {
  if (!isTauri) return;
  try {
    applyStatus(await adminApi.status());
  } catch (e) {
    /* backend hali tayyor emas */
  }
}

/** Ilova ochilganda bir marta. */
export async function initAdmin() {
  if (!isTauri) return;
  const { listen } = await import("@tauri-apps/api/event");
  await listen<AdminSessionEvent>("admin://session", (e) => apply(e.payload, e.payload.reason || "expire"));
  // Fondan qaytganda: telefon uxlab yotganda muddat o'tgan bo'lishi mumkin.
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible") refresh();
  });
  for (const ev of ["touchstart", "mousedown", "keydown", "scroll", "input"]) {
    document.addEventListener(ev, () => touch(), { capture: true, passive: true });
  }
  await refresh();
}

/** Har qanday foydalanuvchi harakati sessiyani uzaytiradi. `force` — oraliqni kutmasdan. */
export function touch(force = false) {
  if (!adminState.active || !isTauri) return;
  const t = Date.now();
  // Ogohlantirish chiqqanda darhol (lekin scroll hodisalari bilan IPC'ni to'ldirmasdan).
  const gap = remainingMs.value > WARN_MS ? TOUCH_EVERY_MS : 1000;
  if (!force && t - lastTouch < gap) return;
  lastTouch = t;
  adminApi.touch().then(applyStatus, refresh);
}

export async function logout() {
  if (!isTauri) return;
  await adminApi.lock().catch(() => {});
  await refresh();
}

export function onSessionEnd(f: (reason: EndReason) => void) {
  endListeners.push(f);
}

export function openPin() {
  adminState.pinOpen = true;
  // Blok holati (qolgan vaqt) yangilanadi
  refresh();
}

/** PIN ochilgandan keyin (`setup`/`unlock`) backend xabari kelguncha kutmaslik uchun. */
export async function afterUnlock() {
  lastTouch = Date.now();
  await refresh();
}

/** Rust `admin::validate_pin` bilan bir xil qoida (xabarni tarjima qilish uchun shu yerda ham). */
export function isWeakPin(pin: string): boolean {
  const d = pin.split("").map(Number);
  const step = d[1] - d[0];
  if (Math.abs(step) > 1) return false;
  for (let i = 1; i < d.length; i++) if (d[i] - d[i - 1] !== step) return false;
  return true;
}

export function fmtClock(ms: number): string {
  const s = Math.ceil(ms / 1000);
  const m = Math.floor(s / 60);
  const r = s % 60;
  return m + ":" + (r < 10 ? "0" : "") + r;
}
