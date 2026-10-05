use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
use rusqlite::Connection;

use crate::error::{AppError, AppResult};
use crate::face::{matcher::Matcher, FaceEngine};

/// Modellar fonda yuklanadi: eski telefonda bu bir necha soniya oladi va
/// ilova oynasi shu vaqtda ochiq turishi kerak.
pub enum EngineState {
    Loading,
    Ready(Arc<FaceEngine>),
    Failed(String),
}

pub struct AppState {
    pub db: Mutex<Connection>,
    pub data_dir: PathBuf,
    pub engine: RwLock<EngineState>,
    pub matcher: RwLock<Matcher>,
    pub stream: crate::camera::stream::Stream,
    /// Ro'yxatga olish ustasi ochiq bo'lsa — joriy sessiya.
    pub enroll: Mutex<Option<crate::enroll::Session>>,
}

impl AppState {
    pub fn new(db: Connection, data_dir: PathBuf) -> Self {
        Self {
            db: Mutex::new(db),
            data_dir,
            engine: RwLock::new(EngineState::Loading),
            matcher: RwLock::new(Matcher::new(FaceEngine::DIM)),
            stream: Default::default(),
            enroll: Mutex::new(None),
        }
    }

    pub fn engine(&self) -> AppResult<Arc<FaceEngine>> {
        match &*self.engine.read() {
            EngineState::Ready(e) => Ok(e.clone()),
            EngineState::Loading => Err(AppError::NotReady),
            EngineState::Failed(e) => Err(AppError::Engine(e.clone())),
        }
    }
}

/// Baza bilan ishlashni blocking thread'da bajaradi: Tauri'da `async` bo'lmagan
/// buyruqlar asosiy thread'da ishlaydi va eski telefonda UI qotib qolardi.
pub async fn with_db<T, F>(app: &tauri::AppHandle, f: F) -> AppResult<T>
where
    T: Send + 'static,
    F: FnOnce(&mut Connection) -> AppResult<T> + Send + 'static,
{
    use tauri::Manager;
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let mut conn = state.db.lock();
        f(&mut conn)
    })
    .await?
}

/// Og'ir hisoblash (inference) uchun blocking thread.
pub async fn blocking<T, F>(f: F) -> AppResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> AppResult<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f).await?
}
