import { ref } from "vue";

/** Ilova bo'ylab bitta qisqa xabar (Android Toast kabi). */
export const toastText = ref<string | null>(null);
let timer: ReturnType<typeof setTimeout> | null = null;

export function toast(text: string, ms = 2000) {
  toastText.value = text;
  if (timer) clearTimeout(timer);
  timer = setTimeout(() => (toastText.value = null), ms);
}
