use std::collections::BTreeMap;

use tauri::AppHandle;

use crate::db::repo;
use crate::error::{AppError, AppResult};
use crate::state::with_db;

#[tauri::command]
pub async fn get_settings(app: AppHandle) -> AppResult<BTreeMap<String, String>> {
    with_db(&app, |c| Ok(repo::get_settings(c)?.into_iter().collect())).await
}

#[tauri::command]
pub async fn set_setting(app: AppHandle, key: String, value: String) -> AppResult<()> {
    let valid = !key.is_empty() && key.len() <= 64 && key.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '.');
    if !valid {
        return Err(AppError::Invalid(format!("sozlama kaliti: {key}")));
    }
    with_db(&app, move |c| repo::set_setting(c, &key, &value)).await
}
