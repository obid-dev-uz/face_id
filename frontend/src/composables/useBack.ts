// Android "Orqaga" tugmasi: avval sahifaning ichki qadami (masalan, ro'yxatga olish ustasi),
// keyin ota sahifa (route.meta.parent), keyin bosh sahifa; bosh sahifada ikki marta bosib chiqish.
import { onBeforeUnmount, onMounted } from "vue";
import type { Router } from "vue-router";
import { exit, onBackButtonPress } from "@tauri-apps/api/app";
import { platform } from "../platform";
import { toast } from "./useToast";

export const HOME = "/live";
const EXIT_WINDOW_MS = 2000;

/** `true` qaytarsa — bosish shu yerda ishlandi. */
type Handler = () => boolean;
const handlers: Handler[] = [];

/** Komponent ochiq turganda "Orqaga" ni o'zi ishlaydi; eng oxirgi ochilgani birinchi. */
export function useBackHandler(h: Handler) {
  onMounted(() => handlers.push(h));
  onBeforeUnmount(() => {
    const i = handlers.indexOf(h);
    if (i >= 0) handlers.splice(i, 1);
  });
}

/** Keyingi qadam qayerga: test qilish uchun alohida. */
export function backTarget(path: string, parent: unknown): string | null {
  if (typeof parent === "string") return parent;
  return path === HOME ? null : HOME;
}

export async function installBackButton(router: Router, exitHint: () => string) {
  if (platform() !== "android") return;
  let lastPress = 0;
  await onBackButtonPress(() => {
    for (let i = handlers.length - 1; i >= 0; i--) if (handlers[i]()) return;
    const route = router.currentRoute.value;
    const target = backTarget(route.path, route.meta.parent);
    if (target) {
      router.replace(target);
      return;
    }
    const now = Date.now();
    if (now - lastPress < EXIT_WINDOW_MS) {
      exit(0);
    } else {
      lastPress = now;
      toast(exitHint(), EXIT_WINDOW_MS);
    }
  });
}
