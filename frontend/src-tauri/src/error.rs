use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("baza xatosi: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("fayl xatosi: {0}")]
    Io(#[from] std::io::Error),
    #[error("tauri xatosi: {0}")]
    Tauri(#[from] tauri::Error),
    #[error("topilmadi: {0}")]
    NotFound(&'static str),
    #[error("noto'g'ri qiymat: {0}")]
    Invalid(String),
}

/// Frontend `invoke` xatoni oddiy matn sifatida oladi.
impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
