//! Aniqlik benchmarki (ROC, chegara tanlash). Oddiy testlarda ishlamaydi (`#[ignore]`):
//!
//! ```text
//! KHOFACE_DATASET=/yo'l/to'plam cargo test --lib roc -- --ignored --nocapture
//! ```
//!
//! To'plam tuzilishi: har bir shaxs uchun alohida papka, ichida `.jpg`/`.png` rasmlar
//! (`to'plam/ali/1.jpg`, `to'plam/ali/2.jpg`, `to'plam/vali/1.jpg`, ...). Har bir rasmdagi eng katta
//! yuz olinadi. Barcha juftliklar solishtiriladi: bir shaxs — "o'zi", turli shaxs — "begona".
//! `KHOFACE_ROC_CSV=fayl.csv` bo'lsa, ROC egri chizig'i (chegara, FAR, TAR) yoziladi.

use std::path::{Path, PathBuf};

use super::align::align;
use super::tests::models_dir;
use super::FaceEngine;
use crate::commands::engine::DEFAULT_THRESHOLD;
use crate::engine::PHOTO_DET;

/// Ilovadagi kabi: katta rasmlar tahlildan oldin kichraytiriladi.
const MAX_SIDE: u32 = 1280;

struct Sample {
    person: usize,
    embedding: Vec<f32>,
}

fn images(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| matches!(x.to_ascii_lowercase().as_str(), "jpg" | "jpeg" | "png"))
        })
        .collect();
    v.sort();
    v
}

/// `scores` o'sish tartibida; `>= t` bo'lganlar ulushi.
fn rate_at_or_above(sorted: &[f32], t: f32) -> f64 {
    let below = sorted.partition_point(|&s| s < t);
    (sorted.len() - below) as f64 / sorted.len().max(1) as f64
}

#[test]
#[ignore]
fn roc() {
    let root = PathBuf::from(std::env::var_os("KHOFACE_DATASET").expect("KHOFACE_DATASET ko'rsatilmagan"));
    let dir = models_dir();
    let engine = FaceEngine::load(
        &std::fs::read(dir.join("face_detection_yunet_2023mar.onnx")).unwrap(),
        &std::fs::read(dir.join("face_recognition_sface_2021dec.onnx")).unwrap(),
        PHOTO_DET.0,
        PHOTO_DET.1,
    )
    .unwrap();

    let mut persons: Vec<PathBuf> = std::fs::read_dir(&root).unwrap().flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    persons.sort();
    let (mut samples, mut no_face) = (Vec::new(), Vec::new());
    for (pi, pdir) in persons.iter().enumerate() {
        for path in images(pdir) {
            let mut img = image::open(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            if img.width().max(img.height()) > MAX_SIDE {
                img = img.resize(MAX_SIDE, MAX_SIDE, image::imageops::FilterType::Triangle);
            }
            let img = img.to_rgb8();
            let faces = engine.detector.detect(&img).unwrap();
            let Some(face) = faces.iter().max_by(|a, b| (a.bbox[2] * a.bbox[3]).total_cmp(&(b.bbox[2] * b.bbox[3]))) else {
                no_face.push(path);
                continue;
            };
            let (aligned, _) = align(&img, &face.landmarks);
            samples.push(Sample { person: pi, embedding: engine.embedder.embed(&aligned).unwrap() });
        }
    }

    let (mut genuine, mut impostor) = (Vec::new(), Vec::new());
    for i in 0..samples.len() {
        for j in i + 1..samples.len() {
            let s: f32 = samples[i].embedding.iter().zip(&samples[j].embedding).map(|(a, b)| a * b).sum();
            if samples[i].person == samples[j].person {
                genuine.push(s);
            } else {
                impostor.push(s);
            }
        }
    }
    genuine.sort_by(f32::total_cmp);
    impostor.sort_by(f32::total_cmp);

    println!("\n== Aniqlik benchmarki: {}", root.display());
    println!("shaxslar: {}, rasmlar: {}, yuz topilmagan: {}", persons.len(), samples.len() + no_face.len(), no_face.len());
    for p in &no_face {
        println!("  yuz yo'q: {}", p.display());
    }
    println!("juftliklar: o'zi {} ta, begona {} ta", genuine.len(), impostor.len());
    assert!(
        !genuine.is_empty() && !impostor.is_empty(),
        "kamida 2 ta shaxs va bir shaxsga yuzi topilgan 2 ta rasm kerak"
    );

    // Chegaralar: 0.000 .. 1.000, 0.001 qadam
    let curve: Vec<(f32, f64, f64)> = (0..=1000)
        .map(|i| {
            let t = i as f32 / 1000.0;
            (t, rate_at_or_above(&impostor, t), rate_at_or_above(&genuine, t))
        })
        .collect();
    let (eer_t, eer_far, eer_tar) = *curve
        .iter()
        .min_by(|a, b| (a.1 - (1.0 - a.2)).abs().total_cmp(&(b.1 - (1.0 - b.2)).abs()))
        .unwrap();
    println!("EER ≈ {:.2}% (chegara {eer_t:.3}; FAR {:.2}%, FRR {:.2}%)", (eer_far + 1.0 - eer_tar) * 50.0, eer_far * 100.0, (1.0 - eer_tar) * 100.0);

    let show = |label: &str, t: f32| {
        println!(
            "{label:<24} chegara {t:.3}: TAR {:.2}%  FAR {:.4}%",
            rate_at_or_above(&genuine, t) * 100.0,
            rate_at_or_above(&impostor, t) * 100.0
        );
    };
    show("joriy (ilova)", DEFAULT_THRESHOLD);
    for far in [1e-2, 1e-3, 1e-4] {
        // FAR ≤ maqsad bo'lgan eng kichik chegara
        if let Some(&(t, ..)) = curve.iter().find(|c| c.1 <= far) {
            show(&format!("FAR ≤ {}%", far * 100.0), t);
        }
    }

    if let Some(csv) = std::env::var_os("KHOFACE_ROC_CSV") {
        let mut out = String::from("threshold,far,tar\n");
        for (t, far, tar) in &curve {
            out.push_str(&format!("{t:.3},{far:.6},{tar:.6}\n"));
        }
        std::fs::write(&csv, out).unwrap();
        println!("ROC: {}", PathBuf::from(csv).display());
    }
}
