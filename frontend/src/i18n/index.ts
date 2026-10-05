import { createI18n } from "vue-i18n";
import uz from "./uz";
import ru from "./ru";
import en from "./en";

export type Locale = "uz" | "ru" | "en";
export const LOCALES: Locale[] = ["uz", "ru", "en"];
const KEY = "khoface-locale";

function initialLocale(): Locale {
  try {
    const saved = localStorage.getItem(KEY) as Locale | null;
    if (saved && LOCALES.indexOf(saved) >= 0) return saved;
  } catch (e) {
    /* localStorage yo'q bo'lishi mumkin */
  }
  return "uz";
}

export const i18n = createI18n({
  legacy: false,
  locale: initialLocale(),
  fallbackLocale: "uz",
  messages: { uz, ru, en },
});

export function setLocale(l: Locale) {
  i18n.global.locale.value = l;
  try {
    localStorage.setItem(KEY, l);
  } catch (e) {
    /* e'tiborsiz */
  }
}
