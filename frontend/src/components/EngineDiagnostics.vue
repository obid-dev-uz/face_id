<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { engineApi, readFile, type Analysis, type Benchmark, type EngineStatus } from "../api";
import { isTauri } from "../platform";

const status = ref<EngineStatus | null>(null);
const busy = ref<"" | "analyze" | "bench">("");
const error = ref<string | null>(null);
const analysis = ref<Analysis | null>(null);
const bench = ref<Benchmark | null>(null);
const preview = ref<string | null>(null);
let lastBytes: Uint8Array | null = null;
let unlisten: UnlistenFn | null = null;

async function refresh() {
  if (isTauri) status.value = await engineApi.status();
}

onMounted(async () => {
  await refresh();
  if (isTauri) unlisten = await listen("engine-status", refresh);
});
onUnmounted(() => {
  if (unlisten) unlisten();
  if (preview.value) URL.revokeObjectURL(preview.value);
});

async function run<T>(kind: "analyze" | "bench", fn: () => Promise<T>): Promise<T | null> {
  busy.value = kind;
  error.value = null;
  try {
    return await fn();
  } catch (e) {
    error.value = String(e);
    return null;
  } finally {
    busy.value = "";
  }
}

async function onFile(ev: Event) {
  const input = ev.target as HTMLInputElement;
  const file = input.files && input.files[0];
  input.value = "";
  if (!file) return;
  if (preview.value) URL.revokeObjectURL(preview.value);
  preview.value = URL.createObjectURL(file);
  analysis.value = null;
  lastBytes = await readFile(file);
  const bytes = lastBytes;
  analysis.value = await run("analyze", () => engineApi.analyzeImage(bytes));
}

async function runBenchmark() {
  bench.value = await run("bench", () => engineApi.benchmark(lastBytes, 10));
}

const pct = (v: number, total: number) => (v / total) * 100 + "%";
const f1 = (v: number) => v.toFixed(1);
</script>

<template>
  <div>
    <p v-if="!status" class="muted">{{ $t("settings.loading") }}</p>
    <p v-else-if="status.state === 'loading'" class="muted">{{ $t("diag.engineLoading") }}</p>
    <p v-else-if="status.state === 'failed'" class="error wrap">{{ status.error }}</p>
    <p v-else class="muted">{{ $t("diag.engineReady", { model: status.modelId, n: status.samples }) }}</p>

    <label class="btn file-btn" :class="{ disabled: !status || status.state !== 'ready' || busy }">
      {{ busy === "analyze" ? $t("diag.working") : $t("diag.pickImage") }}
      <input type="file" accept="image/*" :disabled="!status || status.state !== 'ready' || !!busy" @change="onFile" />
    </label>
    <button type="button" class="btn spaced" :disabled="!status || status.state !== 'ready' || !!busy" @click="runBenchmark">
      {{ busy === "bench" ? $t("diag.working") : $t("diag.benchmark") }}
    </button>
    <p v-if="error" class="error wrap">{{ error }}</p>

    <div v-if="preview && analysis" class="preview">
      <img :src="preview" alt="" />
      <div
        v-for="(f, i) in analysis.faces"
        :key="i"
        class="box"
        :class="{ known: f.personId !== null }"
        :style="{
          left: pct(f.face.bbox[0], analysis.width),
          top: pct(f.face.bbox[1], analysis.height),
          width: pct(f.face.bbox[2], analysis.width),
          height: pct(f.face.bbox[3], analysis.height),
        }"
      >
        <span class="box-label">{{ f.personName || $t("diag.unknown") }}</span>
      </div>
    </div>

    <dl v-if="analysis" class="kv">
      <dt>{{ $t("diag.faces") }}</dt>
      <dd>{{ analysis.faces.length }} · {{ analysis.width }}×{{ analysis.height }}</dd>
      <template v-for="(f, i) in analysis.faces" :key="'f' + i">
        <dt>#{{ i + 1 }}</dt>
        <dd>
          {{ $t("diag.detScore") }} {{ f.face.score.toFixed(2) }} ·
          {{ $t("diag.similarity") }} {{ f.score === null ? "—" : f.score.toFixed(3) }} ·
          {{ Math.round(f.quality.faceWidth) }}px · {{ $t("diag.sharpness") }} {{ Math.round(f.quality.sharpness) }} ·
          yaw {{ Math.round(f.quality.yawDeg) }}°
          <span v-if="f.quality.issues.length" class="error">
            — <template v-for="(q, j) in f.quality.issues" :key="q">{{ j ? ", " : "" }}{{ $t("quality." + q) }}</template>
          </span>
        </dd>
      </template>
      <dt>{{ $t("diag.timings") }}</dt>
      <dd>
        decode {{ f1(analysis.decodeMs) }} · detect {{ f1(analysis.timings.detectMs) }} · align {{ f1(analysis.timings.alignMs) }} ·
        embed {{ f1(analysis.timings.embedMs) }} · match {{ f1(analysis.matchMs) }} ms
      </dd>
    </dl>

    <dl v-if="bench" class="kv">
      <dt>{{ $t("diag.benchmark") }} · {{ bench.arch }} · ×{{ bench.iterations }}</dt>
      <dd>{{ $t("diag.benchNote", { n: bench.facesFound }) }}</dd>
      <template v-for="k in ['detect640', 'detect320', 'detect160', 'embed'] as const" :key="k">
        <dt>{{ k }} · {{ bench.single.threads }} / {{ bench.multi.threads }} {{ $t("diag.threads") }}</dt>
        <dd>{{ f1(bench.single[k].avgMs) }} ms / {{ f1(bench.multi[k].avgMs) }} ms</dd>
      </template>
      <template v-for="k in ['align', 'match10k'] as const" :key="k">
        <dt>{{ k }}</dt>
        <dd>{{ f1(bench[k].avgMs) }} ms ({{ f1(bench[k].minMs) }}–{{ f1(bench[k].maxMs) }})</dd>
      </template>
    </dl>
  </div>
</template>

<style scoped>
.file-btn { position: relative; overflow: hidden; text-align: center; line-height: 22px; }
.file-btn input { position: absolute; left: 0; top: 0; width: 100%; height: 100%; opacity: 0; cursor: pointer; }
.file-btn.disabled { opacity: .5; }
.spaced { margin-top: 8px; }
.preview { position: relative; margin: 12px 0; line-height: 0; }
.preview img { width: 100%; height: auto; border-radius: 8px; }
.box { position: absolute; border: 2px solid #f79009; border-radius: 4px; }
.box.known { border-color: #12b76a; }
.box-label {
  position: absolute; left: -2px; bottom: 100%; padding: 2px 6px; line-height: 16px;
  font-size: 12px; color: #fff; background: #f79009; border-radius: 4px 4px 0 0; white-space: nowrap;
}
.box.known .box-label { background: #12b76a; }
.kv { margin-top: 12px; }
</style>
