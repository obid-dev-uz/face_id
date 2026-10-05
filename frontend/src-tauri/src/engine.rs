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
/// Galereya rasmlari uchun detektor kirishi. Live rejim (7-bosqich) kichikroq o'lchamdan foydalanadi.
pub const PHOTO_DET: (usize, usize) = (640, 640);

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
    let engine = FaceEngine::load(&yunet, &sface, PHOTO_DET.0, PHOTO_DET.1)?;
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
