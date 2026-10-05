//! Desktop kamera: nokhwa (Windows Media Foundation, macOS AVFoundation, Linux V4L2).
//! Kamera o'z thread'ida ochiladi (ba'zi platformalarda `Camera` Send emas).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;
use std::time::Instant;

use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{ApiBackend, CameraFormat, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType, Resolution};
use nokhwa::Camera;
use parking_lot::Mutex;

use super::{Facing, Frame, FrameBus};
use crate::error::{AppError, AppResult};

struct Running {
    stop: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

static CURRENT: Mutex<Option<Running>> = Mutex::new(None);

fn cam_err(e: impl std::fmt::Display) -> AppError {
    AppError::Camera(e.to_string())
}

/// Foydalanish mumkin bo'lgan kameralar. Linux'da bitta kamera bir nechta /dev/video
/// tugunida ko'rinadi (metadata), shuning uchun nom bo'yicha takrorlar olib tashlanadi.
/// IR (Windows Hello) kameralari kulrang tasvir beradi — ular hisobga olinmaydi.
fn cameras() -> Vec<CameraIndex> {
    let Ok(mut list) = nokhwa::query(ApiBackend::Auto) else { return Vec::new() };
    list.sort_by_key(|c| c.index().as_index().unwrap_or(u32::MAX));
    let mut seen = Vec::new();
    let mut out = Vec::new();
    for c in list {
        let name = c.human_name();
        let lower = name.to_lowercase();
        if lower.contains("ir camera") || lower.contains("infrared") || seen.contains(&name) {
            continue;
        }
        seen.push(name);
        out.push(c.index().clone());
    }
    out
}

/// "front" — birinchi kamera (noutbukdagi), "back" — ikkinchisi (masalan USB).
fn index_for(facing: Facing) -> AppResult<CameraIndex> {
    let i = match facing {
        Facing::Front => 0,
        Facing::Back => 1,
    };
    cameras().into_iter().nth(i).ok_or_else(|| AppError::Camera("Kamera topilmadi".into()))
}

pub fn camera_count() -> usize {
    cameras().len()
}

pub fn start(bus: FrameBus, facing: Facing) -> AppResult<()> {
    stop();
    #[cfg(target_os = "macos")]
    nokhwa::nokhwa_initialize(|_| {});
    let index = index_for(facing)?;
    let stop_flag = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel::<AppResult<()>>();
    let flag = stop_flag.clone();
    let thread = std::thread::Builder::new()
        .name("khoface-camera".into())
        .spawn(move || capture(index, facing, bus, flag, tx))
        .map_err(cam_err)?;
    // Kamera ochilishi natijasini kutamiz, xato bo'lsa frontend'ga qaytariladi.
    match rx.recv() {
        Ok(Ok(())) => {
            *CURRENT.lock() = Some(Running { stop: stop_flag, thread });
            Ok(())
        }
        Ok(Err(e)) => {
            let _ = thread.join();
            Err(e)
        }
        Err(_) => Err(AppError::Camera("kamera thread'i to'xtadi".into())),
    }
}

pub fn stop() {
    if let Some(r) = CURRENT.lock().take() {
        r.stop.store(true, Ordering::Release);
        let _ = r.thread.join();
    }
}

fn capture(index: CameraIndex, facing: Facing, bus: FrameBus, stop: Arc<AtomicBool>, opened: mpsc::Sender<AppResult<()>>) {
    // 640×480 ga eng yaqin format; MJPEG ko'pchilik web-kameralarda yuqori FPS beradi.
    let wanted = CameraFormat::new(Resolution::new(640, 480), FrameFormat::MJPEG, 30);
    let format = RequestedFormat::new::<RgbFormat>(RequestedFormatType::Closest(wanted));
    let mut cam = match Camera::new(index, format).and_then(|mut c| c.open_stream().map(|_| c)) {
        Ok(c) => c,
        Err(e) => {
            let _ = opened.send(Err(cam_err(e)));
            return;
        }
    };
    let _ = opened.send(Ok(()));
    while !stop.load(Ordering::Acquire) {
        let rgb = match cam.frame().and_then(|b| b.decode_image::<RgbFormat>()) {
            Ok(img) => img,
            Err(e) => {
                log::warn!("kamera kadri o'qilmadi: {e}");
                std::thread::sleep(std::time::Duration::from_millis(50));
                continue;
            }
        };
        bus.put(Frame { rgb, facing, at: Instant::now() });
    }
    let _ = cam.stop_stream();
}
