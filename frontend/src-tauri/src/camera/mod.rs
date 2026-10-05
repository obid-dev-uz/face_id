//! Kamera manbalari. Har bir platforma kadrni [`FrameBus`] ga yozadi; oqim ishchisi
//! ([`stream`]) uni o'qiydi. Platformaga xos qism faqat kadr qanday olinishida.

pub mod stream;
pub mod yuv;

use std::time::{Duration, Instant};

use image::RgbImage;
use parking_lot::{Condvar, Mutex};
use std::sync::Arc;

pub use tauri_plugin_khoface_camera::Facing;

/// Tik holatga keltirilgan RGB kadr (ko'zgu aksi qilinmagan — ML uchun).
pub struct Frame {
    pub rgb: RgbImage,
    pub facing: Facing,
    pub at: Instant,
}

/// Faqat oxirgi kadr saqlanadi: sekin qurilmada navbat to'planmaydi va kechikish o'smaydi.
#[derive(Clone, Default)]
pub struct FrameBus(Arc<(Mutex<Option<Frame>>, Condvar)>);

impl FrameBus {
    pub fn put(&self, f: Frame) {
        *self.0 .0.lock() = Some(f);
        self.0 .1.notify_one();
    }

    /// Yangi kadrni `timeout` gacha kutadi.
    pub fn wait_take(&self, timeout: Duration) -> Option<Frame> {
        let mut slot = self.0 .0.lock();
        if slot.is_none() {
            self.0 .1.wait_for(&mut slot, timeout);
        }
        slot.take()
    }

    pub fn clear(&self) {
        *self.0 .0.lock() = None;
    }
}

#[cfg(target_os = "android")]
pub mod android {
    use std::time::Instant;

    use tauri_plugin_khoface_camera::{set_frame_sink, YuvFrame};

    use super::{yuv, Frame, FrameBus};

    /// CameraX kadrlarini (analyzer thread'da) RGB'ga o'girib, FrameBus'ga yozadi.
    pub fn attach(bus: FrameBus) {
        set_frame_sink(Some(Box::new(move |f: YuvFrame<'_>| {
            let planes = yuv::YuvPlanes {
                width: f.width as usize,
                height: f.height as usize,
                y: f.y,
                u: f.u,
                v: f.v,
                y_stride: f.y_stride,
                uv_stride: f.uv_stride,
                uv_pixel_stride: f.uv_pixel_stride,
            };
            let rgb = yuv::to_rgb_rotated(&planes, f.rotation, 1);
            bus.put(Frame { rgb, facing: f.facing, at: Instant::now() });
        })));
    }

    pub fn detach() {
        set_frame_sink(None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bus_keeps_only_latest() {
        let bus = FrameBus::default();
        for w in 1..=3 {
            bus.put(Frame { rgb: RgbImage::new(w, 1), facing: Facing::Front, at: Instant::now() });
        }
        assert_eq!(bus.wait_take(Duration::ZERO).unwrap().rgb.width(), 3);
        assert!(bus.wait_take(Duration::from_millis(5)).is_none());
    }

    #[test]
    fn bus_wakes_waiter() {
        let bus = FrameBus::default();
        let b2 = bus.clone();
        let t = std::thread::spawn(move || b2.wait_take(Duration::from_secs(5)).map(|f| f.rgb.width()));
        std::thread::sleep(Duration::from_millis(20));
        bus.put(Frame { rgb: RgbImage::new(7, 1), facing: Facing::Back, at: Instant::now() });
        assert_eq!(t.join().unwrap(), Some(7));
    }
}
