use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::db::repo::{self, Person, PersonInput};
use crate::enroll::{self, Duplicate, Pose, Session};
use crate::error::{AppError, AppResult};
use crate::state::{blocking, AppState};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SampleView {
    thumb: String,
    pose: Option<Pose>,
    score: f32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrollState {
    samples: Vec<SampleView>,
    target: Option<Pose>,
    /// Halqa to'liq qoplandi (namunalar soni oldindan belgilanmagan).
    complete: bool,
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

/// Yangi sessiya (avvalgisi bo'lsa, tashlab yuboriladi). `person_id` — mavjud shaxsga
/// namuna qo'shish.
#[tauri::command]
pub fn enroll_start(state: State<'_, AppState>, person_id: Option<i64>) {
    let mut s = Session::default();
    s.person_id = person_id;
    *state.enroll.lock() = Some(s);
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
                .map(|x| SampleView { thumb: enroll::thumb_data_url(&x.thumb_png), pose: x.pose, score: x.score })
                .collect();
            Ok((samples, s.target(), s.duplicate(&state.matcher.read(), threshold)))
        })?;
        let duplicate = match dup {
            Some((person_id, score)) => repo::get_person(&state.db.lock(), person_id)
                .ok()
                .map(|p| Duplicate { person_id, person_name: p.full_name, score }),
            None => None,
        };
        Ok(EnrollState { samples, complete: target.is_none(), target, duplicate })
    })
    .await
}

/// Ekrandagi tiniq doiraning kadrdagi joyi (ekran o'lchami o'zgarganda qayta yuboriladi).
#[tauri::command]
pub fn enroll_set_region(state: State<'_, AppState>, region: enroll::Region) -> AppResult<()> {
    with_session(&state, |s| {
        s.region = Some(region);
        Ok(())
    })
}

#[tauri::command]
pub fn enroll_remove_sample(state: State<'_, AppState>, index: usize) -> AppResult<()> {
    with_session(&state, |s| {
        s.remove(index);
        Ok(())
    })
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
