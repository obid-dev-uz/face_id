use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::admin;
use crate::db::repo::{self, Person, PersonInput};
use crate::error::AppResult;
use crate::face::{matcher::Matcher, FaceEngine};
use crate::state::{with_db, AppState};

/// Faol shaxslar o'zgarganda Matcher bazadan qayta to'ldiriladi (aks holda o'chirilgan
/// shaxs ham tanilib qolaveradi).
pub(crate) async fn reload_matcher(app: &AppHandle) -> AppResult<()> {
    let rows = with_db(app, |c| repo::load_embeddings(c, FaceEngine::MODEL_ID)).await?;
    *app.state::<AppState>().matcher.write() = Matcher::from_embeddings(FaceEngine::DIM, rows);
    Ok(())
}

#[tauri::command]
pub async fn list_persons(app: AppHandle, query: Option<String>, include_inactive: Option<bool>) -> AppResult<Vec<Person>> {
    admin::check(&app)?;
    with_db(&app, move |c| repo::list_persons(c, query.as_deref(), include_inactive.unwrap_or(false))).await
}

#[tauri::command]
pub async fn get_person(app: AppHandle, id: i64) -> AppResult<Person> {
    admin::check(&app)?;
    with_db(&app, move |c| repo::get_person(c, id)).await
}

/// Namunasiz shaxs yaratadi; namunalar 6-bosqichdagi ro'yxatga olish ustasida qo'shiladi.
#[tauri::command]
pub async fn create_person(app: AppHandle, input: PersonInput) -> AppResult<Person> {
    admin::act(&app)?;
    with_db(&app, move |c| {
        let p = repo::create_person(c, &input, &[])?;
        admin::log_event(c, "person_create", Some(&p.full_name))?;
        Ok(p)
    })
    .await
}

#[tauri::command]
pub async fn update_person(app: AppHandle, id: i64, input: PersonInput) -> AppResult<Person> {
    admin::act(&app)?;
    with_db(&app, move |c| {
        let p = repo::update_person(c, id, &input)?;
        admin::log_event(c, "person_update", Some(&p.full_name))?;
        Ok(p)
    })
    .await
}

#[tauri::command]
pub async fn set_person_active(app: AppHandle, id: i64, active: bool) -> AppResult<()> {
    admin::act(&app)?;
    with_db(&app, move |c| {
        repo::set_person_active(c, id, active)?;
        let name = repo::get_person(c, id)?.full_name;
        admin::log_event(c, if active { "person_activate" } else { "person_deactivate" }, Some(&name))
    })
    .await?;
    reload_matcher(&app).await
}

#[tauri::command]
pub async fn delete_person(app: AppHandle, id: i64) -> AppResult<()> {
    admin::act(&app)?;
    with_db(&app, move |c| {
        let name = repo::get_person(c, id)?.full_name;
        repo::delete_person(c, id)?;
        admin::log_event(c, "person_delete", Some(&name))
    })
    .await?;
    reload_matcher(&app).await
}

/// Barcha biometrik ma'lumotni o'chirish (shaxslar, namunalar, jurnal). Sozlamalar qoladi.
#[tauri::command]
pub async fn wipe_all_data(app: AppHandle) -> AppResult<()> {
    admin::act(&app)?;
    *app.state::<AppState>().enroll.lock() = None;
    with_db(&app, |c| {
        repo::wipe_all(c)?;
        admin::log_event(c, "wipe", None)
    })
    .await?;
    reload_matcher(&app).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SampleItem {
    id: i64,
    /// Tekislangan yuz, `data:` URL.
    thumb: Option<String>,
    quality: Option<f32>,
    created_at: String,
}

#[tauri::command]
pub async fn list_person_samples(app: AppHandle, person_id: i64) -> AppResult<Vec<SampleItem>> {
    admin::check(&app)?;
    let rows = with_db(&app, move |c| repo::list_samples(c, person_id)).await?;
    Ok(rows
        .into_iter()
        .map(|r| SampleItem {
            id: r.id,
            thumb: r.thumb.as_deref().map(crate::enroll::thumb_data_url),
            quality: r.quality,
            created_at: r.created_at,
        })
        .collect())
}

/// Bitta namunani o'chiradi (masalan, xira yoki boshqa odam tushib qolgan).
#[tauri::command]
pub async fn delete_person_sample(app: AppHandle, id: i64) -> AppResult<()> {
    admin::act(&app)?;
    with_db(&app, move |c| {
        repo::delete_embedding(c, id)?;
        admin::log_event(c, "sample_delete", None)
    })
    .await?;
    reload_matcher(&app).await
}
