use tauri::AppHandle;

use crate::db::repo::{self, LogEntry};
use crate::error::AppResult;
use crate::state::with_db;

#[tauri::command]
pub async fn list_logs(app: AppHandle, before_id: Option<i64>, limit: Option<u32>) -> AppResult<Vec<LogEntry>> {
    with_db(&app, move |c| repo::list_logs(c, before_id, limit.unwrap_or(50))).await
}

#[tauri::command]
pub async fn purge_logs(app: AppHandle, older_than_days: u32) -> AppResult<usize> {
    with_db(&app, move |c| repo::purge_logs_older_than(c, older_than_days)).await
}
