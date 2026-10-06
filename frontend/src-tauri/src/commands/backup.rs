use std::path::PathBuf;

use tauri::{AppHandle, Manager};

use crate::admin;
use crate::db::backup;
use crate::error::AppResult;
use crate::state::{with_db, AppState};

/// `dest` berilmasa, ilova papkasidagi `backups/` ga yoziladi. Yozilgan yo'lni qaytaradi.
/// Android'da foydalanuvchi tanlagan joyga (SAF) chiqarish UI bosqichida qo'shiladi.
#[tauri::command]
pub async fn backup_database(app: AppHandle, dest: Option<String>) -> AppResult<String> {
    admin::act(&app)?;
    let data_dir = app.state::<AppState>().data_dir.clone();
    let dest = dest.map(PathBuf::from).unwrap_or_else(|| backup::default_path(&data_dir, "khoface"));
    with_db(&app, move |c| {
        backup::backup_to(c, &dest)?;
        admin::log_event(c, "backup", None)?;
        Ok(dest.display().to_string())
    })
    .await
}

/// Tiklashdan oldin joriy baza `backups/pre-restore-*.db` ga saqlanadi.
#[tauri::command]
pub async fn restore_database(app: AppHandle, src: String) -> AppResult<()> {
    admin::act(&app)?;
    let data_dir = app.state::<AppState>().data_dir.clone();
    let safety = backup::default_path(&data_dir, "pre-restore");
    *app.state::<AppState>().enroll.lock() = None;
    with_db(&app, move |c| {
        // Joriy PIN va urinishlar hisobi saqlanadi: zaxiradagi eski PIN qaytib kelmasin.
        let private = admin::private_settings(c)?;
        backup::restore_from(c, &PathBuf::from(src), &safety)?;
        admin::put_private_settings(c, &private)?;
        admin::log_event(c, "restore", None)
    })
    .await?;
    crate::commands::persons::reload_matcher(&app).await
}
