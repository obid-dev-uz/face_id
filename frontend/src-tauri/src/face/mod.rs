//! Yuz aniqlash, tekislash, embedding va solishtirish. Butun konveyer sof Rust'da
//! (tract), shuning uchun har bir platformada bir xil natija beradi.

pub mod align;
pub mod backend;
pub mod detector;
pub mod embedder;
#[cfg(test)]
mod golden;
#[cfg(test)]
mod bench;
pub mod matcher;
pub mod quality;

use std::time::Instant;

use image::RgbImage;
use serde::Serialize;

use detector::Detector;
use embedder::Embedder;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Face {
    /// `[x, y, w, h]` asl rasm koordinatalarida.
    pub bbox: [f32; 4],
    pub score: f32,
    /// O'ng ko'z, chap ko'z, burun, og'izning o'ng va chap burchagi.
    pub landmarks: [[f32; 2]; 5],
}

impl Face {
    fn scale(&mut self, k: f32) {
        self.bbox.iter_mut().for_each(|v| *v *= k);
        self.landmarks.iter_mut().flatten().for_each(|v| *v *= k);
    }

    fn clip(&mut self, w: f32, h: f32) {
        let [x, y, bw, bh] = self.bbox;
        let (x0, y0) = (x.clamp(0.0, w), y.clamp(0.0, h));
        let (x1, y1) = ((x + bw).clamp(0.0, w), (y + bh).clamp(0.0, h));
        self.bbox = [x0, y0, x1 - x0, y1 - y0];
    }
}

pub struct FaceEngine {
    /// Galereya rasmlari uchun (katta kirish, aniqroq).
    pub detector: Detector,
    /// Kamera oqimi uchun (160×160): eski telefonda ~60 ms.
    pub live_detector: Detector,
    pub embedder: Embedder,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Timings {
    pub detect_ms: f64,
    pub align_ms: f64,
    pub embed_ms: f64,
}

pub struct Analyzed {
    pub face: Face,
    pub aligned: RgbImage,
    pub embedding: Vec<f32>,
}

impl FaceEngine {
    pub const DIM: usize = 128;
    /// Bazadagi `face_embeddings.model` qiymati: embeddinglar faqat bir xil model ichida
    /// solishtiriladi. SFace int8 varianti tract'da ishlamaydi (QLinearMul yo'q).
    pub const MODEL_ID: &'static str = "sface-2021dec";

    pub const LIVE_DET: usize = 160;

    pub fn load(yunet: &[u8], sface: &[u8], det_w: usize, det_h: usize) -> anyhow::Result<Self> {
        Ok(Self {
            detector: Detector::load(yunet, det_w, det_h)?,
            live_detector: Detector::load(yunet, Self::LIVE_DET, Self::LIVE_DET)?,
            embedder: Embedder::load(sface)?,
        })
    }

    /// Yuz atrofidagi kvadrat kesmani live detektorga berib, aniqroq landmarklar oladi
    /// (160 px'li butun kadrdagi landmarklar tekislash uchun juda taxminiy).
    pub fn refine(&self, img: &RgbImage, f: &Face) -> Option<Face> {
        let [x, y, bw, bh] = f.bbox;
        let side = (bw.max(bh) * 1.8).min(img.width().min(img.height()) as f32);
        let x0 = (x + bw / 2.0 - side / 2.0).clamp(0.0, img.width() as f32 - side) as u32;
        let y0 = (y + bh / 2.0 - side / 2.0).clamp(0.0, img.height() as f32 - side) as u32;
        let crop = image::imageops::crop_imm(img, x0, y0, side as u32, side as u32).to_image();
        let mut best = self
            .live_detector
            .detect(&crop)
            .ok()?
            .into_iter()
            .max_by(|a, b| (a.bbox[2] * a.bbox[3]).total_cmp(&(b.bbox[2] * b.bbox[3])))?;
        best.bbox[0] += x0 as f32;
        best.bbox[1] += y0 as f32;
        for p in &mut best.landmarks {
            p[0] += x0 as f32;
            p[1] += y0 as f32;
        }
        Some(best)
    }

    /// Rasmdagi barcha yuzlarni aniqlab, har biri uchun embedding hisoblaydi.
    pub fn analyze(&self, img: &RgbImage, max_faces: usize) -> anyhow::Result<(Vec<Analyzed>, Timings)> {
        let mut t = Timings::default();
        let t0 = Instant::now();
        let faces = self.detector.detect(img)?;
        t.detect_ms = ms(t0);
        let mut out = Vec::new();
        for face in faces.into_iter().take(max_faces) {
            let t0 = Instant::now();
            let (aligned, _) = align::align(img, &face.landmarks);
            t.align_ms += ms(t0);
            let t0 = Instant::now();
            let embedding = self.embedder.embed(&aligned)?;
            t.embed_ms += ms(t0);
            out.push(Analyzed { face, aligned, embedding });
        }
        Ok((out, t))
    }
}

fn ms(t0: Instant) -> f64 {
    t0.elapsed().as_secs_f64() * 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Modellar papkasi: `KHOFACE_MODELS` (telefonda: scripts/android-test.sh) yoki loyihadagi resources.
    pub(crate) fn models_dir() -> std::path::PathBuf {
        std::env::var_os("KHOFACE_MODELS")
            .map(Into::into)
            .unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/models"))
    }

    /// Haqiqiy modellar bilan; modellar yuklanmagan bo'lsa (scripts/fetch-models.sh) o'tkazib yuboriladi.
    /// `KHOFACE_REQUIRE_MODELS=1` bo'lsa (qurilmadagi test), modelsiz test yiqiladi.
    pub(crate) fn engine() -> Option<FaceEngine> {
        let dir = models_dir();
        let read = |name: &str| match std::fs::read(dir.join(name)) {
            Ok(b) => Some(b),
            Err(e) if std::env::var_os("KHOFACE_REQUIRE_MODELS").is_some() => panic!("{name}: {e}"),
            Err(_) => None,
        };
        let yunet = read("face_detection_yunet_2023mar.onnx")?;
        let sface = read("face_recognition_sface_2021dec.onnx")?;
        Some(FaceEngine::load(&yunet, &sface, 320, 256).unwrap())
    }

    #[test]
    fn real_models_run_and_are_deterministic() {
        let Some(e) = engine() else {
            eprintln!("modellar yo'q — test o'tkazib yuborildi");
            return;
        };
        // Yuzsiz kadr: yolg'on topilma bo'lmasligi kerak
        let blank = RgbImage::from_pixel(640, 480, image::Rgb([128, 128, 128]));
        assert!(e.detector.detect(&blank).unwrap().is_empty());

        // Embedding L2-normallangan va bir xil kirishda aynan bir xil
        let crop = RgbImage::from_fn(112, 112, |x, y| image::Rgb([(x * 2) as u8, (y * 2) as u8, ((x + y) % 256) as u8]));
        let a = e.embedder.embed(&crop).unwrap();
        let b = e.embedder.embed(&crop).unwrap();
        assert_eq!(a.len(), FaceEngine::DIM);
        assert_eq!(a, b);
        let norm: f32 = a.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-4);
    }
}
