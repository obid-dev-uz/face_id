<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useCamera, type EnrollFeedback, type FrameHeader, type Pose } from "../composables/useCamera";
import { readFile } from "../api";

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

const count = computed(() => (feedback.value ? feedback.value.count : 0));

const signed = (v: number) => (v > 0 ? "+" : "") + Math.round(v);

/* Halqa: Rust `DIRECTIONS` tartibida, 45° qadam, 0° — yaw musbat (foydalanuvchining chapi).
 * Koordinatalar viewer foizida; oval markazi (50, 45), yarim o'qlari 25 × 28 (.oval bilan mos). */
const DIRECTIONS: Pose[] = ["left", "upLeft", "up", "upRight", "right", "downRight", "down", "downLeft"];
const CX = 50, CY = 45, RX = 30, RY = 32;
/** Oldingi kamerada kadr ko'zgu kabi ko'rsatiladi: foydalanuvchining chapi ekranda chapda. */
const sx = computed(() => (cam.header.value && cam.header.value.mirrored ? -1 : 1));
const around = computed(() => !!feedback.value && feedback.value.target !== "frontal");

function point(deg: number, k = 1) {
  const a = (deg * Math.PI) / 180;
  return [CX + sx.value * RX * k * Math.cos(a), CY - RY * k * Math.sin(a)];
}
const segments = computed(() =>
  DIRECTIONS.map((pose, i) => {
    const pts: string[] = [];
    for (let d = -18; d <= 18; d += 6) pts.push(point(i * 45 + d).join(","));
    const done = !!feedback.value && feedback.value.covered.indexOf(pose) >= 0;
    return { pose, points: pts.join(" "), done };
  }),
);
/** Boshning hozirgi yo'nalishi: Rust `Pose::direction` bilan bir xil ellips (12°, 7°). */
const headDot = computed(() => {
  const f = feedback.value;
  if (!f || !around.value || f.yawDeg === null || f.pitchDeg === null) return null;
  const nx = f.yawDeg / 12, ny = f.pitchDeg / 7;
  const m = Math.sqrt(nx * nx + ny * ny);
  if (m < 0.01) return { left: CX + "%", top: CY + "%" };
  const [x, y] = point((Math.atan2(ny, nx) * 180) / Math.PI, Math.min(m, 1.2) / 1.2);
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
        <polyline v-for="seg in segments" :key="seg.pose" :points="seg.points" :class="{ done: seg.done }" />
      </svg>
      <div v-if="headDot" class="head-dot" :style="headDot" />
      <div v-if="flash" class="flash" />
    </div></div>

    <div class="progress">
      <span v-for="i in props.required" :key="i" class="dot" :class="{ done: i <= count }" />
    </div>

    <p class="instruction">
      <template v-if="cam.status.value === 'denied'">{{ $t("camera.deniedTitle") }}</template>
      <template v-else-if="!feedback">{{ $t("enroll.lookAtCamera") }}</template>
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
  fill: none; stroke: rgba(255, 255, 255, .45); stroke-width: 6px; stroke-linecap: round;
  vector-effect: non-scaling-stroke; -webkit-transition: stroke .2s; transition: stroke .2s;
}
.ring polyline.done { stroke: #12b76a; }
.head-dot {
  position: absolute; width: 12px; height: 12px; margin: -6px 0 0 -6px; border-radius: 50%;
  background: #fff; box-shadow: 0 0 0 2px rgba(0, 0, 0, .4);
}
.flash { position: absolute; left: 0; top: 0; right: 0; bottom: 0; background: rgba(255, 255, 255, .6); }
.progress { text-align: center; margin: 8px 0 2px; }
.dot { display: inline-block; width: 12px; height: 12px; margin: 0 5px; border-radius: 50%; background: var(--border); }
.dot.done { background: #12b76a; }
.instruction { text-align: center; font-size: 18px; font-weight: 600; min-height: 26px; margin: 4px 0; }
.debug { text-align: center; font-size: 12px; margin: 0 0 8px; }
.actions .btn { margin-top: 8px; }
.file-btn { position: relative; overflow: hidden; text-align: center; line-height: 22px; }
.file-btn input { position: absolute; left: 0; top: 0; width: 100%; height: 100%; opacity: 0; }
</style>
