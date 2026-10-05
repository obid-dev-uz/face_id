export type Platform = "android" | "ios" | "windows" | "macos" | "linux" | "web";

/** Tauri ichidamizmi yoki oddiy brauzerdami (vite dev / vitest). */
export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export function platform(): Platform {
  const ua = navigator.userAgent;
  if (!isTauri) return "web";
  if (/Android/i.test(ua)) return "android";
  if (/iPhone|iPad|iPod/i.test(ua)) return "ios";
  if (/Windows/i.test(ua)) return "windows";
  if (/Mac OS X/i.test(ua)) return "macos";
  return "linux";
}

export const isMobile = (): boolean => {
  const p = platform();
  return p === "android" || p === "ios";
};
