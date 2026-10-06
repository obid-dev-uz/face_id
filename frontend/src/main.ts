import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./App.vue";
import { router } from "./router";
import { i18n } from "./i18n";
import "./styles.css";
import { waitForIpc } from "./ipcReady";

// Android: Tauri sahifa manzilini bilmaguncha chaqiruvlar rad etiladi (ipcReady.ts)
waitForIpc().then(() => createApp(App).use(createPinia()).use(router).use(i18n).mount("#app"));
