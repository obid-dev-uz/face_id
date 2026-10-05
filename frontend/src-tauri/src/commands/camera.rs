use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, State};
use tauri_plugin_khoface_camera::{CameraAvailability, CameraExt, Facing, PermissionState};

use crate::error::{AppError, AppResult};
use crate::state::{blocking, AppState};

impl From<tauri_plugin_khoface_camera::Error> for AppError {
    fn from(e: tauri_plugin_khoface_camera::Error) -> Self {
        AppError::Camera(e.to_string())
    }
}

#[tauri::command]
pub async fn camera_permission(app: AppHandle) -> AppResult<PermissionState> {
    blocking(move || Ok(app.camera().permission()?)).await
}

/// Tizim so'rov oynasi yopilgunicha kutadi.
#[tauri::command]
pub async fn request_camera_permission(app: AppHandle) -> AppResult<PermissionState> {
    blocking(move || Ok(app.camera().request_permission()?)).await
}

#[tauri::command]
pub async fn open_app_settings(app: AppHandle) -> AppResult<()> {
    blocking(move || Ok(app.camera().open_settings()?)).await
}

#[tauri::command]
pub async fn camera_availability(app: AppHandle) -> AppResult<CameraAvailability> {
    #[cfg(all(desktop, feature = "desktop-camera"))]
    return blocking(move || {
        let _ = app;
        let n = crate::camera::desktop::camera_count();
        Ok(CameraAvailability { front: n > 0, back: n > 1 })
    })
    .await;
    #[cfg(not(all(desktop, feature = "desktop-camera")))]
    blocking(move || Ok(app.camera().availability()?)).await
}

#[tauri::command]
pub async fn start_stream(app: AppHandle, facing: Facing, on_frame: Channel<InvokeResponseBody>) -> AppResult<()> {
    blocking(move || {
        let state = tauri::Manager::state::<AppState>(&app);
        state.stream.start(app.clone(), on_frame);
        #[cfg(target_os = "android")]
        crate::camera::android::attach(state.stream.bus.clone());
        #[cfg(all(desktop, feature = "desktop-camera"))]
        let started = crate::camera::desktop::start(state.stream.bus.clone(), facing);
        #[cfg(not(all(desktop, feature = "desktop-camera")))]
        let started = app.camera().start(facing, 640, 480).map_err(AppError::from);
        if let Err(e) = started {
            state.stream.stop_worker();
            return Err(e);
        }
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn stop_stream(app: AppHandle) -> AppResult<()> {
    blocking(move || {
        let state = tauri::Manager::state::<AppState>(&app);
        #[cfg(all(desktop, feature = "desktop-camera"))]
        crate::camera::desktop::stop();
        #[cfg(not(all(desktop, feature = "desktop-camera")))]
        app.camera().stop()?;
        #[cfg(target_os = "android")]
        crate::camera::android::detach();
        state.stream.stop_worker();
        Ok(())
    })
    .await
}

/// Frontend kadrni chizib bo'lganda chaqiradi (backpressure).
#[tauri::command]
pub fn camera_ack(state: State<'_, AppState>) {
    state.stream.ack();
}
