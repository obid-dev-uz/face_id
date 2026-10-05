use std::io::Cursor;
use std::time::Instant;

use image::{DynamicImage, ImageDecoder, ImageReader, RgbImage};
use serde::Serialize;
use tauri::ipc::{InvokeBody, Request};
use tauri::{AppHandle, Manager, State};

use crate::db::repo;
use crate::error::{AppError, AppResult};
use crate::face::{backend, detector::Detector, matcher::Matcher, quality, Face, FaceEngine, Timings};
use crate::state::{blocking, AppState, EngineState};

/// Telefon kameralari 12+ MP rasm beradi; tahlil uchun shu o'lcham yetarli va ancha tez.
const MAX_SIDE: u32 = 1280;
pub const DEFAULT_THRESHOLD: f32 = 0.363;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    state: &'static str,
    error: Option<String>,
    model_id: &'static str,
    samples: usize,
}

#[tauri::command]
pub fn engine_status(state: State<'_, AppState>) -> EngineStatus {
    let samples = state.matcher.read().len();
    match &*state.engine.read() {
        EngineState::Loading => EngineStatus { state: "loading", error: None, model_id: FaceEngine::MODEL_ID, samples },
        EngineState::Failed(e) => EngineStatus { state: "failed", error: Some(e.clone()), model_id: FaceEngine::MODEL_ID, samples },
        EngineState::Ready(_) => EngineStatus { state: "ready", error: None, model_id: FaceEngine::MODEL_ID, samples },
    }
}

fn raw_body(request: &Request<'_>) -> AppResult<Vec<u8>> {
    match request.body() {
        InvokeBody::Raw(b) if !b.is_empty() => Ok(b.clone()),
        _ => Err(AppError::Invalid("rasm baytlari kutilgan edi".into())),
    }
}

/// JPEG/PNG'ni ochadi, EXIF bo'yicha buradi (telefon rasmlari) va kichraytiradi.
pub fn decode_image(bytes: &[u8]) -> AppResult<RgbImage> {
    let mut decoder = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?.into_decoder()?;
    let orientation = decoder.orientation()?;
    let mut img = DynamicImage::from_decoder(decoder)?;
    img.apply_orientation(orientation);
    if img.width().max(img.height()) > MAX_SIDE {
        img = img.resize(MAX_SIDE, MAX_SIDE, image::imageops::FilterType::Triangle);
    }
    Ok(img.into_rgb8())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundFace {
    face: Face,
    quality: quality::Quality,
    person_id: Option<i64>,
    person_name: Option<String>,
    score: Option<f32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Analysis {
    width: u32,
    height: u32,
    faces: Vec<FoundFace>,
    timings: Timings,
    decode_ms: f64,
    match_ms: f64,
}

/// Rasmdagi yuzlarni aniqlaydi va bazadagi shaxslar bilan solishtiradi.
/// Frontend: `invoke("analyze_image", uint8Array)`.
#[tauri::command]
pub async fn analyze_image(app: AppHandle, request: Request<'_>) -> AppResult<Analysis> {
    let bytes = raw_body(&request)?;
    let state = app.state::<AppState>();
    let engine = state.engine()?;
    let threshold = repo::get_setting(&state.db.lock(), "match.threshold")?
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_THRESHOLD);
    blocking(move || {
        let state = app.state::<AppState>();
        let t0 = Instant::now();
        let img = decode_image(&bytes)?;
        let decode_ms = ms(t0);
        let (found, timings) = engine.analyze(&img, 10)?;
        let t0 = Instant::now();
        let matches: Vec<_> = {
            let matcher = state.matcher.read();
            found.iter().map(|a| matcher.nearest(&a.embedding)).collect()
        };
        let match_ms = ms(t0);
        let conn = state.db.lock();
        let faces = found
            .into_iter()
            .zip(matches)
            .map(|(a, nearest)| {
                let known = nearest.filter(|m| m.score >= threshold);
                FoundFace {
                    quality: quality::assess(&a.face, &a.aligned, &Default::default()),
                    face: a.face,
                    person_id: known.map(|m| m.person_id),
                    // Matcher faqat id saqlaydi, ism bazadan olinadi.
                    person_name: known.and_then(|m| repo::get_person(&conn, m.person_id).ok()).map(|p| p.full_name),
                    score: nearest.map(|m| m.score),
                }
            })
            .collect();
        Ok(Analysis { width: img.width(), height: img.height(), faces, timings, decode_ms, match_ms })
    })
    .await
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Stat {
    avg_ms: f64,
    min_ms: f64,
    max_ms: f64,
}

impl Stat {
    fn of(samples: &[f64]) -> Self {
        if samples.is_empty() {
            return Self::default();
        }
        Stat {
            avg_ms: samples.iter().sum::<f64>() / samples.len() as f64,
            min_ms: samples.iter().copied().fold(f64::INFINITY, f64::min),
            max_ms: samples.iter().copied().fold(0.0, f64::max),
        }
    }
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct InferenceStats {
    threads: usize,
    detect_640: Stat,
    detect_320: Stat,
    detect_160: Stat,
    embed: Stat,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Benchmark {
    iterations: usize,
    faces_found: usize,
    single: InferenceStats,
    multi: InferenceStats,
    align: Stat,
    /// 10 000 ta sintetik namuna orasidan qidiruv.
    match_10k: Stat,
    arch: &'static str,
}

/// PoC-2: qurilmada har bir bosqich vaqtini 1 oqim va ko'p oqimda o'lchaydi.
/// Rasm berilmasa, kulrang sintetik kadr ishlatiladi.
#[tauri::command]
pub async fn benchmark_engine(app: AppHandle, request: Request<'_>) -> AppResult<Benchmark> {
    let bytes = match request.body() {
        InvokeBody::Raw(b) if !b.is_empty() => Some(b.clone()),
        _ => None,
    };
    let iterations = request
        .headers()
        .get("x-iterations")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .unwrap_or(10usize)
        .clamp(1, 100);
    let engine = app.state::<AppState>().engine()?;
    let yunet = crate::engine::read_resource(&app, crate::engine::YUNET)?;
    blocking(move || {
        let img = match bytes {
            Some(b) => decode_image(&b)?,
            None => RgbImage::from_pixel(640, 480, image::Rgb([128, 128, 128])),
        };
        let small = Detector::load(&yunet, 320, 256)?;
        let tiny = Detector::load(&yunet, 160, 128)?;
        let faces = engine.detector.detect(&img)?;
        let faces_found = faces.len();
        let landmarks = faces
            .first()
            .map(|f| f.landmarks)
            .unwrap_or_else(|| crate::face::align::REF_5PTS.map(|p| [p[0] * 2.0 + 200.0, p[1] * 2.0 + 120.0]));
        let (aligned, _) = crate::face::align::align(&img, &landmarks);

        let inference = |threads: usize| -> AppResult<InferenceStats> {
            backend::multithread_tract_scope(backend::executor(threads), || {
                let (mut d640, mut d320, mut d160, mut em) = (vec![], vec![], vec![], vec![]);
                for _ in 0..iterations {
                    let t0 = Instant::now();
                    engine.detector.detect(&img)?;
                    d640.push(ms(t0));
                    let t0 = Instant::now();
                    small.detect(&img)?;
                    d320.push(ms(t0));
                    let t0 = Instant::now();
                    tiny.detect(&img)?;
                    d160.push(ms(t0));
                    let t0 = Instant::now();
                    engine.embedder.embed(&aligned)?;
                    em.push(ms(t0));
                }
                Ok(InferenceStats { threads, detect_640: Stat::of(&d640), detect_320: Stat::of(&d320), detect_160: Stat::of(&d160), embed: Stat::of(&em) })
            })
        };
        let single = inference(1)?;
        let multi = inference(backend::default_threads())?;

        let mut matcher = Matcher::new(FaceEngine::DIM);
        let mut seed = 1u32;
        for id in 0..10_000 {
            let mut v: Vec<f32> = (0..FaceEngine::DIM)
                .map(|_| {
                    seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    (seed >> 8) as f32 / (1u32 << 24) as f32 - 0.5
                })
                .collect();
            crate::face::matcher::l2_normalize(&mut v);
            matcher.add(id, &v);
        }
        let query = engine.embedder.embed(&aligned)?;
        let (mut al, mut mt) = (vec![], vec![]);
        for _ in 0..iterations {
            let t0 = Instant::now();
            std::hint::black_box(crate::face::align::align(&img, &landmarks));
            al.push(ms(t0));
            let t0 = Instant::now();
            std::hint::black_box(matcher.nearest(&query));
            mt.push(ms(t0));
        }
        Ok(Benchmark {
            iterations,
            faces_found,
            single,
            multi,
            align: Stat::of(&al),
            match_10k: Stat::of(&mt),
            arch: std::env::consts::ARCH,
        })
    })
    .await
}

fn ms(t0: Instant) -> f64 {
    t0.elapsed().as_secs_f64() * 1000.0
}
