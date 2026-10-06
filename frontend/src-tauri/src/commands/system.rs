use serde::Serialize;
use tauri::State;

use crate::{db, error::AppResult, state::AppState};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    version: &'static str,
    os: &'static str,
    arch: &'static str,
    data_dir: String,
    sqlite_version: &'static str,
    schema_version: u32,
    /// Qurilmadagi umumiy RAM (MB); ≤ 2 GB bo'lsa, tahlil kichikroq kirish bilan ishlaydi.
    total_ram_mb: Option<u64>,
}

/// Diagnostika: qurilmada skelet to'g'ri yig'ilganini tekshirish uchun.
#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> AppResult<AppInfo> {
    let schema_version = db::schema_version(&state.db.lock())?;
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        data_dir: state.data_dir.display().to_string(),
        sqlite_version: rusqlite::version(),
        schema_version,
        total_ram_mb: crate::engine::total_ram_mb(),
    })
}

/// Skrinshot taqiqi (Android FLAG_SECURE). Sozlama saqlanadi; ilova ochilganda frontend
/// `sync_secure_screen` bilan uni qayta qo'llaydi (oyna har safar yangidan yaratiladi).
/// Admin rejimida esa sozlamadan qat'i nazar yoqiq.
#[tauri::command]
pub async fn set_secure_screen(app: tauri::AppHandle, enabled: bool) -> AppResult<()> {
    crate::admin::act(&app)?;
    let value = if enabled { "1" } else { "0" };
    crate::state::with_db(&app, move |c| db::repo::set_setting(c, "privacy.secure_screen", value)).await?;
    crate::admin::apply_secure(&app);
    Ok(())
}

/// Ilova ochilganda: sozlama va admin rejimiga ko'ra FLAG_SECURE qayta qo'llanadi.
#[tauri::command]
pub fn sync_secure_screen(app: tauri::AppHandle) {
    crate::admin::apply_secure(&app);
}
