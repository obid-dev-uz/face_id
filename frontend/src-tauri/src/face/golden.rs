//! Platformalar o'rtasida bir xillik (golden) testi: bir xil kirishda tekislash va embedding
//! har bir qurilmada (x86_64, arm64, armv7) kompyuterda yozib olingan natija bilan mos kelishi kerak.
//!
//! Golden fayl: `tests/fixtures/golden.json`. Qayta yozish (faqat model yoki konveyer ataylab
//! o'zgarganda): `KHOFACE_WRITE_GOLDEN=1 cargo test --lib golden`.
//! Telefonda: `scripts/android-test.sh`.
//!
//! `tests/fixtures/` dagi har bir `.jpg`/`.png` rasm uchun detektor natijasi ham solishtiriladi.

use std::collections::BTreeMap;
use std::path::PathBuf;

use image::RgbImage;
use serde::{Deserialize, Serialize};

use super::align::{align, REF_5PTS};
use super::tests::engine;
use super::FaceEngine;

/// Embedding o'xshashligi uchun chegara: tanish chegarasi (≈0.36) dan ancha baland.
const MIN_COS: f32 = 0.999;
/// Detektor koordinatalari uchun ruxsat etilgan farq (piksel).
const MAX_PX: f32 = 1.0;

#[derive(Serialize, Deserialize, Default)]
struct Golden {
    /// Sun'iy kirish: tekislangan kesmaning FNV-1a xeshi va SFace embedding'i.
    synthetic_aligned_fnv: String,
    synthetic_embedding: Vec<f32>,
    /// Rasm nomi → topilgan eng katta yuz.
    photos: BTreeMap<String, PhotoGolden>,
}

#[derive(Serialize, Deserialize)]
struct PhotoGolden {
    faces: usize,
    bbox: [f32; 4],
    landmarks: [[f32; 2]; 5],
    embedding: Vec<f32>,
}

fn fixtures_dir() -> PathBuf {
    std::env::var_os("KHOFACE_FIXTURES")
        .map(Into::into)
        .unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures"))
}

/// Butunsonli formula: har qanday platformada bit-darajada bir xil rasm.
fn synthetic() -> RgbImage {
    RgbImage::from_fn(320, 256, |x, y| {
        let r = (x * 7 + y * 3) % 256;
        let g = ((x ^ y) * 5) % 256;
        let b = ((x * x + y * y) / 97) % 256;
        image::Rgb([r as u8, g as u8, b as u8])
    })
}

/// Shablon nuqtalari: 1.7 marta kattalashtirilgan, biroz qiya, kadr markazida.
fn synthetic_landmarks() -> [[f32; 2]; 5] {
    REF_5PTS.map(|[x, y]| [70.0 + 1.7 * x - 0.2 * y, 30.0 + 0.2 * x + 1.7 * y])
}

fn fnv1a(bytes: &[u8]) -> String {
    let h = bytes.iter().fold(0xcbf29ce484222325u64, |h, &b| (h ^ b as u64).wrapping_mul(0x100000001b3));
    format!("{h:016x}")
}

fn cos(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len());
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn photos() -> Vec<(String, RgbImage)> {
    let Ok(dir) = std::fs::read_dir(fixtures_dir()) else { return Vec::new() };
    let mut out: Vec<_> = dir
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "jpg" || x == "jpeg" || x == "png"))
        .map(|p| {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            let img = image::open(&p).unwrap_or_else(|e| panic!("{name}: {e}")).to_rgb8();
            (name, img)
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn compute(e: &FaceEngine) -> Golden {
    let (aligned, _) = align(&synthetic(), &synthetic_landmarks());
    let mut g = Golden {
        synthetic_aligned_fnv: fnv1a(aligned.as_raw()),
        synthetic_embedding: e.embedder.embed(&aligned).unwrap(),
        photos: BTreeMap::new(),
    };
    for (name, img) in photos() {
        let faces = e.detector.detect(&img).unwrap();
        let face = faces
            .iter()
            .max_by(|a, b| (a.bbox[2] * a.bbox[3]).total_cmp(&(b.bbox[2] * b.bbox[3])))
            .unwrap_or_else(|| panic!("{name}: yuz topilmadi"));
        let (aligned, _) = align(&img, &face.landmarks);
        let embedding = e.embedder.embed(&aligned).unwrap();
        g.photos.insert(name, PhotoGolden { faces: faces.len(), bbox: face.bbox, landmarks: face.landmarks, embedding });
    }
    g
}

#[test]
fn same_results_on_every_platform() {
    let Some(e) = engine() else {
        eprintln!("modellar yo'q — test o'tkazib yuborildi");
        return;
    };
    let path = fixtures_dir().join("golden.json");
    let got = compute(&e);
    if std::env::var_os("KHOFACE_WRITE_GOLDEN").is_some() {
        std::fs::write(&path, serde_json::to_string_pretty(&got).unwrap()).unwrap();
        eprintln!("yozildi: {}", path.display());
        return;
    }
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let want: Golden = serde_json::from_str(&text).unwrap();

    // Tekislash sof f32 arifmetika (FMA'siz) — bit-darajada bir xil bo'lishi kerak.
    assert_eq!(got.synthetic_aligned_fnv, want.synthetic_aligned_fnv, "tekislangan kesma farq qiladi");
    let c = cos(&got.synthetic_embedding, &want.synthetic_embedding);
    eprintln!("sun'iy kirish: cos={c:.6}");
    assert!(c > MIN_COS, "embedding platformaga qarab farq qilyapti: cos={c}");

    assert_eq!(
        got.photos.keys().collect::<Vec<_>>(),
        want.photos.keys().collect::<Vec<_>>(),
        "fixtures rasmlari golden.json bilan mos emas (KHOFACE_WRITE_GOLDEN=1 bilan qayta yozing)"
    );
    for (name, w) in &want.photos {
        let g = &got.photos[name];
        assert_eq!(g.faces, w.faces, "{name}: yuzlar soni");
        let dist = g
            .bbox
            .iter()
            .chain(g.landmarks.iter().flatten())
            .zip(w.bbox.iter().chain(w.landmarks.iter().flatten()))
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        let c = cos(&g.embedding, &w.embedding);
        eprintln!("{name}: koordinata farqi {dist:.3} px, cos={c:.6}");
        assert!(dist <= MAX_PX, "{name}: detektor natijasi farq qiladi ({dist} px)");
        assert!(c > MIN_COS, "{name}: embedding farq qiladi: cos={c}");
    }
}
