use serde::Serialize;
use tauri::ipc::{InvokeBody, Request};
use tauri::{AppHandle, Manager, State};

use crate::db::repo::{self, Person, PersonInput};
use crate::enroll::{self, Duplicate, Hint, Pose, Session};
use crate::error::{AppError, AppResult};
use crate::state::{blocking, AppState};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SampleView {
    thumb: String,
    pose: Option<Pose>,
    ring: u8,
    score: f32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrollState {
    samples: Vec<SampleView>,
    target: Option<Pose>,
    required: usize,
    duplicate: Option<Duplicate>,
}

fn threshold(state: &AppState) -> AppResult<f32> {
    Ok(repo::get_setting(&state.db.lock(), "match.threshold")?
        .and_then(|v| v.parse().ok())
        .unwrap_or(crate::commands::engine::DEFAULT_THRESHOLD))
}

fn with_session<T>(state: &AppState, f: impl FnOnce(&mut Session) -> AppResult<T>) -> AppResult<T> {
    match state.enroll.lock().as_mut() {
        Some(s) => f(s),
        None => Err(AppError::Invalid("ro'yxatga olish boshlanmagan".into())),
    }
}

/// Yangi sessiya (avvalgisi bo'lsa, tashlab yuboriladi).
#[tauri::command]
pub fn enroll_start(state: State<'_, AppState>) {
    *state.enroll.lock() = Some(Session::default());
}

#[tauri::command]
pub fn enroll_cancel(state: State<'_, AppState>) {
    *state.enroll.lock() = None;
}

#[tauri::command]
pub async fn enroll_state(app: AppHandle) -> AppResult<EnrollState> {
    blocking(move || {
        let state = app.state::<AppState>();
        let threshold = threshold(&state)?;
        let (samples, target, dup) = with_session(&state, |s| {
            let samples = s
                .samples
                .iter()
                .map(|x| SampleView { thumb: enroll::thumb_data_url(&x.thumb_png), pose: x.pose, ring: x.ring, score: x.score })
                .collect();
            Ok((samples, s.target(), s.duplicate(&state.matcher.read(), threshold)))
        })?;
        let duplicate = match dup {
            Some((person_id, score)) => repo::get_person(&state.db.lock(), person_id)
                .ok()
                .map(|p| Duplicate { person_id, person_name: p.full_name, score }),
            None => None,
        };
        Ok(EnrollState { samples, target, required: enroll::TARGET_SAMPLES, duplicate })
    })
    .await
}

#[tauri::command]
pub fn enroll_remove_sample(state: State<'_, AppState>, index: usize) -> AppResult<()> {
    with_session(&state, |s| {
        s.remove(index);
        Ok(())
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageResult {
    accepted: bool,
    hint: Option<Hint>,
}

/// Galereyadan rasm: `invoke("enroll_add_image", uint8Array)`.
#[tauri::command]
pub async fn enroll_add_image(app: AppHandle, request: Request<'_>) -> AppResult<ImageResult> {
    let bytes = match request.body() {
        InvokeBody::Raw(b) if !b.is_empty() => b.clone(),
        _ => return Err(AppError::Invalid("rasm baytlari kutilgan edi".into())),
    };
    blocking(move || {
        let state = app.state::<AppState>();
        let engine = state.engine()?;
        let img = crate::commands::engine::decode_image(&bytes)?;
        let r = with_session(&state, |s| s.add_image(&engine, &img))?;
        Ok(ImageResult { accepted: r.is_ok(), hint: r.err() })
    })
    .await
}

/// Saqlash. Bazada o'xshash shaxs bo'lsa, `allow_duplicate` bo'lmaguncha rad etiladi.
#[tauri::command]
pub async fn enroll_commit(app: AppHandle, input: PersonInput, allow_duplicate: Option<bool>) -> AppResult<Person> {
    blocking(move || {
        let state = app.state::<AppState>();
        let threshold = threshold(&state)?;
        let mut guard = state.enroll.lock();
        let session = guard.as_ref().ok_or_else(|| AppError::Invalid("ro'yxatga olish boshlanmagan".into()))?;
        let mut matcher = state.matcher.write();
        if !allow_duplicate.unwrap_or(false) && session.duplicate(&matcher, threshold).is_some() {
            return Err(AppError::Invalid("bu shaxs bazada bor".into()));
        }
        let person = session.commit(&mut state.db.lock(), &mut matcher, &input)?;
        *guard = None;
        Ok(person)
    })
    .await
}
