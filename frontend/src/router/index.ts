import { createRouter, createWebHashHistory } from "vue-router";

export const routes = [
  { path: "/", redirect: "/live" },
  { path: "/live", name: "live", component: () => import("../views/LiveView.vue") },
  { path: "/persons", name: "persons", component: () => import("../views/PersonsView.vue") },
  { path: "/enroll", name: "enroll", component: () => import("../views/EnrollView.vue") },
  { path: "/logs", name: "logs", component: () => import("../views/LogsView.vue") },
  { path: "/settings", name: "settings", component: () => import("../views/SettingsView.vue") },
];

// Hash history: Tauri asset protokoli va Android WebView'da qayta yuklashda 404 bo'lmaydi.
export const router = createRouter({ history: createWebHashHistory(), routes });
