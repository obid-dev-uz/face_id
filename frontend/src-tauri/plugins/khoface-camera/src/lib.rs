//! KHOFace kamera plagini. Android'da CameraX `ImageAnalysis` kadrlari (YUV_420_888)
//! JNI orqali to'g'ridan-to'g'ri Rust'ga beriladi — WebView orqali o'tmaydi.
//! Kadrni qabul qiluvchi funksiya [`set_frame_sink`] bilan o'rnatiladi.

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tauri::{
    plugin::{Builder, TauriPlugin},
    Manager, Runtime,
};

#[cfg(target_os = "android")]
mod android;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("kamera bu platformada hali qo'llab-quvvatlanmaydi")]
    Unsupported,
    #[cfg(mobile)]
    #[error(transparent)]
    Plugin(#[from] tauri::plugin::mobile::PluginInvokeError),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Facing {
    Front,
    Back,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PermissionState {
    Granted,
    Denied,
    Prompt,
    PromptWithRationale,
}

/// Ishga tushgan kamera haqida (diagnostika).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraInfo {
    /// Camera2 darajasi: "legacy" | "limited" | "full" | "level3" | "external" | "unknown".
    pub hardware_level: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraAvailability {
    pub front: bool,
    pub back: bool,
}

/// Kamera kadri: YUV_420_888 tekisliklari, CameraX buferlariga to'g'ridan-to'g'ri ishora qiladi.
/// Faqat callback ichida yaroqli — kerakli ma'lumotni nusxalab oling.
pub struct YuvFrame<'a> {
    pub width: u32,
    pub height: u32,
    /// Kadrni tik holatga keltirish uchun soat yo'nalishida burish (0/90/180/270).
    pub rotation: u32,
    pub facing: Facing,
    pub y: &'a [u8],
    pub u: &'a [u8],
    pub v: &'a [u8],
    pub y_stride: usize,
    pub uv_stride: usize,
    /// 1 (planar) yoki 2 (U/V aralash, NV21/NV12 kabi).
    pub uv_pixel_stride: usize,
}

type Sink = Box<dyn Fn(YuvFrame<'_>) + Send + Sync>;
static SINK: RwLock<Option<Sink>> = RwLock::new(None);

/// Kadrlar kamera oqimida (CameraX analyzer thread) chaqiriladi; sekin ishlov
/// berilsa, CameraX eski kadrlarni o'zi tashlab yuboradi (KEEP_ONLY_LATEST).
pub fn set_frame_sink(sink: Option<Sink>) {
    *SINK.write() = sink;
}

#[allow(dead_code)]
fn deliver(frame: YuvFrame<'_>) {
    if let Some(sink) = SINK.read().as_ref() {
        sink(frame);
    }
}

pub struct Camera<R: Runtime> {
    #[cfg(target_os = "android")]
    handle: tauri::plugin::PluginHandle<R>,
    #[cfg(not(target_os = "android"))]
    _r: std::marker::PhantomData<fn() -> R>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct StartArgs {
    facing: Facing,
    width: u32,
    height: u32,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct PermissionResponse {
    camera: PermissionState,
}

#[cfg(target_os = "android")]
impl<R: Runtime> Camera<R> {
    pub fn start(&self, facing: Facing, width: u32, height: u32) -> Result<CameraInfo> {
        Ok(self.handle.run_mobile_plugin("start", StartArgs { facing, width, height })?)
    }

    pub fn stop(&self) -> Result<()> {
        Ok(self.handle.run_mobile_plugin("stop", ())?)
    }

    pub fn availability(&self) -> Result<CameraAvailability> {
        Ok(self.handle.run_mobile_plugin("availability", ())?)
    }

    pub fn permission(&self) -> Result<PermissionState> {
        Ok(self.handle.run_mobile_plugin::<PermissionResponse>("checkPermissions", ())?.camera)
    }

    /// Tizim oynasini ko'rsatadi va foydalanuvchi javobini kutadi.
    pub fn request_permission(&self) -> Result<PermissionState> {
        #[derive(Serialize)]
        struct Args {
            permissions: [&'static str; 1],
        }
        let r: PermissionResponse = self.handle.run_mobile_plugin("requestPermissions", Args { permissions: ["camera"] })?;
        Ok(r.camera)
    }

    /// Ruxsat butunlay rad etilganda: ilova sozlamalarini ochish.
    pub fn open_settings(&self) -> Result<()> {
        Ok(self.handle.run_mobile_plugin("openSettings", ())?)
    }

    /// FLAG_SECURE: ilova oynasi skrinshot, ekran yozuvi va "so'nggi ilovalar" ko'rinishida yashiriladi.
    pub fn set_secure(&self, enabled: bool) -> Result<()> {
        #[derive(Serialize)]
        struct Args {
            enabled: bool,
        }
        Ok(self.handle.run_mobile_plugin("setSecure", Args { enabled })?)
    }
}

#[cfg(not(target_os = "android"))]
impl<R: Runtime> Camera<R> {
    pub fn start(&self, _facing: Facing, _width: u32, _height: u32) -> Result<CameraInfo> {
        Err(Error::Unsupported)
    }
    pub fn stop(&self) -> Result<()> {
        Ok(())
    }
    pub fn availability(&self) -> Result<CameraAvailability> {
        Ok(CameraAvailability { front: false, back: false })
    }
    pub fn permission(&self) -> Result<PermissionState> {
        Ok(PermissionState::Granted)
    }
    pub fn request_permission(&self) -> Result<PermissionState> {
        Ok(PermissionState::Granted)
    }
    pub fn open_settings(&self) -> Result<()> {
        Err(Error::Unsupported)
    }
    pub fn set_secure(&self, _enabled: bool) -> Result<()> {
        Ok(())
    }
}

pub trait CameraExt<R: Runtime> {
    fn camera(&self) -> &Camera<R>;
}

impl<R: Runtime, T: Manager<R>> CameraExt<R> for T {
    fn camera(&self) -> &Camera<R> {
        self.state::<Camera<R>>().inner()
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("khoface-camera")
        .setup(|app, _api| {
            #[cfg(target_os = "android")]
            let camera = Camera { handle: _api.register_android_plugin("kho.face.camera", "CameraPlugin")? };
            #[cfg(not(target_os = "android"))]
            let camera = Camera::<R> { _r: std::marker::PhantomData };
            app.manage(camera);
            Ok(())
        })
        .build()
}
