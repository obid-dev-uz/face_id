use tauri::AppHandle;

use crate::db::repo::{self, Person, PersonInput};
use crate::error::AppResult;
use crate::state::with_db;

#[tauri::command]
pub async fn list_persons(app: AppHandle, query: Option<String>, include_inactive: Option<bool>) -> AppResult<Vec<Person>> {
    with_db(&app, move |c| repo::list_persons(c, query.as_deref(), include_inactive.unwrap_or(false))).await
}

#[tauri::command]
pub async fn get_person(app: AppHandle, id: i64) -> AppResult<Person> {
    with_db(&app, move |c| repo::get_person(c, id)).await
}

/// Namunasiz shaxs yaratadi; namunalar 6-bosqichdagi ro'yxatga olish ustasida qo'shiladi.
#[tauri::command]
pub async fn create_person(app: AppHandle, input: PersonInput) -> AppResult<Person> {
    with_db(&app, move |c| repo::create_person(c, &input, &[])).await
}

#[tauri::command]
pub async fn update_person(app: AppHandle, id: i64, input: PersonInput) -> AppResult<Person> {
    with_db(&app, move |c| repo::update_person(c, id, &input)).await
}

#[tauri::command]
pub async fn set_person_active(app: AppHandle, id: i64, active: bool) -> AppResult<()> {
    with_db(&app, move |c| repo::set_person_active(c, id, active)).await
}

#[tauri::command]
pub async fn delete_person(app: AppHandle, id: i64) -> AppResult<()> {
    with_db(&app, move |c| repo::delete_person(c, id)).await
}
