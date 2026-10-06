import { createRouter, createWebHashHistory, type Router } from "vue-router";
import { adminState } from "../composables/useAdmin";

// Kamera (tanish) — hamma uchun; qolganlari faqat admin sessiyasida (meta.admin).
export const routes = [
  { path: "/", redirect: "/live" },
  { path: "/live", name: "live", component: () => import("../views/LiveView.vue") },
  { path: "/persons", name: "persons", component: () => import("../views/PersonsView.vue"), meta: { admin: true } },
  // meta.parent — Android "Orqaga" tugmasi shu sahifaga qaytaradi (composables/useBack.ts).
  {
    path: "/persons/:id",
    name: "person",
    component: () => import("../views/PersonView.vue"),
    meta: { parent: "/persons", admin: true },
  },
  { path: "/enroll", name: "enroll", component: () => import("../views/EnrollView.vue"), meta: { admin: true } },
  { path: "/logs", name: "logs", component: () => import("../views/LogsView.vue"), meta: { admin: true } },
  { path: "/settings", name: "settings", component: () => import("../views/SettingsView.vue"), meta: { admin: true } },
];

/** Asosiy himoya Rust'da (har bir buyruq tekshiriladi); bu — mehmon admin sahifasini ochmasligi uchun. */
export function installAdminGuard(r: Router) {
  r.beforeEach((to) => (to.meta.admin && !adminState.active ? "/live" : true));
}

// Hash history: Tauri asset protokoli va Android WebView'da qayta yuklashda 404 bo'lmaydi.
export const router = createRouter({ history: createWebHashHistory(), routes });
installAdminGuard(router);
