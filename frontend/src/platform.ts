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

/** WebView (Chrome) asosiy versiyasi; aniqlab bo'lmasa — `null`. */
export function webviewMajor(ua: string = navigator.userAgent): number | null {
  const m = ua.match(/Chrome\/(\d+)\./);
  return m ? parseInt(m[1], 10) : null;
}

/** Bundan eski WebView'da ilova legacy bundle bilan ishlaydi, lekin sekin va xatolarga moyil:
 *  foydalanuvchiga "Android System WebView"ni yangilash tavsiya qilinadi (reja, 10-bosqich). */
export const MIN_RECOMMENDED_WEBVIEW = 60;

export const isMobile = (): boolean => {
  const p = platform();
  return p === "android" || p === "ios";
};
