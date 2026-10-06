use std::collections::BTreeMap;

use tauri::{AppHandle, Manager};

use crate::admin;
use crate::db::repo;
use crate::error::{AppError, AppResult};
use crate::state::{with_db, AppState};

#[tauri::command]
pub async fn get_settings(app: AppHandle) -> AppResult<BTreeMap<String, String>> {
    admin::check(&app)?;
    with_db(&app, |c| Ok(repo::get_settings(c)?.into_iter().filter(|(k, _)| !k.starts_with(admin::PRIVATE_PREFIX)).collect()))
        .await
}

#[tauri::command]
pub async fn set_setting(app: AppHandle, key: String, value: String) -> AppResult<()> {
    let valid = !key.is_empty() && key.len() <= 64 && key.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '.');
    if !valid || key.starts_with(admin::PRIVATE_PREFIX) {
        return Err(AppError::Invalid(format!("sozlama kaliti: {key}")));
    }
    admin::act(&app)?;
    if key == admin::SESSION_KEY {
        let min: u32 = value.parse().map_err(|_| AppError::Invalid(format!("{key}: {value}")))?;
        if !admin::SESSION_MIN_RANGE.contains(&min) {
            return Err(AppError::Invalid(format!("{key}: {value}")));
        }
        app.state::<AppState>().admin.set_timeout_min(min);
    }
    with_db(&app, move |c| {
        repo::set_setting(c, &key, &value)?;
        admin::log_event(c, "setting", Some(&format!("{key}={value}")))
    })
    .await?;
    // Yangi muddat shu zahoti amal qiladi
    admin::act(&app)
}
