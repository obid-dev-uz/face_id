//! 5 nuqta bo'yicha yuzni 112×112 ArcFace shabloniga tekislash.
//! OpenCV `FaceRecognizerSF::alignCrop` bilan bir xil shablon va bilinear interpolatsiya.

use image::RgbImage;

pub const SIZE: u32 = 112;

/// Nuqtalar tartibi YuNet bilan bir xil: o'ng ko'z, chap ko'z, burun, og'izning o'ng va chap burchagi
/// (rasmda ko'rinishi bo'yicha: chapdan o'ngga).
pub const REF_5PTS: [[f32; 2]; 5] = [
    [38.2946, 51.6963],
    [73.5318, 51.5014],
    [56.0252, 71.7366],
    [41.5493, 92.3655],
    [70.7299, 92.2041],
];

/// `dst = [a -b; b a] * src + t` ko'rinishidagi o'xshashlik almashtirishi (aylantirish,
/// masshtab, siljitish; ko'zgu aksisiz).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Similarity {
    pub a: f32,
    pub b: f32,
    pub tx: f32,
    pub ty: f32,
}

impl Similarity {
    /// Eng kichik kvadratlar bo'yicha baholash (2D Umeyama, ko'zgusiz holat).
    pub fn estimate(src: &[[f32; 2]; 5], dst: &[[f32; 2]; 5]) -> Self {
        let n = src.len() as f32;
        let mean = |p: &[[f32; 2]; 5]| {
            let (x, y) = p.iter().fold((0.0, 0.0), |(x, y), q| (x + q[0], y + q[1]));
            [x / n, y / n]
        };
        let (ms, md) = (mean(src), mean(dst));
        let (mut num_a, mut num_b, mut den) = (0.0f32, 0.0f32, 0.0f32);
        for (s, d) in src.iter().zip(dst) {
            let (sx, sy) = (s[0] - ms[0], s[1] - ms[1]);
            let (dx, dy) = (d[0] - md[0], d[1] - md[1]);
            num_a += sx * dx + sy * dy;
            num_b += sx * dy - sy * dx;
            den += sx * sx + sy * sy;
        }
        let den = den.max(1e-6);
        let (a, b) = (num_a / den, num_b / den);
        Self { a, b, tx: md[0] - a * ms[0] + b * ms[1], ty: md[1] - b * ms[0] - a * ms[1] }
    }

    pub fn apply(&self, p: [f32; 2]) -> [f32; 2] {
        [self.a * p[0] - self.b * p[1] + self.tx, self.b * p[0] + self.a * p[1] + self.ty]
    }

    pub fn inverse(&self) -> Self {
        let det = (self.a * self.a + self.b * self.b).max(1e-12);
        let (a, b) = (self.a / det, -self.b / det);
        Self { a, b, tx: -(a * self.tx - b * self.ty), ty: -(b * self.tx + a * self.ty) }
    }
}

/// Yuzni 112×112 ga kesib tekislaydi. Rasmdan tashqaridagi piksellar qora bo'ladi.
pub fn align(img: &RgbImage, landmarks: &[[f32; 2]; 5]) -> (RgbImage, Similarity) {
    let m = Similarity::estimate(landmarks, &REF_5PTS);
    let inv = m.inverse();
    let (w, h) = (img.width() as i64, img.height() as i64);
    let raw = img.as_raw();
    let px = |x: i64, y: i64, c: usize| -> f32 {
        if x < 0 || y < 0 || x >= w || y >= h {
            0.0
        } else {
            raw[((y * w + x) * 3) as usize + c] as f32
        }
    };
    let mut out = RgbImage::new(SIZE, SIZE);
    for v in 0..SIZE {
        for u in 0..SIZE {
            let [sx, sy] = inv.apply([u as f32, v as f32]);
            let (x0, y0) = (sx.floor(), sy.floor());
            let (fx, fy) = (sx - x0, sy - y0);
            let (x0, y0) = (x0 as i64, y0 as i64);
            let mut rgb = [0u8; 3];
            for (c, val) in rgb.iter_mut().enumerate() {
                let top = px(x0, y0, c) * (1.0 - fx) + px(x0 + 1, y0, c) * fx;
                let bot = px(x0, y0 + 1, c) * (1.0 - fx) + px(x0 + 1, y0 + 1, c) * fx;
                *val = (top * (1.0 - fy) + bot * fy).round().clamp(0.0, 255.0) as u8;
            }
            out.put_pixel(u, v, image::Rgb(rgb));
        }
    }
    (out, m)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3
    }

    #[test]
    fn recovers_known_transform() {
        // 30° burish, 2.5x masshtab, siljitish
        let (s, r) = (2.5f32, 30f32.to_radians());
        let truth = Similarity { a: s * r.cos(), b: s * r.sin(), tx: 100.0, ty: -40.0 };
        let src = REF_5PTS;
        let dst = src.map(|p| truth.apply(p));
        let est = Similarity::estimate(&src, &dst);
        for p in src {
            assert!(close(est.apply(p), truth.apply(p)));
        }
        let back = est.inverse();
        for p in dst {
            assert!(close(est.apply(back.apply(p)), p));
        }
    }

    #[test]
    fn identity_on_reference_points() {
        let m = Similarity::estimate(&REF_5PTS, &REF_5PTS);
        assert!((m.a - 1.0).abs() < 1e-5 && m.b.abs() < 1e-5 && m.tx.abs() < 1e-3 && m.ty.abs() < 1e-3);
    }

    #[test]
    fn align_moves_marked_pixel_to_template() {
        // 2x kattalashtirilgan va siljitilgan yuz: burun nuqtasi atrofi oq
        let scale = 2.0;
        let lm = REF_5PTS.map(|p| [p[0] * scale + 50.0, p[1] * scale + 30.0]);
        let mut img = RgbImage::new(400, 400);
        let [nx, ny] = lm[2];
        for y in (ny as u32 - 3)..=(ny as u32 + 3) {
            for x in (nx as u32 - 3)..=(nx as u32 + 3) {
                img.put_pixel(x, y, image::Rgb([255, 255, 255]));
            }
        }
        let (out, _) = align(&img, &lm);
        let [rx, ry] = REF_5PTS[2];
        assert_eq!(out.get_pixel(rx.round() as u32, ry.round() as u32)[0], 255);
        assert_eq!(out.get_pixel(5, 5)[0], 0);
    }
}
