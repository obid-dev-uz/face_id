//! YUV_420_888 → RGB, bir vaqtda tik holatga burish bilan. BT.601 to'liq diapazon
//! (Android kamerasi JPEG/JFIF bilan bir xil). Fixed-point: eski ARM'da ham tez.

use image::RgbImage;

pub struct YuvPlanes<'a> {
    pub width: usize,
    pub height: usize,
    pub y: &'a [u8],
    pub u: &'a [u8],
    pub v: &'a [u8],
    pub y_stride: usize,
    pub uv_stride: usize,
    pub uv_pixel_stride: usize,
}

#[inline]
fn clamp(v: i32) -> u8 {
    v.clamp(0, 255) as u8
}

#[inline]
fn to_rgb(y: u8, u: u8, v: u8) -> [u8; 3] {
    let (y, u, v) = ((y as i32) << 10, u as i32 - 128, v as i32 - 128);
    // 1.402, 0.344136, 0.714136, 1.772 × 1024
    [
        clamp((y + 1436 * v + 512) >> 10),
        clamp((y - 352 * u - 731 * v + 512) >> 10),
        clamp((y + 1815 * u + 512) >> 10),
    ]
}

/// `rotation` — soat yo'nalishida 0/90/180/270. `step` — har `step` pikselda bittasi
/// olinadi (2 = ikki baravar kichraytirish, live oqim uchun arzon).
pub fn to_rgb_rotated(p: &YuvPlanes, rotation: u32, step: usize) -> RgbImage {
    let step = step.max(1);
    let (sw, sh) = (p.width / step, p.height / step);
    let (dw, dh) = if rotation % 180 == 90 { (sh, sw) } else { (sw, sh) };
    let mut out = RgbImage::new(dw as u32, dh as u32);
    let buf: &mut [u8] = &mut out;
    let get = |plane: &[u8], i: usize| plane.get(i).copied().unwrap_or(128);
    for sy in 0..sh {
        let (y0, uvr) = (sy * step, (sy * step / 2) * p.uv_stride);
        for sx in 0..sw {
            let x0 = sx * step;
            let ui = uvr + (x0 / 2) * p.uv_pixel_stride;
            let rgb = to_rgb(get(p.y, y0 * p.y_stride + x0), get(p.u, ui), get(p.v, ui));
            let (dx, dy) = match rotation {
                90 => (sh - 1 - sy, sx),
                180 => (sw - 1 - sx, sh - 1 - sy),
                270 => (sy, sw - 1 - sx),
                _ => (sx, sy),
            };
            let o = (dy * dw + dx) * 3;
            buf[o..o + 3].copy_from_slice(&rgb);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 4×2 kadr uchun neytral (kulrang) U/V: 2 ta UV ustun, 1 qator.
    fn neutral_uv(uv_pixel_stride: usize) -> (Vec<u8>, Vec<u8>) {
        (vec![128; 2 * uv_pixel_stride], vec![128; 2 * uv_pixel_stride])
    }

    fn frame<'a>(y: &'a [u8], u: &'a [u8], v: &'a [u8], ps: usize) -> YuvPlanes<'a> {
        YuvPlanes { width: 4, height: 2, y, u, v, y_stride: 4, uv_stride: 2 * ps, uv_pixel_stride: ps }
    }

    fn lum(img: &RgbImage) -> Vec<u8> {
        img.pixels().map(|p| p[0]).collect()
    }

    #[test]
    fn rotations_place_pixels_correctly() {
        let y = [1, 2, 3, 4, 5, 6, 7, 8];
        let (u, v) = neutral_uv(1);
        let f = frame(&y, &u, &v, 1);
        assert_eq!(lum(&to_rgb_rotated(&f, 0, 1)), [1, 2, 3, 4, 5, 6, 7, 8]);
        // 90°: chap ustun yuqori qatorga aylanadi (pastdan yuqoriga)
        let r90 = to_rgb_rotated(&f, 90, 1);
        assert_eq!(r90.dimensions(), (2, 4));
        assert_eq!(lum(&r90), [5, 1, 6, 2, 7, 3, 8, 4]);
        assert_eq!(lum(&to_rgb_rotated(&f, 180, 1)), [8, 7, 6, 5, 4, 3, 2, 1]);
        assert_eq!(lum(&to_rgb_rotated(&f, 270, 1)), [4, 8, 3, 7, 2, 6, 1, 5]);
    }

    #[test]
    fn interleaved_uv_and_colors() {
        // pixelStride 2 (NV21 kabi): U va V buferlari aralash
        let y = [76u8; 8];
        let u = [85u8, 0, 85, 0];
        let v = [255u8, 0, 255, 0];
        let img = to_rgb_rotated(&frame(&y, &u, &v, 2), 0, 1);
        let p = img.get_pixel(3, 1);
        assert!(p[0] > 240 && p[1] < 10 && p[2] < 10, "qizil kutilgan: {p:?}");
    }

    #[test]
    fn downscale_step() {
        let y = [1, 2, 3, 4, 5, 6, 7, 8];
        let (u, v) = neutral_uv(1);
        let img = to_rgb_rotated(&frame(&y, &u, &v, 1), 0, 2);
        assert_eq!(lum(&img), [1, 3]);
    }
}
