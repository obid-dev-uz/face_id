import { getVersion } from "@tauri-apps/api/app";
import { isTauri } from "./platform";

/* Android'da Tauri chaqiruv kelgan sahifa manzilini WebView'ning "sahifa yuklana boshladi"
 * signalidan oladi; ungacha manzil `about:blank` — bu "tashqi sayt" deb hisoblanadi va
 * release build'da barcha chaqiruvlar "Command … not allowed by ACL" bilan rad etiladi.
 * Tez telefonlarda (masalan, Realme C30) JS bu signaldan oldin ishga tushib ulguradi.
 * Shuning uchun ilova bitta zararsiz chaqiruv o'tguncha kutib, keyin ishga tushadi. */
// Release: "Command … not allowed by ACL"; debug: "… not allowed on origin …"
const ACL_DENIED = "not allowed";
const STEP_MS = 50;
const MAX_WAIT_MS = 3000;

const delay = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

export async function waitForIpc(probe: () => Promise<unknown> = getVersion): Promise<void> {
  if (!isTauri) return;
  for (let waited = 0; ; waited += STEP_MS) {
    try {
      await probe();
      return;
    } catch (e) {
      // Boshqa xato yoki kutish tugadi — ilova baribir ishga tushsin, xato o'z joyida ko'rinadi
      if (String(e).indexOf(ACL_DENIED) < 0 || waited >= MAX_WAIT_MS) return;
      await delay(STEP_MS);
    }
  }
}
