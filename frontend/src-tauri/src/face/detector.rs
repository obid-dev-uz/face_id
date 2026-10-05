//! YuNet (2023mar) yuz detektori. Dekodlash va NMS OpenCV `FaceDetectorYN` bilan bir xil:
//! har bir stride (8, 16, 32) uchun cls/obj/bbox/kps chiqishlari.

use image::{imageops, RgbImage};

use super::backend::{Array4, ArrayD, InferenceBackend, TractBackend};
use super::Face;

const STRIDES: [usize; 3] = [8, 16, 32];

pub struct Detector {
    backend: Box<dyn InferenceBackend>,
    /// Kirish o'lchami (32 ga karrali). Rasm shu o'lchamga sig'diriladi, qolgani nol bilan to'ldiriladi.
    pub input_w: usize,
    pub input_h: usize,
    /// `[cls, obj, bbox, kps]` chiqishlarining har bir stride uchun indekslari.
    heads: [[usize; 4]; 3],
    pub score_threshold: f32,
    pub nms_iou: f32,
}

impl Detector {
    pub fn load(bytes: &[u8], input_w: usize, input_h: usize) -> anyhow::Result<Self> {
        anyhow::ensure!(input_w.is_multiple_of(32) && input_h.is_multiple_of(32), "YuNet kirishi 32 ga karrali bo'lishi kerak");
        let backend = TractBackend::load(bytes, [1, 3, input_h, input_w])?;
        let heads = resolve_heads(backend.output_names())?;
        Ok(Self { backend: Box::new(backend), input_w, input_h, heads, score_threshold: 0.8, nms_iou: 0.3 })
    }

    pub fn detect(&self, img: &RgbImage) -> anyhow::Result<Vec<Face>> {
        let (input, scale) = self.preprocess(img);
        let out = self.backend.run(input)?;
        let mut faces = Vec::new();
        for (i, &stride) in STRIDES.iter().enumerate() {
            let [c, o, b, k] = self.heads[i];
            decode_level(
                flat(&out[c]),
                flat(&out[o]),
                flat(&out[b]),
                flat(&out[k]),
                stride,
                self.input_w / stride,
                self.score_threshold,
                &mut faces,
            );
        }
        let mut faces = nms(faces, self.nms_iou);
        let (iw, ih) = (img.width() as f32, img.height() as f32);
        for f in &mut faces {
            f.scale(1.0 / scale);
            f.clip(iw, ih);
        }
        Ok(faces)
    }

    /// RGB → BGR, 0..255, NCHW. Rasm nisbati saqlanadi, chap-yuqori burchakka joylanadi.
    fn preprocess(&self, img: &RgbImage) -> (Array4, f32) {
        let scale = (self.input_w as f32 / img.width() as f32).min(self.input_h as f32 / img.height() as f32);
        let (w, h) = (
            ((img.width() as f32 * scale).round() as u32).clamp(1, self.input_w as u32),
            ((img.height() as f32 * scale).round() as u32).clamp(1, self.input_h as u32),
        );
        let resized;
        let src = if (w, h) == img.dimensions() {
            img
        } else {
            resized = imageops::resize(img, w, h, imageops::FilterType::Triangle);
            &resized
        };
        let mut t = Array4::zeros((1, 3, self.input_h, self.input_w));
        for (x, y, p) in src.enumerate_pixels() {
            let (x, y) = (x as usize, y as usize);
            t[[0, 0, y, x]] = p[2] as f32;
            t[[0, 1, y, x]] = p[1] as f32;
            t[[0, 2, y, x]] = p[0] as f32;
        }
        (t, scale)
    }
}

fn flat(a: &ArrayD) -> &[f32] {
    a.as_slice().expect("tract chiqishi ketma-ket xotirada")
}

/// Chiqishlarni nomi bo'yicha topadi (`cls_8`, `obj_16`, `bbox_32`, `kps_8` ...).
fn resolve_heads(names: &[String]) -> anyhow::Result<[[usize; 4]; 3]> {
    let find = |kind: &str, stride: usize| {
        let want = format!("{kind}_{stride}");
        names
            .iter()
            .position(|n| *n == want)
            .ok_or_else(|| anyhow::anyhow!("YuNet chiqishi topilmadi: {want} (bor: {names:?})"))
    };
    let mut heads = [[0; 4]; 3];
    for (i, &s) in STRIDES.iter().enumerate() {
        heads[i] = [find("cls", s)?, find("obj", s)?, find("bbox", s)?, find("kps", s)?];
    }
    Ok(heads)
}

#[allow(clippy::too_many_arguments)]
fn decode_level(
    cls: &[f32],
    obj: &[f32],
    bbox: &[f32],
    kps: &[f32],
    stride: usize,
    cols: usize,
    threshold: f32,
    out: &mut Vec<Face>,
) {
    let s = stride as f32;
    for idx in 0..cls.len() {
        let score = (cls[idx].clamp(0.0, 1.0) * obj[idx].clamp(0.0, 1.0)).sqrt();
        if score < threshold {
            continue;
        }
        let (r, c) = ((idx / cols) as f32, (idx % cols) as f32);
        let b = &bbox[idx * 4..idx * 4 + 4];
        let (cx, cy) = ((c + b[0]) * s, (r + b[1]) * s);
        let (w, h) = (b[2].exp() * s, b[3].exp() * s);
        let k = &kps[idx * 10..idx * 10 + 10];
        let landmarks = std::array::from_fn(|n| [(k[2 * n] + c) * s, (k[2 * n + 1] + r) * s]);
        out.push(Face { bbox: [cx - w / 2.0, cy - h / 2.0, w, h], score, landmarks });
    }
}

fn iou(a: &[f32; 4], b: &[f32; 4]) -> f32 {
    let ix = (a[0] + a[2]).min(b[0] + b[2]) - a[0].max(b[0]);
    let iy = (a[1] + a[3]).min(b[1] + b[3]) - a[1].max(b[1]);
    if ix <= 0.0 || iy <= 0.0 {
        return 0.0;
    }
    let inter = ix * iy;
    inter / (a[2] * a[3] + b[2] * b[3] - inter)
}

fn nms(mut faces: Vec<Face>, threshold: f32) -> Vec<Face> {
    faces.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut keep: Vec<Face> = Vec::new();
    for f in faces {
        if keep.iter().all(|k| iou(&k.bbox, &f.bbox) <= threshold) {
            keep.push(f);
        }
    }
    keep
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_single_anchor() {
        // 2×2 katak, stride 8; faqat (r=1, c=0) katagida yuz bor
        let cls = [0.0, 0.0, 0.81, 0.0];
        let obj = [0.0, 0.0, 1.0, 0.0];
        let mut bbox = [0.0; 16];
        bbox[8..12].copy_from_slice(&[0.5, 0.5, 2f32.ln(), 2f32.ln()]);
        let mut kps = [0.0; 40];
        kps[20..30].copy_from_slice(&[0.25, 0.25, 0.75, 0.25, 0.5, 0.5, 0.25, 0.75, 0.75, 0.75]);
        let mut out = Vec::new();
        decode_level(&cls, &obj, &bbox, &kps, 8, 2, 0.8, &mut out);
        assert_eq!(out.len(), 1);
        let f = &out[0];
        assert!((f.score - 0.9).abs() < 1e-6);
        // markaz (0.5*8, 1.5*8) = (4, 12), o'lcham 16×16
        assert_eq!(f.bbox, [-4.0, 4.0, 16.0, 16.0]);
        assert_eq!(f.landmarks[0], [2.0, 10.0]);
        assert_eq!(f.landmarks[4], [6.0, 14.0]);
    }

    #[test]
    fn nms_keeps_best_of_overlapping() {
        let f = |x: f32, score: f32| Face { bbox: [x, 0.0, 10.0, 10.0], score, landmarks: [[0.0; 2]; 5] };
        let kept = nms(vec![f(0.0, 0.85), f(1.0, 0.95), f(50.0, 0.9)], 0.3);
        assert_eq!(kept.iter().map(|k| k.score).collect::<Vec<_>>(), [0.95, 0.9]);
    }

    #[test]
    fn head_names_resolved() {
        let names: Vec<String> = ["cls_8", "cls_16", "cls_32", "obj_8", "obj_16", "obj_32", "bbox_8", "bbox_16", "bbox_32", "kps_8", "kps_16", "kps_32"]
            .map(String::from)
            .to_vec();
        assert_eq!(resolve_heads(&names).unwrap()[1], [1, 4, 7, 10]);
        assert!(resolve_heads(&names[..11]).is_err());
    }
}
