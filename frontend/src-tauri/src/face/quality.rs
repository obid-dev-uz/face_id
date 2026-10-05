//! Namuna sifatini baholash. Matnlar frontend'da (i18n), bu yerda faqat kodlar.

use image::RgbImage;
use serde::Serialize;

use super::align::{Similarity, REF_5PTS};
use super::Face;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum QualityIssue {
    TooSmall,
    LowScore,
    Blurry,
    NotFrontal,
    TooDark,
    TooBright,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Quality {
    pub face_width: f32,
    pub score: f32,
    /// Tekislangan yuzdagi Laplasian dispersiyasi: kichik bo'lsa rasm xira.
    pub sharpness: f32,
    /// Taxminiy chap-o'ng burilish, gradusda (0 = to'g'ri qaragan).
    pub yaw_deg: f32,
    pub brightness: f32,
    pub issues: Vec<QualityIssue>,
}

pub struct Thresholds {
    pub min_face_width: f32,
    pub min_score: f32,
    pub min_sharpness: f32,
    pub max_yaw_deg: f32,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self { min_face_width: 80.0, min_score: 0.85, min_sharpness: 60.0, max_yaw_deg: 30.0 }
    }
}

pub fn assess(face: &Face, aligned: &RgbImage, m: &Similarity, t: &Thresholds) -> Quality {
    let gray: Vec<f32> = aligned
        .pixels()
        .map(|p| 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32)
        .collect();
    let brightness = center_mean(&gray, aligned.width() as usize);
    let sharpness = laplacian_variance(&gray, aligned.width() as usize, aligned.height() as usize);
    let yaw_deg = estimate_yaw(&face.landmarks, m);
    let face_width = face.bbox[2];

    let mut issues = Vec::new();
    if face_width < t.min_face_width {
        issues.push(QualityIssue::TooSmall);
    }
    if face.score < t.min_score {
        issues.push(QualityIssue::LowScore);
    }
    if sharpness < t.min_sharpness {
        issues.push(QualityIssue::Blurry);
    }
    if yaw_deg.abs() > t.max_yaw_deg {
        issues.push(QualityIssue::NotFrontal);
    }
    if brightness < 50.0 {
        issues.push(QualityIssue::TooDark);
    } else if brightness > 220.0 {
        issues.push(QualityIssue::TooBright);
    }
    Quality { face_width, score: face.score, sharpness, yaw_deg, brightness, issues }
}

/// Tekislangan koordinatalarda burun ko'zlar o'rtasidan qancha siljiganiga qarab.
/// Shablonda burun deyarli markazda; 45° burilishda siljish ko'zlar orasining ~yarmi.
fn estimate_yaw(lm: &[[f32; 2]; 5], m: &Similarity) -> f32 {
    let p = lm.map(|q| m.apply(q));
    let eye_mid = (p[0][0] + p[1][0]) / 2.0;
    let eye_dist = (p[1][0] - p[0][0]).abs().max(1.0);
    let ref_off = REF_5PTS[2][0] - (REF_5PTS[0][0] + REF_5PTS[1][0]) / 2.0;
    let r = ((p[2][0] - eye_mid - ref_off) / eye_dist * 2.0).clamp(-1.0, 1.0);
    r.asin().to_degrees()
}

fn laplacian_variance(g: &[f32], w: usize, h: usize) -> f32 {
    let mut vals = Vec::with_capacity((w - 2) * (h - 2));
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let i = y * w + x;
            vals.push(g[i - w] + g[i + w] + g[i - 1] + g[i + 1] - 4.0 * g[i]);
        }
    }
    let n = vals.len() as f32;
    let mean = vals.iter().sum::<f32>() / n;
    vals.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / n
}

/// Yuzning markaziy qismi (fon va qora chegaralarsiz).
fn center_mean(g: &[f32], w: usize) -> f32 {
    let (lo, hi) = (w / 4, w * 3 / 4);
    let mut sum = 0.0;
    for y in lo..hi {
        for x in lo..hi {
            sum += g[y * w + x];
        }
    }
    sum / ((hi - lo) * (hi - lo)) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face(lm: [[f32; 2]; 5], width: f32, score: f32) -> Face {
        Face { bbox: [0.0, 0.0, width, width], score, landmarks: lm }
    }

    #[test]
    fn frontal_template_has_zero_yaw() {
        let m = Similarity::estimate(&REF_5PTS, &REF_5PTS);
        assert!(estimate_yaw(&REF_5PTS, &m).abs() < 1.0);
        let mut turned = REF_5PTS;
        turned[2][0] += 15.0;
        assert!(estimate_yaw(&turned, &m) > 30.0);
    }

    #[test]
    fn flat_dark_crop_is_blurry_and_dark() {
        let img = RgbImage::from_pixel(112, 112, image::Rgb([20, 20, 20]));
        let m = Similarity::estimate(&REF_5PTS, &REF_5PTS);
        let q = assess(&face(REF_5PTS, 40.0, 0.5), &img, &m, &Thresholds::default());
        assert_eq!(
            q.issues,
            [QualityIssue::TooSmall, QualityIssue::LowScore, QualityIssue::Blurry, QualityIssue::TooDark]
        );
    }

    #[test]
    fn checkerboard_is_sharp() {
        let img = RgbImage::from_fn(112, 112, |x, y| {
            let v = if (x / 2 + y / 2) % 2 == 0 { 40 } else { 200 };
            image::Rgb([v, v, v])
        });
        let m = Similarity::estimate(&REF_5PTS, &REF_5PTS);
        let q = assess(&face(REF_5PTS, 150.0, 0.95), &img, &m, &Thresholds::default());
        assert!(q.issues.is_empty(), "{:?}", q.issues);
    }
}
