use std::path::PathBuf;

use tauri::{AppHandle, Manager};

use crate::db::backup;
use crate::error::AppResult;
use crate::state::{with_db, AppState};

/// `dest` berilmasa, ilova papkasidagi `backups/` ga yoziladi. Yozilgan yo'lni qaytaradi.
/// Android'da foydalanuvchi tanlagan joyga (SAF) chiqarish UI bosqichida qo'shiladi.
#[tauri::command]
pub async fn backup_database(app: AppHandle, dest: Option<String>) -> AppResult<String> {
    let data_dir = app.state::<AppState>().data_dir.clone();
    let dest = dest.map(PathBuf::from).unwrap_or_else(|| backup::default_path(&data_dir, "khoface"));
    with_db(&app, move |c| {
        backup::backup_to(c, &dest)?;
        Ok(dest.display().to_string())
    })
    .await
}

/// Tiklashdan oldin joriy baza `backups/pre-restore-*.db` ga saqlanadi.
#[tauri::command]
pub async fn restore_database(app: AppHandle, src: String) -> AppResult<()> {
    let data_dir = app.state::<AppState>().data_dir.clone();
    let safety = backup::default_path(&data_dir, "pre-restore");
    with_db(&app, move |c| backup::restore_from(c, &PathBuf::from(src), &safety)).await
}
