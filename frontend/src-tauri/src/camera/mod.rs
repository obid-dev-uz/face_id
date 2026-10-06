//! Kamera manbalari. Har bir platforma kadrni [`FrameBus`] ga yozadi; oqim ishchisi
//! ([`stream`]) uni o'qiydi. Platformaga xos qism faqat kadr qanday olinishida.

#[cfg(all(desktop, feature = "desktop-camera"))]
pub mod desktop;
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

/// Bitta o'rinli pochta qutisi: faqat oxirgi qiymat saqlanadi. Sekin qurilmada navbat
/// to'planmaydi va kechikish o'smaydi.
pub struct Slot<T>(Arc<(Mutex<Option<T>>, Condvar)>);

impl<T> Clone for Slot<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> Default for Slot<T> {
    fn default() -> Self {
        Self(Arc::new((Mutex::new(None), Condvar::new())))
    }
}

impl<T> Slot<T> {
    pub fn put(&self, v: T) {
        *self.0 .0.lock() = Some(v);
        self.0 .1.notify_one();
    }

    /// Yangi qiymatni `timeout` gacha kutadi.
    pub fn wait_take(&self, timeout: Duration) -> Option<T> {
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

/// Kamera kadrlari: platforma manbasi yozadi, oqim ishchisi o'qiydi.
pub type FrameBus = Slot<Frame>;

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
