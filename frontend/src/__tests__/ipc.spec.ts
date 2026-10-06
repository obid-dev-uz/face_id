import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { createPinia } from "pinia";
import { createMemoryHistory, createRouter } from "vue-router";
import { i18n, setLocale } from "../i18n";

/* `isTauri` modul yuklanganda bir marta hisoblanadi, shuning uchun mockIPC'dan keyin
 * komponentlar qayta import qilinadi (vi.resetModules). */
const ANDROID_UA = (chrome: number) =>
  `Mozilla/5.0 (Linux; Android 7.0; wv) AppleWebKit/537.36 Chrome/${chrome}.0.2743.100 Mobile Safari/537.36`;

let calls: { cmd: string; args: unknown }[] = [];
let persons: { id: number; fullName: string; isActive: boolean; sampleCount: number; externalId: string | null }[] = [];

beforeEach(() => {
  vi.resetModules();
  calls = [];
  persons = [
    { id: 1, fullName: "Ali Valiyev", isActive: true, sampleCount: 5, externalId: "T-01" },
    { id: 2, fullName: "Olim Karimov", isActive: false, sampleCount: 3, externalId: null },
  ];
  mockIPC((cmd, args) => {
    calls.push({ cmd, args });
    if (cmd === "list_persons") return persons;
    if (cmd === "delete_person") {
      persons = persons.filter((p) => p.id !== (args as { id: number }).id);
      return null;
    }
    throw new Error("kutilmagan buyruq: " + cmd);
  });
  setLocale("uz");
});

afterEach(() => {
  clearMocks();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  localStorage.clear();
});

async function mountView(path: string, component: unknown) {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: "/:p(.*)*", component: component as never }],
  });
  router.push(path);
  await router.isReady();
  const w = mount(component as never, { global: { plugins: [createPinia(), router, i18n] } });
  await flushPromises();
  return w;
}

describe("PersonsView (mock IPC)", () => {
  it("lists active and inactive persons", async () => {
    const { default: PersonsView } = await import("../views/PersonsView.vue");
    const w = await mountView("/persons", PersonsView);
    expect(calls[0]).toEqual({ cmd: "list_persons", args: { query: undefined, includeInactive: true } });
    const rows = w.findAll(".row");
    expect(rows).toHaveLength(2);
    expect(rows[0].text()).toContain("Ali Valiyev");
    expect(rows[0].text()).toContain("T-01");
    expect(rows[1].find(".name.inactive").exists()).toBe(true);
  });

  it("deletes after confirmation and reloads", async () => {
    vi.spyOn(window, "confirm").mockReturnValue(true);
    const { default: PersonsView } = await import("../views/PersonsView.vue");
    const w = await mountView("/persons", PersonsView);
    await w.findAll(".del")[0].trigger("click");
    await flushPromises();
    expect(calls.map((c) => c.cmd)).toEqual(["list_persons", "delete_person", "list_persons"]);
    expect(w.findAll(".row")).toHaveLength(1);
  });

  it("does nothing when deletion is cancelled", async () => {
    vi.spyOn(window, "confirm").mockReturnValue(false);
    const { default: PersonsView } = await import("../views/PersonsView.vue");
    const w = await mountView("/persons", PersonsView);
    await w.findAll(".del")[0].trigger("click");
    await flushPromises();
    expect(calls.map((c) => c.cmd)).toEqual(["list_persons"]);
  });
});

describe("WebViewNotice", () => {
  async function mountNotice(chrome: number) {
    vi.stubGlobal("navigator", { ...navigator, userAgent: ANDROID_UA(chrome) });
    const { default: WebViewNotice } = await import("../components/WebViewNotice.vue");
    return mountView("/", WebViewNotice);
  }

  it("warns on old Android WebView and remembers dismissal", async () => {
    const w = await mountNotice(53);
    expect(w.find(".wv").text()).toContain("Chrome 53");
    await w.findAll(".wv .btn")[1].trigger("click");
    expect(w.find(".wv").exists()).toBe(false);

    const again = await mountNotice(53);
    expect(again.find(".wv").exists()).toBe(false);
  });

  it("stays hidden on a recent WebView", async () => {
    const w = await mountNotice(71);
    expect(w.find(".wv").exists()).toBe(false);
  });
});
