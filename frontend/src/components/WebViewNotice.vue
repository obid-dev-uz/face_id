<script setup lang="ts">
import { ref } from "vue";
import { openUrl } from "@tauri-apps/plugin-opener";
import { MIN_RECOMMENDED_WEBVIEW, platform, webviewMajor } from "../platform";

/* Eski Android 7 qurilmalarda System WebView yangilanmagan bo'lishi mumkin. Ilova baribir
 * ishlaydi (legacy bundle), lekin yangilash tavsiya qilinadi. Yopilsa, shu versiya uchun
 * qayta chiqmaydi (yangilangach yana tekshiriladi). */
const PLAY_URL = "https://play.google.com/store/apps/details?id=com.google.android.webview";
const major = webviewMajor();
const key = "webviewNotice.dismissed." + major;

function dismissed(): boolean {
  try {
    return localStorage.getItem(key) === "1";
  } catch (e) {
    return false;
  }
}

const show = ref(platform() === "android" && major !== null && major < MIN_RECOMMENDED_WEBVIEW && !dismissed());

function close() {
  show.value = false;
  try {
    localStorage.setItem(key, "1");
  } catch (e) {
    /* xotira yo'q — keyingi safar yana chiqadi */
  }
}

const update = () => openUrl(PLAY_URL).catch(() => {});
</script>

<template>
  <div v-if="show" class="wv" role="alert">
    <p>{{ $t("webview.old", { v: major, min: MIN_RECOMMENDED_WEBVIEW }) }}</p>
    <div class="row">
      <button type="button" class="btn primary" @click="update">{{ $t("webview.update") }}</button>
      <button type="button" class="btn" @click="close">{{ $t("webview.later") }}</button>
    </div>
  </div>
</template>

<style scoped>
.wv { margin-bottom: 12px; padding: 12px; border-radius: 10px; background: #fef0c7; color: #93370d; }
.wv p { margin: 0 0 10px; }
.row .btn { display: inline-block; width: 48%; margin-right: 2%; }
.row .btn:last-child { margin-right: 0; }
</style>
