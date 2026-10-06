//! Ilova darajasidagi bog'lovchi: modellarni resurslardan o'qish, FaceEngine'ni
//! fonda yuklash va Matcher'ni bazadan to'ldirish.

use std::sync::Arc;
use std::time::Instant;

use tauri::{path::BaseDirectory, AppHandle, Emitter, Manager};
use tauri_plugin_fs::FsExt;

use crate::db::repo;
use crate::face::{matcher::Matcher, FaceEngine};
use crate::state::{AppState, EngineState};

pub const YUNET: &str = "resources/models/face_detection_yunet_2023mar.onnx";
pub const SFACE: &str = "resources/models/face_recognition_sface_2021dec.onnx";
/// Rasm tahlili (diagnostika) uchun detektor kirishi. Live rejim kichikroq (160) o'lchamdan foydalanadi.
pub const PHOTO_DET: (usize, usize) = (640, 640);
/// Kam xotirali qurilmada (≤ LOW_RAM_MB): tract oraliq tensorlari kirish maydoniga mutanosib,
/// 320×320 ularni ~4 barobar kamaytiradi.
pub const PHOTO_DET_LOW_RAM: (usize, usize) = (320, 320);
pub const LOW_RAM_MB: u64 = 2048;

/// Qurilmadagi umumiy RAM (MB), `/proc/meminfo` dan (Android/Linux). Boshqa joyda — `None`.
pub fn total_ram_mb() -> Option<u64> {
    let text = std::fs::read_to_string("/proc/meminfo").ok()?;
    parse_mem_total_mb(&text)
}

fn parse_mem_total_mb(meminfo: &str) -> Option<u64> {
    let line = meminfo.lines().find(|l| l.starts_with("MemTotal:"))?;
    let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb / 1024)
}

pub fn is_low_ram() -> bool {
    total_ram_mb().is_some_and(|mb| mb <= LOW_RAM_MB)
}

/// Android'da resurslar APK ichida (assets) — `std::fs` ularni o'qiy olmaydi,
/// fs plagini esa barcha platformalarda ishlaydi.
pub fn read_resource(app: &AppHandle, rel: &str) -> anyhow::Result<Vec<u8>> {
    let path = app.path().resolve(rel, BaseDirectory::Resource)?;
    app.fs().read(&path).map_err(|e| anyhow::anyhow!("{rel}: {e}"))
}

pub fn spawn_loader(app: AppHandle) {
    std::thread::Builder::new()
        .name("khoface-engine".into())
        .spawn(move || {
            let t0 = Instant::now();
            let state = app.state::<AppState>();
            let result = load(&app);
            match result {
                Ok((engine, matcher)) => {
                    log::info!("FaceEngine {:?} da yuklandi, {} namuna", t0.elapsed(), matcher.len());
                    *state.matcher.write() = matcher;
                    *state.engine.write() = EngineState::Ready(engine);
                }
                Err(e) => {
                    log::error!("FaceEngine yuklanmadi: {e:#}");
                    *state.engine.write() = EngineState::Failed(format!("{e:#}"));
                }
            }
            let _ = app.emit("engine-status", ());
        })
        .expect("engine thread");
}

fn load(app: &AppHandle) -> anyhow::Result<(Arc<FaceEngine>, Matcher)> {
    crate::face::backend::set_threads(crate::face::backend::default_threads());
    let yunet = read_resource(app, YUNET)?;
    let sface = read_resource(app, SFACE)?;
    let (det_w, det_h) = if is_low_ram() { PHOTO_DET_LOW_RAM } else { PHOTO_DET };
    log::info!("RAM: {:?} MB, rasm detektori {det_w}×{det_h}", total_ram_mb());
    let engine = FaceEngine::load(&yunet, &sface, det_w, det_h)?;
    let state = app.state::<AppState>();
    let mut conn = state.db.lock();
    // Model yangilangan bo'lsa, eski namunalarni saqlangan tekislangan rasmdan qayta hisoblash.
    let report = repo::reembed_outdated(&mut conn, FaceEngine::MODEL_ID, |thumb| {
        let img = image::load_from_memory(thumb)?.into_rgb8();
        Ok(engine.embedder.embed(&img)?)
    })?;
    if report.updated + report.skipped > 0 {
        log::info!("qayta indekslash: {report:?}");
    }
    let rows = repo::load_embeddings(&conn, FaceEngine::MODEL_ID)?;
    drop(conn);
    Ok((Arc::new(engine), Matcher::from_embeddings(FaceEngine::DIM, rows)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mem_total() {
        let text = "MemTotal:        2893932 kB\nMemFree:          114388 kB\n";
        assert_eq!(parse_mem_total_mb(text), Some(2826));
        assert_eq!(parse_mem_total_mb("MemFree: 1 kB"), None);
    }
}
