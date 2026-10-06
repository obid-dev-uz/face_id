import { onBeforeUnmount, ref, shallowRef } from "vue";
import { Channel, invoke } from "@tauri-apps/api/core";
import { isTauri } from "../platform";

export type Facing = "front" | "back";
export type PermissionState = "granted" | "denied" | "prompt" | "prompt-with-rationale";
export type CameraStatus = "idle" | "starting" | "running" | "denied" | "noCamera" | "unsupported" | "error";

export interface LiveFace {
  bbox: [number, number, number, number];
  score: number;
  landmarks: [number, number][];
}

export interface TrackView {
  id: number;
  bbox: [number, number, number, number];
  status: "pending" | "known" | "unknown";
  personId: number | null;
  name: string | null;
  score: number | null;
}

export type Pose =
  | "frontal" | "left" | "upLeft" | "up" | "upRight" | "right" | "downRight" | "down" | "downLeft";
export type EnrollHint =
  | "noFace" | "multipleFaces" | "notCentered" | "tooFar" | "tooClose" | "turnPose"
  | "blurry" | "lowScore" | "tooDark" | "tooBright" | "differentPerson" | "hold";

export interface EnrollFeedback {
  count: number;
  target: Pose | null;
  hint: EnrollHint | null;
  /** Halqalardagi olingan yo'nalishlar: [ichki, tashqi]. */
  covered: Pose[][];
  captured: boolean;
  yawDeg: number | null;
  pitchDeg: number | null;
  sharpness: number | null;
}

export interface FrameHeader {
  width: number;
  height: number;
  mirrored: boolean;
  faces: LiveFace[];
  /** Live rejimda tanish holati (bo'lmasa — faqat aniqlash ramkalari). */
  tracks?: TrackView[];
  enroll?: EnrollFeedback;
  fps: number;
  detectMs: number;
  encodeMs: number;
  latencyMs: number;
}

/** Rust `camera/stream.rs` formati: [u32 LE sarlavha uzunligi][JSON][JPEG]. */
export function parseFrame(raw: ArrayBuffer | number[]): { header: FrameHeader; jpeg: Uint8Array<ArrayBuffer> } {
  // Kichik xabarlar Tauri'da raqamlar massivi bo'lib kelishi mumkin.
  const buf: ArrayBuffer = raw instanceof ArrayBuffer ? raw : (new Uint8Array(raw).buffer as ArrayBuffer);
  const len = new DataView(buf).getUint32(0, true);
  const header = JSON.parse(new TextDecoder().decode(new Uint8Array(buf, 4, len))) as FrameHeader;
  return { header, jpeg: new Uint8Array(buf, 4 + len) };
}

export function useCamera(options: { enroll?: boolean; onFrame?: (h: FrameHeader) => void } = {}) {
  const status = ref<CameraStatus>("idle");
  const error = ref<string | null>(null);
  const facing = ref<Facing>("front");
  const frameUrl = ref<string | null>(null);
  const header = shallowRef<FrameHeader | null>(null);
  let shownUrl: string | null = null;

  function onFrame(raw: ArrayBuffer | number[]) {
    const f = parseFrame(raw);
    header.value = f.header;
    if (options.onFrame) options.onFrame(f.header);
    frameUrl.value = URL.createObjectURL(new Blob([f.jpeg], { type: "image/jpeg" }));
  }

  /** <img @load> dan chaqiriladi: oldingi URL bo'shatiladi va Rust'ga keyingi kadr so'raladi. */
  function frameShown() {
    if (shownUrl && shownUrl !== frameUrl.value) URL.revokeObjectURL(shownUrl);
    shownUrl = frameUrl.value;
    if (status.value === "running") invoke("camera_ack").catch(() => {});
  }

  async function ensurePermission(): Promise<boolean> {
    let p = await invoke<PermissionState>("camera_permission");
    if (p !== "granted") p = await invoke<PermissionState>("request_camera_permission");
    return p === "granted";
  }

  async function start(which: Facing = facing.value) {
    if (!isTauri) {
      status.value = "unsupported";
      return;
    }
    facing.value = which;
    status.value = "starting";
    error.value = null;
    try {
      if (!(await ensurePermission())) {
        status.value = "denied";
        return;
      }
      const channel = new Channel<ArrayBuffer>();
      channel.onmessage = onFrame;
      await invoke("start_stream", { facing: which, enroll: !!options.enroll, onFrame: channel });
      status.value = "running";
    } catch (e) {
      const msg = String(e);
      status.value = msg.indexOf("topilmadi") >= 0 ? "noCamera" : "error";
      error.value = msg;
    }
  }

  async function stop() {
    if (status.value === "running" || status.value === "starting") {
      status.value = "idle";
      await invoke("stop_stream").catch(() => {});
    }
  }

  async function switchFacing() {
    await stop();
    await start(facing.value === "front" ? "back" : "front");
  }

  const openSettings = () => invoke("open_app_settings");

  onBeforeUnmount(() => {
    stop();
    if (shownUrl) URL.revokeObjectURL(shownUrl);
  });

  return { status, error, facing, frameUrl, header, frameShown, start, stop, switchFacing, openSettings };
}
