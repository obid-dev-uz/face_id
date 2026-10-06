use tauri::AppHandle;

use crate::admin::{self, AdminEvent, Status, Verdict};
use crate::error::AppResult;
use crate::state::{blocking, with_db};

#[tauri::command]
pub async fn admin_status(app: AppHandle) -> AppResult<Status> {
    blocking(move || admin::status(&app)).await
}

/// PIN hali o'rnatilmagan bo'lsagina ishlaydi.
#[tauri::command]
pub async fn admin_setup(app: AppHandle, pin: String) -> AppResult<()> {
    blocking(move || admin::setup(&app, &pin)).await
}

#[tauri::command]
pub async fn admin_unlock(app: AppHandle, pin: String) -> AppResult<Verdict> {
    blocking(move || admin::unlock(&app, &pin)).await
}

#[tauri::command]
pub fn admin_lock(app: AppHandle) {
    admin::close_session(&app, "logout");
}

/// Foydalanuvchi ekranga tekkanda (frontend ko'pi bilan 10 soniyada bir marta yuboradi).
#[tauri::command]
pub async fn admin_touch(app: AppHandle) -> AppResult<Status> {
    blocking(move || {
        admin::act(&app)?;
        admin::status(&app)
    })
    .await
}

#[tauri::command]
pub async fn admin_change_pin(app: AppHandle, old_pin: String, new_pin: String) -> AppResult<Verdict> {
    blocking(move || admin::change_pin(&app, &old_pin, &new_pin)).await
}

/// PIN unutilganda (mehmon rejimidan): hammasi o'chiriladi, keyin yangi PIN o'rnatiladi.
#[tauri::command]
pub async fn admin_reset(app: AppHandle) -> AppResult<()> {
    with_db(&app, admin::reset_all).await?;
    admin::close_session(&app, "reset");
    crate::commands::persons::reload_matcher(&app).await
}

#[tauri::command]
pub async fn list_admin_events(app: AppHandle, limit: Option<u32>) -> AppResult<Vec<AdminEvent>> {
    admin::check(&app)?;
    with_db(&app, move |c| admin::list_events(c, limit.unwrap_or(100))).await
}
