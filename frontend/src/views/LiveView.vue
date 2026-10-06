<script setup lang="ts">
import { computed, onMounted } from "vue";
import { useI18n } from "vue-i18n";
import { useCamera } from "../composables/useCamera";
import CameraNotice from "../components/CameraNotice.vue";
import EngineBanner from "../components/EngineBanner.vue";

const { t } = useI18n();

const cam = useCamera();
onMounted(() => cam.start());

/** Old kamerada tasvir ko'zgu kabi ko'rsatiladi, ramkalar ham shunga moslanadi. */
const boxes = computed(() => {
  const h = cam.header.value;
  if (!h) return [];
  const place = (bbox: [number, number, number, number]) => {
    const [x, y, w, bh] = bbox;
    const left = h.mirrored ? h.width - x - w : x;
    return {
      left: (left / h.width) * 100 + "%",
      top: (y / h.height) * 100 + "%",
      width: (w / h.width) * 100 + "%",
      height: (bh / h.height) * 100 + "%",
    };
  };
  // Modellar hali yuklanmagan bo'lsa treklar yo'q — faqat aniqlash ramkalari.
  if (!h.tracks) return h.faces.map((f, i) => ({ key: "f" + i, style: place(f.bbox), status: "pending", label: "" }));
  return h.tracks.map((tr) => ({
    key: "t" + tr.id,
    style: place(tr.bbox),
    status: tr.status,
    label:
      tr.status === "known"
        ? (tr.name || "#" + tr.personId) + (tr.score !== null ? " · " + tr.score.toFixed(2) : "")
        : t(tr.status === "unknown" ? "live.unknown" : "live.pending"),
  }));
});
</script>

<template>
  <section class="page live">
    <EngineBanner />
    <CameraNotice :status="cam.status.value" :error="cam.error.value" @retry="cam.start()" @settings="cam.openSettings()" />
    <template v-if="['idle', 'starting', 'running'].indexOf(cam.status.value) >= 0">
      <div class="viewer">
        <img
          v-if="cam.frameUrl.value"
          :src="cam.frameUrl.value"
          :class="{ mirrored: cam.header.value && cam.header.value.mirrored }"
          alt=""
          @load="cam.frameShown"
        />
        <div v-else class="placeholder muted">{{ $t("camera.starting") }}</div>
        <div v-for="b in boxes" :key="b.key" class="box" :class="b.status" :style="b.style">
          <span v-if="b.label" class="label">{{ b.label }}</span>
        </div>
      </div>

      <div class="controls">
        <button type="button" class="btn" @click="cam.switchFacing()">
          {{ cam.facing.value === "front" ? $t("camera.toBack") : $t("camera.toFront") }}
        </button>
      </div>
      <p v-if="cam.header.value" class="stats muted">
        {{ cam.header.value.fps }} fps · detect {{ cam.header.value.detectMs.toFixed(0) }} ms ·
        jpeg {{ cam.header.value.encodeMs.toFixed(0) }} ms · {{ $t("camera.latency") }}
        {{ cam.header.value.latencyMs.toFixed(0) }} ms · {{ cam.header.value.width }}×{{ cam.header.value.height }}
        <template v-if="cam.info.value"> · Camera2 {{ cam.info.value.hardwareLevel.toUpperCase() }}</template>
      </p>
    </template>
  </section>
</template>

<style scoped>
.viewer { position: relative; line-height: 0; background: #000; border-radius: 12px; overflow: hidden; min-height: 200px; }
.viewer img { width: 100%; height: auto; }
.viewer img.mirrored { -webkit-transform: scaleX(-1); transform: scaleX(-1); }
.placeholder { line-height: 1.4; padding: 80px 16px; text-align: center; color: #98a2b3; }
.box { position: absolute; border: 2px solid #f79009; border-radius: 6px; }
.box.known { border-color: #12b76a; }
.box.unknown { border-color: #f04438; }
.label {
  position: absolute; left: -2px; bottom: 100%; max-width: 220px; margin-bottom: 2px; padding: 2px 6px;
  border-radius: 4px; background: #f79009; color: #fff; font-size: 12px; line-height: 16px;
  white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
}
.known .label { background: #12b76a; }
.unknown .label { background: #f04438; }
.controls { margin-top: 12px; }
.stats { font-size: 12px; margin-top: 8px; }
</style>
