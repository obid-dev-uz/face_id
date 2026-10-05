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
    })
}
