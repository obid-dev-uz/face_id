import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { createPinia } from "pinia";
import { createMemoryHistory, createRouter } from "vue-router";
import { i18n, setLocale } from "../i18n";

/* `isTauri` modul yuklanganda hisoblanadi: mockIPC'dan keyin modullar qayta import qilinadi. */
let calls: { cmd: string; args: unknown }[] = [];
let verdict: unknown = { result: "ok" };
let active = false;

beforeEach(() => {
  vi.resetModules();
  calls = [];
  verdict = { result: "ok" };
  active = false;
  mockIPC((cmd, args) => {
    calls.push({ cmd, args });
    if (cmd === "admin_status") return { configured: true, active, remainingMs: active ? 180000 : 0, timeoutMs: 180000, lockedMs: 0 };
    if (cmd === "admin_unlock") {
      if ((verdict as { result: string }).result === "ok") active = true;
      return verdict;
    }
    if (cmd === "admin_setup") {
      active = true;
      return null;
    }
    throw new Error("kutilmagan buyruq: " + cmd);
  });
  setLocale("uz");
});

afterEach(() => {
  clearMocks();
  vi.restoreAllMocks();
});

async function mountComp(component: unknown, props: Record<string, unknown> = {}) {
  const router = createRouter({ history: createMemoryHistory(), routes: [{ path: "/:p(.*)*", component: { template: "<div/>" } }] });
  router.push("/");
  await router.isReady();
  const w = mount(component as never, { props: props as never, global: { plugins: [createPinia(), router, i18n] } });
  await flushPromises();
  return w;
}

async function typePin(w: Awaited<ReturnType<typeof mountComp>>, pin: string) {
  const keys = w.findAll(".pin-key");
  for (const d of pin) {
    const k = keys.find((x) => x.text() === d);
    await k!.trigger("click");
  }
  await flushPromises();
}

describe("PIN qoidalari", () => {
  it("rejects trivial sequences like the Rust side", async () => {
    const { isWeakPin, fmtClock } = await import("../composables/useAdmin");
    for (const p of ["111111", "123456", "654321", "012345"]) expect(isWeakPin(p)).toBe(true);
    for (const p of ["428193", "135790", "000100"]) expect(isWeakPin(p)).toBe(false);
    expect(fmtClock(180000)).toBe("3:00");
    expect(fmtClock(9200)).toBe("0:10");
  });
});

describe("AdminBar", () => {
  it("shows sign-in for guests and opens the PIN dialog", async () => {
    const { default: AdminBar } = await import("../components/AdminBar.vue");
    const { adminState } = await import("../composables/useAdmin");
    const w = await mountComp(AdminBar);
    expect(w.text()).toContain("Kirish");
    await w.find("button").trigger("click");
    expect(adminState.pinOpen).toBe(true);
  });
});

describe("PinDialog", () => {
  it("unlocks with the right PIN", async () => {
    const { default: PinDialog } = await import("../components/PinDialog.vue");
    const { adminState } = await import("../composables/useAdmin");
    const w = await mountComp(PinDialog, { mode: "unlock" });
    await typePin(w, "428193");
    expect(calls[0]).toEqual({ cmd: "admin_unlock", args: { pin: "428193" } });
    expect(w.emitted("done")).toBeTruthy();
    expect(adminState.active).toBe(true);
  });

  it("shows remaining attempts on a wrong PIN", async () => {
    verdict = { result: "wrong", attemptsLeft: 2, lockedMs: 0 };
    const { default: PinDialog } = await import("../components/PinDialog.vue");
    const w = await mountComp(PinDialog, { mode: "unlock" });
    await typePin(w, "428193");
    expect(w.find(".pin-msg").text()).toContain("2 ta urinish");
    expect(w.findAll(".pin-dots i.on")).toHaveLength(0);
    expect(w.emitted("done")).toBeFalsy();
  });

  it("blocks the keypad while locked out", async () => {
    verdict = { result: "wrong", attemptsLeft: 0, lockedMs: 30000 };
    const { default: PinDialog } = await import("../components/PinDialog.vue");
    const w = await mountComp(PinDialog, { mode: "unlock" });
    await typePin(w, "428193");
    expect(w.find(".pin-msg").text()).toContain("0:30");
    expect(w.find(".pin-pad").classes()).toContain("disabled");
  });

  it("setup rejects weak and mismatched PINs before calling Rust", async () => {
    const { default: PinDialog } = await import("../components/PinDialog.vue");
    const w = await mountComp(PinDialog, { mode: "setup" });
    await typePin(w, "123456");
    expect(w.find(".pin-msg").text()).toContain("oddiy");
    await typePin(w, "428193");
    expect(w.find(".pin-title").text()).toContain("qayta");
    await typePin(w, "428194");
    expect(w.find(".pin-msg").text()).toContain("mos kelmadi");
    expect(calls.filter((c) => c.cmd === "admin_setup")).toHaveLength(0);
    await typePin(w, "428193");
    await typePin(w, "428193");
    expect(calls.find((c) => c.cmd === "admin_setup")).toEqual({ cmd: "admin_setup", args: { pin: "428193" } });
    expect(w.emitted("done")).toBeTruthy();
  });
});

describe("Router guard", () => {
  it("keeps guests on the live page", async () => {
    const { routes, installAdminGuard } = await import("../router");
    const { adminState } = await import("../composables/useAdmin");
    const r = createRouter({ history: createMemoryHistory(), routes });
    installAdminGuard(r);
    await r.push("/persons");
    expect(r.currentRoute.value.path).toBe("/live");
    adminState.active = true;
    await r.push("/settings");
    expect(r.currentRoute.value.path).toBe("/settings");
  });
});
