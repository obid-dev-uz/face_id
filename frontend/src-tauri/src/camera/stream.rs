//! Live oqim: FrameBus'dan kadr olib, yuzlarni aniqlaydi va frontend'ga JPEG + ramkalarni
//! bitta binary xabarda yuboradi: `[u32 LE sarlavha uzunligi][JSON sarlavha][JPEG]`.

use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use image::codecs::jpeg::JpegEncoder;
use parking_lot::Mutex;
use serde::Serialize;
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Manager};

use super::{Facing, FrameBus};
use crate::face::Face;
use crate::state::AppState;

const JPEG_QUALITY: u8 = 70;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Header<'a> {
    width: u32,
    height: u32,
    /// Old kamera: UI'da ko'zgu kabi ko'rsatiladi (ramkalar ham).
    mirrored: bool,
    faces: &'a [Face],
    /// Ro'yxatga olish rejimida: namuna olish holati.
    #[serde(skip_serializing_if = "Option::is_none")]
    enroll: Option<crate::enroll::Feedback>,
    fps: f32,
    detect_ms: f32,
    encode_ms: f32,
    /// Kamera kadri olinganidan yuborilgunicha o'tgan vaqt.
    latency_ms: f32,
}

#[derive(Default)]
pub struct Stream {
    pub bus: FrameBus,
    running: Arc<AtomicBool>,
    /// Frontend oldingi kadrni chizib bo'ldi — keyingisini yuborish mumkin.
    ready: Arc<AtomicBool>,
    /// Kadrlar ro'yxatga olish sessiyasiga ham beriladi.
    enroll_mode: Arc<AtomicBool>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl Stream {
    pub fn ack(&self) {
        self.ready.store(true, Ordering::Release);
    }

    pub fn start(&self, app: AppHandle, channel: Channel<InvokeResponseBody>, enroll: bool) {
        self.stop_worker();
        self.bus.clear();
        self.running.store(true, Ordering::Release);
        self.ready.store(true, Ordering::Release);
        self.enroll_mode.store(enroll, Ordering::Release);
        let (running, ready, bus, enroll) = (self.running.clone(), self.ready.clone(), self.bus.clone(), self.enroll_mode.clone());
        let handle = std::thread::Builder::new()
            .name("khoface-stream".into())
            .spawn(move || run(app, channel, bus, running, ready, enroll))
            .expect("stream thread");
        *self.worker.lock() = Some(handle);
    }

    pub fn stop_worker(&self) {
        self.running.store(false, Ordering::Release);
        if let Some(h) = self.worker.lock().take() {
            let _ = h.join();
        }
    }
}

fn run(
    app: AppHandle,
    channel: Channel<InvokeResponseBody>,
    bus: FrameBus,
    running: Arc<AtomicBool>,
    ready: Arc<AtomicBool>,
    enroll_mode: Arc<AtomicBool>,
) {
    let state = app.state::<AppState>();
    let mut fps = Fps::default();
    while running.load(Ordering::Acquire) {
        let Some(frame) = bus.wait_take(Duration::from_millis(200)) else { continue };

        // Modellar hali yuklanmagan bo'lsa ham kamera tasviri ko'rsatiladi.
        let t0 = Instant::now();
        let engine = state.engine().ok();
        let faces = match &engine {
            Some(engine) => engine.live_detector.detect(&frame.rgb).unwrap_or_default(),
            None => Vec::new(),
        };
        let detect_ms = ms(t0);
        let enroll = match (&engine, enroll_mode.load(Ordering::Acquire)) {
            (Some(engine), true) => state.enroll.lock().as_mut().map(|s| s.process_frame(engine, &frame.rgb, &faces)),
            _ => None,
        };
        fps.tick();

        // Namuna olingan kadr albatta ko'rsatiladi (frontend shu xabardan biladi).
        let captured = enroll.as_ref().is_some_and(|e| e.captured);
        if !ready.swap(false, Ordering::AcqRel) && !captured {
            continue; // frontend band — bu kadrni ko'rsatmaymiz, lekin aniqlash davom etadi
        }
        let t0 = Instant::now();
        let mut jpeg = Vec::with_capacity(64 * 1024);
        if JpegEncoder::new_with_quality(&mut Cursor::new(&mut jpeg), JPEG_QUALITY).encode_image(&frame.rgb).is_err() {
            ready.store(true, Ordering::Release);
            continue;
        }
        let header = Header {
            width: frame.rgb.width(),
            height: frame.rgb.height(),
            mirrored: frame.facing == Facing::Front,
            faces: &faces,
            enroll,
            fps: fps.value(),
            detect_ms,
            encode_ms: ms(t0),
            latency_ms: ms(frame.at),
        };
        let json = serde_json::to_vec(&header).unwrap_or_default();
        let mut msg = Vec::with_capacity(4 + json.len() + jpeg.len());
        msg.extend_from_slice(&(json.len() as u32).to_le_bytes());
        msg.extend_from_slice(&json);
        msg.extend_from_slice(&jpeg);
        if channel.send(InvokeResponseBody::Raw(msg)).is_err() {
            break; // sahifa yopilgan
        }
    }
}

fn ms(t0: Instant) -> f32 {
    t0.elapsed().as_secs_f32() * 1000.0
}

/// So'nggi ~1 soniyadagi kadrlar soni.
#[derive(Default)]
struct Fps {
    times: std::collections::VecDeque<Instant>,
}

impl Fps {
    fn tick(&mut self) {
        let now = Instant::now();
        self.times.push_back(now);
        while self.times.front().is_some_and(|t| now.duration_since(*t) > Duration::from_secs(1)) {
            self.times.pop_front();
        }
    }

    fn value(&self) -> f32 {
        self.times.len() as f32
    }
}
