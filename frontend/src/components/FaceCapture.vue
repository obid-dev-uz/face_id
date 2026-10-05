<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { useCamera, type EnrollFeedback, type FrameHeader } from "../composables/useCamera";
import { readFile } from "../api";

const props = defineProps<{ required: number }>();
const emit = defineEmits<{ (e: "captured"): void; (e: "addImage", bytes: Uint8Array): void; (e: "skip"): void }>();

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
      <div v-if="flash" class="flash" />
    </div></div>

    <div class="progress">
      <span v-for="i in props.required" :key="i" class="dot" :class="{ done: i <= count }" />
    </div>

    <p class="instruction">
      <template v-if="cam.status.value === 'denied'">{{ $t("camera.deniedTitle") }}</template>
      <template v-else-if="!feedback">{{ $t("enroll.lookAtCamera") }}</template>
      <template v-else-if="feedback.target === null">{{ $t("enroll.allCaptured") }}</template>
      <template v-else-if="feedback.hint && feedback.hint !== 'turnPose'">{{ $t("enroll.hint." + feedback.hint) }}</template>
      <template v-else>{{ $t("enroll.pose." + feedback.target) }}</template>
    </p>
    <p v-if="feedback && feedback.yawDeg !== null" class="debug muted">
      yaw {{ Math.round(feedback.yawDeg) }}° · pitch {{ Math.round(feedback.pitchDeg || 0) }}° ·
      {{ $t("diag.sharpness") }} {{ Math.round(feedback.sharpness || 0) }}
    </p>

    <div class="actions">
      <button v-if="feedback && feedback.target && feedback.target !== 'frontal'" type="button" class="btn" @click="emit('skip')">
        {{ $t("enroll.skipPose") }}
      </button>
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
