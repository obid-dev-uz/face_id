<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { useCamera, type EnrollFeedback, type FrameHeader } from "../composables/useCamera";
import { enrollApi } from "../api";
import CameraNotice from "./CameraNotice.vue";
import EngineBanner from "./EngineBanner.vue";

const emit = defineEmits<{ (e: "captured"): void; (e: "close"): void }>();

const feedback = ref<EnrollFeedback | null>(null);
const pulse = ref(false);

/* ---------- Ekran geometriyasi ----------
 * Butun ekran — kamera (xira), faqat doira ichi tiniq. Tasvir ekranni `cover` bilan to'ldiradi;
 * doira va kadr koordinatalari shu o'lchov orqali bog'lanadi. Doiraning kadrdagi joyi Rust'ga
 * yuboriladi: yuz shu doira ichiga kirmaguncha rasm olinmaydi (enroll::placement_hint). */
const vw = ref(window.innerWidth);
const vh = ref(window.innerHeight);
const onResize = () => {
  vw.value = window.innerWidth;
  vh.value = window.innerHeight;
};

const circle = computed(() => {
  const r = Math.min(vw.value * 0.4, vh.value * 0.27);
  return { x: vw.value / 2, y: Math.max(r * 1.3 + 16, vh.value * 0.4), r };
});

/** Kadr → ekran (px). */
const view = computed(() => {
  const h = cam.header.value;
  if (!h) return null;
  const s = Math.max(vw.value / h.width, vh.value / h.height);
  return { s, ox: (vw.value - h.width * s) / 2, oy: (vh.value - h.height * s) / 2, w: h.width, h: h.height, mirrored: h.mirrored };
});

let sentRegion = "";
function sendRegion() {
  const v = view.value;
  if (!v) return;
  const c = circle.value;
  let fx = (c.x - v.ox) / v.s;
  if (v.mirrored) fx = v.w - fx;
  const region = { cx: fx / v.w, cy: (c.y - v.oy) / v.s / v.h, r: c.r / v.s / v.w };
  const key = JSON.stringify(region);
  if (key === sentRegion) return;
  sentRegion = key;
  enrollApi.setRegion(region).catch(() => (sentRegion = ""));
}

/* ---------- Yuz "to'ri": sharning ikki katta aylanasi ----------
 * Vertikal (tepadan iyakka) va gorizontal (chakkadan chakkaga) chiziq yuzni 4 bo'lakka
 * ajratadi; ular yuz markazida kesishadi va bosh burilishiga qarab shar sirtidagidek egiladi.
 * Shar yuz ramkasiga ergashadi; koordinatalar — ekran px. */
interface Mesh { cx: number; cy: number; r: number; yaw: number; pitch: number }
const mesh = ref<Mesh | null>(null);
/** Silliqlash: titramasin, lekin harakatdan orqada qolmasin. */
const SMOOTH = 0.45;
/** Landmarklardan olingan burchaklar haqiqiydan kichikroq — chiziqlar ko'rinarli egilsin. */
const ANGLE_GAIN = 1.6;

function updateMesh(h: FrameHeader) {
  const e = h.enroll;
  const v = view.value;
  if (!h.faces.length || !e || !v) {
    mesh.value = null;
    return;
  }
  const f = h.faces.reduce((a, b) => (b.bbox[2] * b.bbox[3] > a.bbox[2] * a.bbox[3] ? b : a));
  const [x, y, w, bh] = f.bbox;
  const fx = x + w / 2;
  const clamp = (val: number) => Math.max(-70, Math.min(70, val * ANGLE_GAIN));
  const t: Mesh = {
    cx: v.ox + (v.mirrored ? v.w - fx : fx) * v.s,
    cy: v.oy + (y + bh / 2) * v.s,
    r: Math.max(w, bh) * v.s * 0.62,
    // Burchak noma'lum bo'lsa (yuz endigina topildi) — to'g'ri qaragan to'r
    yaw: clamp(e.yawDeg || 0),
    pitch: clamp(e.pitchDeg || 0),
  };
  const m = mesh.value;
  mesh.value = m
    ? {
        cx: m.cx + (t.cx - m.cx) * SMOOTH, cy: m.cy + (t.cy - m.cy) * SMOOTH, r: m.r + (t.r - m.r) * SMOOTH,
        yaw: m.yaw + (t.yaw - m.yaw) * SMOOTH, pitch: m.pitch + (t.pitch - m.pitch) * SMOOTH,
      }
    : t;
}

const meshLines = computed(() => {
  const m = mesh.value;
  if (!m) return null;
  const sx = mirrored.value ? -1 : 1;
  const ty = (m.yaw * Math.PI) / 180, tp = (m.pitch * Math.PI) / 180;
  /** Yuzga bog'langan nuqta (x — o'ng, y — tepa, z — oldinga) → ekran; orqa tomoni ko'rinmaydi. */
  const project = (x: number, y: number, z: number) => {
    const x1 = x * Math.cos(ty) + z * Math.sin(ty), z1 = -x * Math.sin(ty) + z * Math.cos(ty);
    const y2 = y * Math.cos(tp) + z1 * Math.sin(tp), z2 = -y * Math.sin(tp) + z1 * Math.cos(tp);
    return z2 < -0.02 ? null : (m.cx + sx * m.r * x1).toFixed(1) + "," + (m.cy - m.r * y2).toFixed(1);
  };
  const curve = (fn: (l: number) => [number, number, number]) => {
    const pts: string[] = [];
    for (let d = -90; d <= 90; d += 6) {
      const l = (d * Math.PI) / 180;
      const p = project(...fn(l));
      if (p) pts.push(p);
    }
    return pts.join(" ");
  };
  return {
    vertical: curve((l) => [0, Math.sin(l), Math.cos(l)]),
    horizontal: curve((l) => [Math.sin(l), 0, Math.cos(l)]),
  };
});

function onFrame(h: FrameHeader) {
  if (!h.enroll) return;
  sendRegion();
  updateMesh(h);
  feedback.value = h.enroll;
  if (h.enroll.captured) {
    pulse.value = true;
    setTimeout(() => (pulse.value = false), 300);
    emit("captured");
  }
}

const cam = useCamera({ enroll: true, onFrame });
onMounted(() => {
  window.addEventListener("resize", onResize);
  cam.start("front");
});
onBeforeUnmount(() => window.removeEventListener("resize", onResize));

/** Doira atrofidagi sahna (halqa SVG'si): viewBox 0..100, doira radiusi 39. */
const stageStyle = computed(() => {
  const c = circle.value, side = (c.r * 2) / 0.78;
  return { left: c.x - side / 2 + "px", top: c.y - side / 2 + "px", width: side + "px", height: side + "px" };
});
const clipStyle = computed(() => {
  const c = circle.value, clip = "circle(" + c.r + "px at " + c.x + "px " + c.y + "px)";
  return { WebkitClipPath: clip, clipPath: clip };
});
const ringStyle = computed(() => {
  const c = circle.value;
  return { left: c.x - c.r + "px", top: c.y - c.r + "px", width: c.r * 2 + "px", height: c.r * 2 + "px" };
});
const textStyle = computed(() => ({ top: circle.value.y + circle.value.r * 1.3 + 12 + "px" }));

const cameraOk = computed(() => ["idle", "starting", "running"].indexOf(cam.status.value) >= 0);
const signed = (v: number) => (v > 0 ? "+" : "") + Math.round(v);

/* ---------- Face ID uslubidagi halqa ----------
 * 72 ta chiziqcha = Rust `enroll::BINS` (5° dan). Burchak Rust `polar` bilan bir xil:
 * (yaw/12°, pitch/7°) tekisligida, 0° — yaw musbat (foydalanuvchining chapi).
 * Bosh burilgan tomondagi chiziqchalar uzayadi; to'liq uzaygan (chegaraga yetgan) zahoti
 * Rust shu joyda rasm oladi va ±20° (COVER_HALF_DEG) yashil bo'ladi. */
const TICKS = 72;
const COVER_HALF = 20;
const DIAGONAL_K = 0.75;

/** Oldingi kamerada kadr ko'zgu kabi: foydalanuvchining chapi ekranda chapda. */
const mirrored = computed(() => !!(cam.header.value && cam.header.value.mirrored));
/** Ekran burchagi (0° — o'ng, soat miliga teskari) ↔ Rust burchagi. */
const toScreen = (raw: number) => (mirrored.value ? 180 - raw : raw);

const frontalDone = computed(() => !!feedback.value && feedback.value.target !== "frontal");
const finished = computed(() => !!feedback.value && feedback.value.target === null);

/** Bosh qaysi tomonga va chegaragacha qancha burilgan: 1 — rasm olinadigan masofa. */
const head = computed(() => {
  const f = feedback.value;
  if (!f || !frontalDone.value || f.yawDeg === null || f.pitchDeg === null) return null;
  const nx = f.yawDeg / 12, ny = f.pitchDeg / 7;
  const raw = (Math.atan2(ny, nx) * 180) / Math.PI;
  const k = 1 - (1 - DIAGONAL_K) * Math.abs(Math.sin((2 * raw * Math.PI) / 180));
  const r = Math.sqrt(nx * nx + ny * ny) / k;
  return r < 0.25 ? null : { deg: toScreen(raw), r: Math.min(1, r) };
});

const ticks = computed(() => {
  const f = feedback.value;
  const covered = f && frontalDone.value ? f.covered : [];
  const h = head.value;
  const out: { x1: number; y1: number; x2: number; y2: number; cls: string }[] = [];
  for (let i = 0; i < TICKS; i++) {
    const deg = (i + 0.5) * (360 / TICKS);
    const raw = (((mirrored.value ? 180 - deg : deg) % 360) + 360) % 360;
    const done = finished.value || !!covered[Math.floor(raw / 5)];
    // Uzayish: bosh yo'nalishidagi ±20° (rasm olinsa aynan shu qism yashil bo'ladi)
    let grow = 0;
    if (h) {
      const d = Math.abs(((deg - h.deg + 540) % 360) - 180);
      if (d <= COVER_HALF) grow = h.r * (1 - (d / COVER_HALF) * 0.4);
    }
    const a = (deg * Math.PI) / 180;
    const r0 = 43, r1 = 47 + grow * 4;
    out.push({
      x1: 50 + r0 * Math.cos(a), y1: 50 - r0 * Math.sin(a),
      x2: 50 + r1 * Math.cos(a), y2: 50 - r1 * Math.sin(a),
      cls: done ? (grow > 0 ? "done long" : "done") : grow > 0.2 ? "active" : "",
    });
  }
  return out;
});

const instruction = computed(() => {
  const f = feedback.value;
  if (!f) return "enroll.lookAtCamera";
  if (f.target === null) return "enroll.allCaptured";
  if (f.hint && f.hint !== "turnPose") return "enroll.hint." + f.hint;
  return frontalDone.value ? "enroll.rotate" : "enroll.frontal";
});

defineExpose({ stop: cam.stop });
</script>

<template>
  <div>
    <CameraNotice :status="cam.status.value" :error="cam.error.value" @retry="cam.start('front')" @settings="cam.openSettings()" />
    <!-- Kamera ishlamasa to'liq ekranli panel (va undagi ✕) yo'q — chiqish yo'li qolsin -->
    <button v-if="!cameraOk" type="button" class="btn" @click="emit('close')">{{ $t("enroll.cancel") }}</button>

    <!-- Face ID uslubi: butun ekran kamera, doiradan tashqarisi xira -->
    <div v-if="cameraOk" class="fid">
      <template v-if="cam.frameUrl.value">
        <img :src="cam.frameUrl.value" class="bg" :class="{ mirrored }" alt="" />
        <div class="sharp" :style="clipStyle">
          <img :src="cam.frameUrl.value" :class="{ mirrored }" alt="" @load="cam.frameShown" />
          <!-- Yuz to'ri: kesishgan joy — yuz markazi, egrilik — bosh burilishi -->
          <svg v-if="!finished && meshLines" class="scan" :viewBox="'0 0 ' + vw + ' ' + vh" aria-hidden="true">
            <polyline class="glow" :points="meshLines.vertical" />
            <polyline class="glow" :points="meshLines.horizontal" />
            <polyline class="line" :points="meshLines.vertical" />
            <polyline class="line" :points="meshLines.horizontal" />
          </svg>
        </div>
      </template>
      <div v-else class="placeholder" :style="textStyle">{{ $t("camera.starting") }}</div>

      <div class="edge" :class="{ pulse }" :style="ringStyle" />
      <div v-if="finished" class="success" :style="ringStyle">
        <svg viewBox="0 0 52 52" aria-hidden="true"><path d="M14 27l8 8 16-17" /></svg>
      </div>
      <svg class="ticks" :style="stageStyle" viewBox="0 0 100 100" aria-hidden="true">
        <line v-for="(t, i) in ticks" :key="i" :x1="t.x1" :y1="t.y1" :x2="t.x2" :y2="t.y2" :class="t.cls" />
      </svg>

      <div class="text" :style="textStyle">
        <p class="instruction">{{ $t(instruction) }}</p>
        <p class="count">{{ $t("enroll.samples", { n: feedback ? feedback.count : 0 }) }}</p>
        <p v-if="feedback && feedback.yawDeg !== null" class="debug">
          yaw {{ signed(feedback.yawDeg) }}° · pitch {{ signed(feedback.pitchDeg || 0) }}° ·
          {{ $t("diag.sharpness") }} {{ Math.round(feedback.sharpness || 0) }}
        </p>
      </div>

      <div class="banner"><EngineBanner /></div>
      <button type="button" class="close" :aria-label="$t('enroll.cancel')" @click="emit('close')">
        <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 6l12 12M18 6L6 18" /></svg>
      </button>
    </div>
  </div>
</template>

<style scoped>
/* Butun ekran (pastki navigatsiya ham yopiladi) */
.fid { position: fixed; left: 0; top: 0; right: 0; bottom: 0; z-index: 50; overflow: hidden; background: #000; color: #fff; }
.fid img { position: absolute; left: 0; top: 0; width: 100%; height: 100%; object-fit: cover; }
/* Doiradan tashqari: xira va qoraytirilgan */
.bg {
  -webkit-filter: blur(14px) brightness(.55); filter: blur(14px) brightness(.55);
  -webkit-transform: scale(1.08); transform: scale(1.08);
}
.bg.mirrored { -webkit-transform: scale(-1.08, 1.08); transform: scale(-1.08, 1.08); }
.sharp { position: absolute; left: 0; top: 0; width: 100%; height: 100%; }
.sharp img.mirrored { -webkit-transform: scaleX(-1); transform: scaleX(-1); }
.edge { position: absolute; border-radius: 50%; box-shadow: 0 0 0 1px rgba(255, 255, 255, .15); -webkit-transition: box-shadow .3s; transition: box-shadow .3s; }
.edge.pulse { box-shadow: 0 0 0 4px rgba(48, 209, 88, .75); }
.ticks { position: absolute; overflow: visible; }
.ticks line {
  stroke: rgba(255, 255, 255, .45); stroke-width: 1.1; stroke-linecap: round;
  -webkit-transition: stroke .35s; transition: stroke .35s;
}
.ticks line.active { stroke: rgba(255, 255, 255, .95); }
.ticks line.done { stroke: #30d158; }
.ticks line.done.long { stroke: #5be37d; }
.scan { position: absolute; left: 0; top: 0; width: 100%; height: 100%; }
.scan polyline { fill: none; stroke-linecap: round; stroke-linejoin: round; }
.scan .glow { stroke: rgba(120, 220, 255, .35); stroke-width: 7; }
.scan .line { stroke: rgba(225, 250, 255, .95); stroke-width: 2; }
.success { position: absolute; border-radius: 50%; background: rgba(48, 209, 88, .85); -webkit-animation: fade .4s ease-out; animation: fade .4s ease-out; }
.success svg { position: absolute; left: 25%; top: 25%; width: 50%; height: 50%; }
.success path {
  fill: none; stroke: #fff; stroke-width: 4; stroke-linecap: round; stroke-linejoin: round;
  stroke-dasharray: 40; stroke-dashoffset: 40;
  -webkit-animation: draw .5s .2s ease-out forwards; animation: draw .5s .2s ease-out forwards;
}
@-webkit-keyframes fade { from { opacity: 0; } to { opacity: 1; } }
@keyframes fade { from { opacity: 0; } to { opacity: 1; } }
@-webkit-keyframes draw { to { stroke-dashoffset: 0; } }
@keyframes draw { to { stroke-dashoffset: 0; } }
.text { position: absolute; left: 16px; right: 16px; text-align: center; text-shadow: 0 1px 3px rgba(0, 0, 0, .6); }
.instruction { font-size: 20px; font-weight: 600; line-height: 1.3; margin: 0 0 6px; }
.count { font-size: 13px; color: rgba(255, 255, 255, .65); margin: 0; }
.debug { font-size: 11px; color: rgba(255, 255, 255, .45); margin: 6px 0 0; }
.placeholder { position: absolute; left: 0; right: 0; text-align: center; color: #98a2b3; }
.banner { position: absolute; left: 16px; right: 64px; top: 12px; color: var(--text); }
.close {
  position: absolute; right: 8px; top: 8px; width: 48px; height: 48px; padding: 12px;
  border: 0; border-radius: 50%; background: rgba(0, 0, 0, .35); cursor: pointer;
}
.close svg { display: block; width: 24px; height: 24px; fill: none; stroke: #fff; stroke-width: 2.2; stroke-linecap: round; }
</style>
