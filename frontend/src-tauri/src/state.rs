use std::path::PathBuf;

use parking_lot::Mutex;
use rusqlite::Connection;

pub struct AppState {
    pub db: Mutex<Connection>,
    pub data_dir: PathBuf,
}

impl AppState {
    pub fn new(db: Connection, data_dir: PathBuf) -> Self {
        Self { db: Mutex::new(db), data_dir }
    }
}

/// Baza bilan ishlashni blocking thread'da bajaradi: Tauri'da `async` bo'lmagan
/// buyruqlar asosiy thread'da ishlaydi va eski telefonda UI qotib qolardi.
pub async fn with_db<T, F>(app: &tauri::AppHandle, f: F) -> crate::error::AppResult<T>
where
    T: Send + 'static,
    F: FnOnce(&mut Connection) -> crate::error::AppResult<T> + Send + 'static,
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
