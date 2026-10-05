//! Namuna sifatini baholash. Matnlar frontend'da (i18n), bu yerda faqat kodlar.

use image::RgbImage;
use serde::Serialize;

use super::align::REF_5PTS;
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
    /// Taxminiy chap-o'ng burilish, gradusda (0 = to'g'ri qaragan). Musbat — odam o'zining
    /// chap tomoniga burilgan (ko'zgusiz rasmda burun o'ngga siljigan).
    pub yaw_deg: f32,
    /// Taxminiy yuqori-past qiyalik, gradusda. Musbat — yuqoriga qaragan.
    pub pitch_deg: f32,
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

pub fn assess(face: &Face, aligned: &RgbImage, t: &Thresholds) -> Quality {
    let gray: Vec<f32> = aligned
        .pixels()
        .map(|p| 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32)
        .collect();
    let brightness = center_mean(&gray, aligned.width() as usize);
    let sharpness = laplacian_variance(&gray, aligned.width() as usize, aligned.height() as usize);
    let (yaw_deg, pitch_deg) = estimate_pose(&face.landmarks);
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
    Quality { face_width, score: face.score, sharpness, yaw_deg, pitch_deg, brightness, issues }
}

/// Bosh pozasi (yaw, pitch) xom landmarklardan: ko'zlar o'rtasidan og'iz o'rtasiga
/// o'tgan o'q yuzning vertikal o'qi; burunning shu o'qdan siljishi burilishni ko'rsatadi.
/// (Tekislangan koordinatalar mos emas: tekislash burunga ham moslanib, siljishni yutadi.)
/// Yaw musbat — burun rasmda o'ngga siljigan; pitch musbat — yuqoriga qaragan.
pub fn estimate_pose(lm: &[[f32; 2]; 5]) -> (f32, f32) {
    let eye = [(lm[0][0] + lm[1][0]) / 2.0, (lm[0][1] + lm[1][1]) / 2.0];
    let mouth = [(lm[3][0] + lm[4][0]) / 2.0, (lm[3][1] + lm[4][1]) / 2.0];
    let axis = [mouth[0] - eye[0], mouth[1] - eye[1]];
    let axis_len2 = (axis[0] * axis[0] + axis[1] * axis[1]).max(1.0);
    let eye_dist = ((lm[1][0] - lm[0][0]).powi(2) + (lm[1][1] - lm[0][1]).powi(2)).sqrt().max(1.0);
    let n = [lm[2][0] - eye[0], lm[2][1] - eye[1]];
    // O'qqa perpendikulyar (rasm o'ngi tomon musbat) va o'q bo'ylab tashkil etuvchilar
    let side = (axis[1] * n[0] - axis[0] * n[1]) / axis_len2.sqrt();
    let along = (n[0] * axis[0] + n[1] * axis[1]) / axis_len2;
    // 45° burilishda burun ko'zlar orasining ~yarmiga siljiydi
    let yaw = (side / eye_dist * 2.0).clamp(-1.0, 1.0).asin().to_degrees();
    let ref_along = {
        let r = &REF_5PTS;
        let eye_y = (r[0][1] + r[1][1]) / 2.0;
        (r[2][1] - eye_y) / ((r[3][1] + r[4][1]) / 2.0 - eye_y)
    };
    // Nisbatning 0.1 siljishi ≈ 15°
    let pitch = ((ref_along - along) * 150.0).clamp(-90.0, 90.0);
    (yaw, pitch)
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
    use crate::face::align::Similarity;

    fn face(lm: [[f32; 2]; 5], width: f32, score: f32) -> Face {
        Face { bbox: [0.0, 0.0, width, width], score, landmarks: lm }
    }

    #[test]
    fn pose_from_landmarks() {
        let (yaw, pitch) = estimate_pose(&REF_5PTS);
        assert!(yaw.abs() < 1.0 && pitch.abs() < 1.0, "{yaw} {pitch}");
        let mut turned = REF_5PTS;
        turned[2][0] += 10.0;
        assert!(estimate_pose(&turned).0 > 30.0);
        let mut up = REF_5PTS;
        up[2][1] -= 6.0;
        assert!(estimate_pose(&up).1 > 15.0);
        // Rasm burilgan (bosh yon tomonga egilgan) bo'lsa ham yuz to'g'ri qaragan deb topiladi
        let m = Similarity { a: 0.0, b: 1.0, tx: 200.0, ty: 0.0 }; // 90° burish
        let (yaw, pitch) = estimate_pose(&REF_5PTS.map(|p| m.apply(p)));
        assert!(yaw.abs() < 1.0 && pitch.abs() < 1.0, "{yaw} {pitch}");
    }

    #[test]
    fn flat_dark_crop_is_blurry_and_dark() {
        let img = RgbImage::from_pixel(112, 112, image::Rgb([20, 20, 20]));
        let q = assess(&face(REF_5PTS, 40.0, 0.5), &img, &Thresholds::default());
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
        let q = assess(&face(REF_5PTS, 150.0, 0.95), &img, &Thresholds::default());
        assert!(q.issues.is_empty(), "{:?}", q.issues);
    }
}
