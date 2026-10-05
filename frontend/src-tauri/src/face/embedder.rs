//! SFace embedding: tekislangan 112×112 RGB yuz → 128 o'lchamli L2-normallangan vektor.

use image::RgbImage;

use super::align::SIZE;
use super::backend::{Array4, InferenceBackend, TractBackend};
use super::matcher::l2_normalize;

pub struct Embedder {
    backend: Box<dyn InferenceBackend>,
}

impl Embedder {
    pub fn load(bytes: &[u8]) -> anyhow::Result<Self> {
        let s = SIZE as usize;
        Ok(Self { backend: Box::new(TractBackend::load(bytes, [1, 3, s, s])?) })
    }

    pub fn embed(&self, aligned: &RgbImage) -> anyhow::Result<Vec<f32>> {
        let s = SIZE as usize;
        anyhow::ensure!(aligned.dimensions() == (SIZE, SIZE), "SFace kirishi 112×112 bo'lishi kerak");
        // OpenCV FaceRecognizerSF: blobFromImage(swapRB=true) → RGB, 0..255, normallashsiz
        let t = Array4::from_shape_fn((1, 3, s, s), |(_, c, y, x)| aligned.get_pixel(x as u32, y as u32)[c] as f32);
        let out = self.backend.run(t)?;
        let mut v: Vec<f32> = out[0].iter().copied().collect();
        l2_normalize(&mut v);
        Ok(v)
    }
}
