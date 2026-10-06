use tauri::AppHandle;

use crate::db::repo::{self, LogEntry};
use crate::error::AppResult;
use crate::state::with_db;

#[tauri::command]
pub async fn list_logs(app: AppHandle, before_id: Option<i64>, limit: Option<u32>) -> AppResult<Vec<LogEntry>> {
    with_db(&app, move |c| repo::list_logs(c, before_id, limit.unwrap_or(50))).await
}

/// `data:` URL yoki `None` (rasm saqlanmagan).
#[tauri::command]
pub async fn log_snapshot(app: AppHandle, id: i64) -> AppResult<Option<String>> {
    use base64::Engine as _;
    let jpeg = with_db(&app, move |c| repo::get_log_snapshot(c, id)).await?;
    Ok(jpeg.map(|b| format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(b))))
}

#[tauri::command]
pub async fn purge_logs(app: AppHandle, older_than_days: u32) -> AppResult<usize> {
    with_db(&app, move |c| repo::purge_logs_older_than(c, older_than_days)).await
}
