<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useCamera } from "../composables/useCamera";
import CameraNotice from "../components/CameraNotice.vue";
import EngineBanner from "../components/EngineBanner.vue";

const { t } = useI18n();

const cam = useCamera();

/* Kamera maydoni o'lchami (px): kadr unga `object-fit: cover` bilan joylanadi. */
const fsEl = ref<HTMLElement | null>(null);
const area = ref({ w: 0, h: 0 });
function measure() {
  const el = fsEl.value;
  if (el) area.value = { w: el.clientWidth, h: el.clientHeight };
}
onMounted(() => {
  window.addEventListener("resize", measure);
  cam.start();
});
onBeforeUnmount(() => window.removeEventListener("resize", measure));
function onShown() {
  if (!area.value.w) measure();
  cam.frameShown();
}

/** Rust `recognize::MIN_FACE_PX`: bundan kichik yuz tanilmaydi. */
const MIN_FACE_PX = 48;

const cameraOk = computed(() => ["idle", "starting", "running"].indexOf(cam.status.value) >= 0);

/** Pastdagi natija. Tanish faqat kadrda bitta yuz bo'lganda ishlaydi (Rust ham shuni talab qiladi).
 *  Yuz yo'q bo'lsa — panel ko'rsatilmaydi. */
const result = computed<{ kind: "warn" | "known" | "unknown"; text: string; sub?: string } | null>(() => {
  const h = cam.header.value;
  if (!h || !h.faces.length) return null;
  if (h.faces.length > 1) return { kind: "warn", text: t("live.oneFace"), sub: t("live.facesInFrame", { n: h.faces.length }) };
  if (h.faces[0].bbox[2] < MIN_FACE_PX) return { kind: "warn", text: t("live.closer") };
  // Modellar hali yuklanmagan bo'lsa treklar yo'q
  const tr = h.tracks && h.tracks[0];
  // Aniqlanmoqda: yozuv yo'q, burchak qavslar "nafas oladi"
  if (!tr || tr.status === "pending") return null;
  if (tr.status === "unknown") return { kind: "unknown", text: t("live.unknown") };
  return {
    kind: "known",
    text: tr.name || "#" + tr.personId,
    sub: tr.score !== null ? t("live.similarity", { v: Math.round(tr.score * 100) }) : undefined,
  };
});

/** <1: YuNet ramkasi yuzning ko'rinadigan qismidan kattaroq. */
const FOCUS_PAD = 0.9;
/** Yuz qayerdaligini ko'rsatuvchi burchak qavslar (ramka emas), yuz ramkasi atrofida.
 *  Rangi natijaga mos: aniqlanmoqda — oq, tanildi — yashil, noma'lum — qizil, bir nechta yuz — sariq. */
const focus = computed(() => {
  const h = cam.header.value;
  const { w, h: ah } = area.value;
  if (!h || !h.faces.length || !w || !ah) return [];
  const s = Math.max(w / h.width, ah / h.height);
  const ox = (w - h.width * s) / 2, oy = (ah - h.height * s) / 2;
  const kind = result.value ? result.value.kind : "pending";
  return h.faces.map((f, i) => {
    const [x, y, fw, fh] = f.bbox;
    const cx = x + fw / 2, cy = y + fh / 2;
    // Qavslar yuzga yopishib turadi
    const bw = fw * FOCUS_PAD * s, bh = fh * FOCUS_PAD * s;
    const sx = ox + (h.mirrored ? h.width - cx : cx) * s;
    return {
      key: i,
      kind,
      style: { left: sx - bw / 2 + "px", top: oy + cy * s - bh / 2 + "px", width: bw + "px", height: bh + "px" },
    };
  });
});
</script>

<template>
  <section class="page live">
    <template v-if="!cameraOk">
      <EngineBanner />
      <CameraNotice :status="cam.status.value" :error="cam.error.value" @retry="cam.start()" @settings="cam.openSettings()" />
    </template>

    <!-- To'liq ekran: kamera pastki navigatsiyagacha; yuz burchak qavslar bilan, natija pastda -->
    <div v-else ref="fsEl" class="fs">
      <img
        v-if="cam.frameUrl.value"
        :src="cam.frameUrl.value"
        :class="{ mirrored: cam.header.value && cam.header.value.mirrored }"
        alt=""
        @load="onShown"
      />
      <div v-for="f in focus" :key="f.key" class="focus" :class="f.kind" :style="f.style" aria-hidden="true">
        <div class="ring"><i class="c tl" /><i class="c tr" /><i class="c bl" /><i class="c br" /></div>
      </div>

      <div class="top">
        <div class="banner"><EngineBanner /></div>
        <button
          type="button"
          class="flip"
          :aria-label="cam.facing.value === 'front' ? $t('camera.toBack') : $t('camera.toFront')"
          @click="cam.switchFacing()"
        >
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path d="M4 7h11l-3-3M20 17H9l3 3" />
          </svg>
        </button>
      </div>

      <div class="bottom">
        <div v-if="result" class="result" :class="result.kind" role="status" aria-live="polite">
          <p class="main">{{ result.text }}</p>
          <p v-if="result.sub" class="sub">{{ result.sub }}</p>
        </div>
        <p v-if="cam.header.value" class="stats">
          {{ cam.header.value.fps }} fps · detect {{ cam.header.value.detectMs.toFixed(0) }} ms ·
          {{ $t("camera.latency") }} {{ cam.header.value.latencyMs.toFixed(0) }} ms ·
          {{ cam.header.value.width }}×{{ cam.header.value.height }}
          <template v-if="cam.info.value"> · Camera2 {{ cam.info.value.hardwareLevel.toUpperCase() }}</template>
        </p>
      </div>
    </div>
  </section>
</template>

<style scoped>
/* Pastki navigatsiya (z-index 10) ochiq qoladi: Live — bosh sahifa, boshqa bo'limlarga o'tish kerak */
.fs {
  position: fixed; left: 0; top: 0; right: 0; z-index: 5;
  bottom: 56px; bottom: calc(var(--tabbar-h) + var(--safe-bottom));
  overflow: hidden; background: #000; color: #fff;
}
.fs img { position: absolute; left: 0; top: 0; width: 100%; height: 100%; object-fit: cover; }
.fs img.mirrored { -webkit-transform: scaleX(-1); transform: scaleX(-1); }

/* Face ID uslubidagi fokus: faqat burchaklar, yuz ortidan silliq ergashadi */
.focus {
  position: absolute; pointer-events: none;
  -webkit-transition: left .12s linear, top .12s linear, width .12s linear, height .12s linear;
  transition: left .12s linear, top .12s linear, width .12s linear, height .12s linear;
}
/* Ichki qatlam: tashqi div joylashuv bilan, bu esa animatsiya bilan band (transform to'qnashmaydi) */
.focus .ring { position: absolute; left: 0; top: 0; width: 100%; height: 100%; }
.focus.pending .ring { -webkit-animation: breathe 1.1s ease-in-out infinite; animation: breathe 1.1s ease-in-out infinite; }
@-webkit-keyframes breathe {
  0%, 100% { -webkit-transform: scale(1); opacity: 1; }
  50% { -webkit-transform: scale(.9); opacity: .55; }
}
@keyframes breathe {
  0%, 100% { transform: scale(1); opacity: 1; }
  50% { transform: scale(.9); opacity: .55; }
}
.focus .c { position: absolute; width: 22%; height: 22%; border: 0 solid #fff; -webkit-transition: border-color .25s; transition: border-color .25s; }
.focus .tl { left: 0; top: 0; border-left-width: 4px; border-top-width: 4px; border-top-left-radius: 18px; }
.focus .tr { right: 0; top: 0; border-right-width: 4px; border-top-width: 4px; border-top-right-radius: 18px; }
.focus .bl { left: 0; bottom: 0; border-left-width: 4px; border-bottom-width: 4px; border-bottom-left-radius: 18px; }
.focus .br { right: 0; bottom: 0; border-right-width: 4px; border-bottom-width: 4px; border-bottom-right-radius: 18px; }
.focus.known .c { border-color: #32d583; }
.focus.unknown .c { border-color: #f97066; }
.focus.warn .c { border-color: #fdb022; }

.top { position: absolute; left: 0; right: 0; top: 0; padding: 12px; display: -webkit-box; display: flex; }
.banner { -webkit-box-flex: 1; flex: 1 1 auto; min-width: 0; margin-right: 8px; color: var(--text); }
.flip {
  -webkit-box-flex: 0; flex: 0 0 auto; width: 48px; height: 48px; padding: 12px;
  border: 0; border-radius: 50%; background: rgba(0, 0, 0, .4); cursor: pointer;
}
.flip svg { display: block; width: 24px; height: 24px; fill: none; stroke: #fff; stroke-width: 2; stroke-linecap: round; stroke-linejoin: round; }

.bottom {
  position: absolute; left: 0; right: 0; bottom: 0; padding: 32px 16px 12px;
  background: -webkit-linear-gradient(top, rgba(0, 0, 0, 0), rgba(0, 0, 0, .65));
  background: linear-gradient(to bottom, rgba(0, 0, 0, 0), rgba(0, 0, 0, .65));
}
/* Fonsiz; tepa va pastda markazdan chetlarga xiralashadigan chiziq */
.result { position: relative; padding: 12px 16px; text-align: center; text-shadow: 0 1px 4px rgba(0, 0, 0, .7); }
.result:before, .result:after {
  content: ""; position: absolute; left: 0; right: 0; height: 2px;
  background: -webkit-linear-gradient(left, rgba(255, 255, 255, 0), rgba(255, 255, 255, .9), rgba(255, 255, 255, 0));
  background: linear-gradient(to right, rgba(255, 255, 255, 0), rgba(255, 255, 255, .9), rgba(255, 255, 255, 0));
}
.result:before { top: 0; }
.result:after { bottom: 0; }
.result.known:before, .result.known:after {
  background: -webkit-linear-gradient(left, rgba(50, 213, 131, 0), #32d583, rgba(50, 213, 131, 0));
  background: linear-gradient(to right, rgba(50, 213, 131, 0), #32d583, rgba(50, 213, 131, 0));
}
.result.unknown:before, .result.unknown:after {
  background: -webkit-linear-gradient(left, rgba(249, 112, 102, 0), #f97066, rgba(249, 112, 102, 0));
  background: linear-gradient(to right, rgba(249, 112, 102, 0), #f97066, rgba(249, 112, 102, 0));
}
.result.warn:before, .result.warn:after {
  background: -webkit-linear-gradient(left, rgba(253, 176, 34, 0), #fdb022, rgba(253, 176, 34, 0));
  background: linear-gradient(to right, rgba(253, 176, 34, 0), #fdb022, rgba(253, 176, 34, 0));
}
.main { margin: 0; font-size: 22px; font-weight: 600; line-height: 1.25; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.sub { margin: 4px 0 0; font-size: 14px; opacity: .85; }
.stats { margin: 8px 0 0; font-size: 11px; color: rgba(255, 255, 255, .55); text-align: center; }

/* Desktop: chapda yon menyu (200px) */
@media (min-width: 768px) {
  .fs { left: 200px; bottom: 0; }
}
</style>
