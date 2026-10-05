import { describe, expect, it } from "vitest";
import { mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import { createMemoryHistory, createRouter } from "vue-router";
import App from "../App.vue";
import { routes } from "../router";
import { i18n, setLocale } from "../i18n";

async function mountAt(path: string) {
  const router = createRouter({ history: createMemoryHistory(), routes });
  router.push(path);
  await router.isReady();
  return mount(App, { global: { plugins: [createPinia(), router, i18n] } });
}

describe("App shell", () => {
  it("renders five tabs and marks the active one", async () => {
    setLocale("uz");
    const w = await mountAt("/persons");
    const tabs = w.findAll(".tab");
    expect(tabs).toHaveLength(5);
    expect(w.find(".router-link-active").text()).toBe("Shaxslar");
  });

  it("switches language", async () => {
    setLocale("ru");
    const w = await mountAt("/live");
    expect(w.find(".router-link-active").text()).toBe("Камера");
    setLocale("uz");
  });
});
