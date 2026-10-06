<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useCamera, type EnrollFeedback, type FrameHeader, type Pose } from "../composables/useCamera";
import { readFile } from "../api";
import CameraNotice from "./CameraNotice.vue";
import EngineBanner from "./EngineBanner.vue";

const props = defineProps<{ required: number }>();
const emit = defineEmits<{ (e: "captured"): void; (e: "addImage", bytes: Uint8Array): void }>();

const feedback = ref<EnrollFeedback | null>(null);
const flash = ref(false);

function onFrame(h: FrameHeader) {
  if (!h.enroll) return;
  feedback.value = h.enroll;
  if (h.enroll.captured) {
    flash.value = true;
    setTimeout(() => (flash.value = false), 250);
    emit("captured");
  }
}

const cam = useCamera({ enroll: true, onFrame });
onMounted(() => cam.start("front"));

const cameraOk = computed(() => ["idle", "starting", "running"].indexOf(cam.status.value) >= 0);
const count = computed(() => (feedback.value ? feedback.value.count : 0));

const signed = (v: number) => (v > 0 ? "+" : "") + Math.round(v);

/* Ikki halqa: Rust `DIRECTIONS` tartibida, 45° qadam, 0° — yaw musbat (foydalanuvchining
 * chapi). Ichki — boshni ozroq, tashqi — ko'proq burish (Rust `MIN_*` va `OUTER_*` ellipslari).
 * Koordinatalar viewer foizida; oval markazi (50, 45), yarim o'qlari 25 × 28 (.oval bilan mos). */
const DIRECTIONS: Pose[] = ["left", "upLeft", "up", "upRight", "right", "downRight", "down", "downLeft"];
const CX = 50, CY = 45, RX = 35, RY = 37;
/** Ichki halqa radiusi tashqisiga nisbatan. */
const INNER_K = 29 / 35;
/** Oldingi kamerada kadr ko'zgu kabi ko'rsatiladi: foydalanuvchining chapi ekranda chapda. */
const sx = computed(() => (cam.header.value && cam.header.value.mirrored ? -1 : 1));
const around = computed(() => !!feedback.value && feedback.value.target !== "frontal");

function point(deg: number, k: number) {
  const a = (deg * Math.PI) / 180;
  return [CX + sx.value * RX * k * Math.cos(a), CY - RY * k * Math.sin(a)];
}
const segments = computed(() => {
  const out: { key: string; points: string; done: boolean }[] = [];
  [INNER_K, 1].forEach((k, ring) => {
    DIRECTIONS.forEach((pose, i) => {
      const pts: string[] = [];
      for (let d = -18; d <= 18; d += 6) pts.push(point(i * 45 + d, k).join(","));
      const covered = feedback.value ? feedback.value.covered[ring] : null;
      out.push({ key: ring + pose, points: pts.join(" "), done: !!covered && covered.indexOf(pose) >= 0 });
    });
  });
  return out;
});
/** Boshning hozirgi yo'nalishi: Rust `Pose::direction` bilan bir xil ellipslar — ichki (12°, 7°)
 * chegarasida nuqta ichki halqada, tashqi (17°, 9°) chegarasida — tashqi halqada. */
const headDot = computed(() => {
  const f = feedback.value;
  if (!f || !around.value || f.yawDeg === null || f.pitchDeg === null) return null;
  const nx = f.yawDeg / 12, ny = f.pitchDeg / 7;
  const m = Math.sqrt(nx * nx + ny * ny);
  if (m < 0.01) return { left: CX + "%", top: CY + "%" };
  // Shu yo'nalishda tashqi chegara ichki ellips bo'yicha qayerda: m === outerAt.
  const mo = Math.sqrt((f.yawDeg / 17) * (f.yawDeg / 17) + (f.pitchDeg / 9) * (f.pitchDeg / 9));
  const outerAt = m / Math.max(mo, 1e-6);
  const k = m <= 1 ? m * INNER_K : Math.min(1.05, INNER_K + ((m - 1) / (outerAt - 1)) * (1 - INNER_K));
  const [x, y] = point((Math.atan2(ny, nx) * 180) / Math.PI, k);
  return { left: x + "%", top: y + "%" };
});

async function onFile(ev: Event) {
  const input = ev.target as HTMLInputElement;
  const files = input.files ? Array.prototype.slice.call(input.files) as File[] : [];
  input.value = "";
  for (const f of files) emit("addImage", await readFile(f));
}

defineExpose({ stop: cam.stop });
</script>

<template>
  <div>
    <EngineBanner />
    <!-- Kamera ishlamasa ham galereyadan qo'shish mumkin -->
    <CameraNotice :status="cam.status.value" :error="cam.error.value" @retry="cam.start('front')" @settings="cam.openSettings()" />
    <template v-if="cameraOk">
    <div class="viewer-wrap"><div class="viewer">
      <img
        v-if="cam.frameUrl.value"
        :src="cam.frameUrl.value"
        :class="{ mirrored: cam.header.value && cam.header.value.mirrored }"
        alt=""
        @load="cam.frameShown"
      />
      <div v-else class="placeholder">{{ $t("camera.starting") }}</div>
      <!-- Oval: yuz shu ichida bo'lishi kerak (Rust: placement_hint) -->
      <div class="oval" :class="{ ok: feedback && (feedback.hint === 'hold' || feedback.captured) }" />
      <svg class="ring" viewBox="0 0 100 100" preserveAspectRatio="none" aria-hidden="true">
        <polyline v-for="seg in segments" :key="seg.key" :points="seg.points" :class="{ done: seg.done }" />
      </svg>
      <div v-if="headDot" class="head-dot" :style="headDot" />
      <div v-if="flash" class="flash" />
    </div></div>

    <div class="progress">
      <span v-for="i in props.required" :key="i" class="dot" :class="{ done: i <= count }" />
    </div>

    <p class="instruction">
      <template v-if="!feedback">{{ $t("enroll.lookAtCamera") }}</template>
      <template v-else-if="feedback.target === null">{{ $t("enroll.allCaptured") }}</template>
      <template v-else-if="feedback.hint && feedback.hint !== 'turnPose' && !(around && feedback.hint === 'hold')">
        {{ $t("enroll.hint." + feedback.hint) }}
      </template>
      <template v-else>{{ around ? $t("enroll.rotate") : $t("enroll.frontal") }}</template>
    </p>
    <p v-if="feedback && feedback.yawDeg !== null" class="debug muted">
      yaw {{ signed(feedback.yawDeg) }}° · pitch {{ signed(feedback.pitchDeg || 0) }}° ·
      {{ $t("diag.sharpness") }} {{ Math.round(feedback.sharpness || 0) }}
    </p>
    </template>

    <div class="actions">
      <label class="btn file-btn">
        {{ $t("enroll.fromGallery") }}
        <input type="file" accept="image/*" multiple @change="onFile" />
      </label>
    </div>
  </div>
</template>

<style scoped>
/* Balandlik cheklangan: ko'rsatma va tugmalar telefon ekranida ko'rinib tursin */
.viewer-wrap { text-align: center; }
.viewer { position: relative; display: inline-block; max-width: 100%; line-height: 0; background: #000; border-radius: 12px; overflow: hidden; vertical-align: top; }
.viewer img { display: block; width: auto; height: auto; max-width: 100%; max-height: 46vh; }
.viewer img.mirrored { -webkit-transform: scaleX(-1); transform: scaleX(-1); }
.placeholder { line-height: 1.4; width: 260px; padding: 100px 16px; text-align: center; color: #98a2b3; }
/* placement_hint bilan mos: markaz (50%, 45%), kenglik ~45% */
.oval {
  position: absolute; left: 25%; top: 17%; width: 50%; height: 56%;
  border: 3px dashed rgba(255, 255, 255, .8); border-radius: 50%;
  box-shadow: 0 0 0 2000px rgba(0, 0, 0, .35);
}
.oval.ok { border-color: #12b76a; border-style: solid; }
/* Face ID kabi halqa: olingan yo'nalishlar yashil */
.ring { position: absolute; left: 0; top: 0; width: 100%; height: 100%; overflow: visible; }
.ring polyline {
  fill: none; stroke: rgba(255, 255, 255, .45); stroke-width: 5px; stroke-linecap: round;
  vector-effect: non-scaling-stroke; -webkit-transition: stroke .2s; transition: stroke .2s;
}
.ring polyline.done { stroke: #12b76a; }
.head-dot {
  position: absolute; width: 12px; height: 12px; margin: -6px 0 0 -6px; border-radius: 50%;
  background: #fff; box-shadow: 0 0 0 2px rgba(0, 0, 0, .4);
}
.flash { position: absolute; left: 0; top: 0; right: 0; bottom: 0; background: rgba(255, 255, 255, .6); }
.progress { text-align: center; margin: 8px 0 2px; }
.dot { display: inline-block; width: 9px; height: 9px; margin: 0 3px; border-radius: 50%; background: var(--border); }
.dot.done { background: #12b76a; }
.instruction { text-align: center; font-size: 18px; font-weight: 600; min-height: 26px; margin: 4px 0; }
.debug { text-align: center; font-size: 12px; margin: 0 0 8px; }
.actions .btn { margin-top: 8px; }
.file-btn { position: relative; overflow: hidden; text-align: center; line-height: 22px; }
.file-btn input { position: absolute; left: 0; top: 0; width: 100%; height: 100%; opacity: 0; }
</style>
