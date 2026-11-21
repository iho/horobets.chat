pub mod api;
pub mod auth;
pub mod db;
pub mod media;
pub mod push;
pub mod storage;
pub mod ws;
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
use std::{path::PathBuf, sync::Arc};
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Bad(String),
    #[error("authentication required")]
    Unauthorized,
    #[error("access denied")]
    Forbidden,
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    Internal(#[from] anyhow::Error),
}
pub type Result<T> = std::result::Result<T, Error>;
impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        if let rusqlite::Error::SqliteFailure(ref code, _) = e
            && code.code == rusqlite::ErrorCode::ConstraintViolation
        {
            return Self::Bad("conflicting or invalid data".into());
        }
        Self::Internal(e.into())
    }
}
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            Self::Bad(m) => (StatusCode::BAD_REQUEST, m),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, self.to_string()),
            Self::Forbidden => (StatusCode::FORBIDDEN, self.to_string()),
            Self::NotFound => (StatusCode::NOT_FOUND, self.to_string()),
            Self::Internal(ref e) => {
                tracing::error!(error=?e,"request failed");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error".into())
            }
        };
        (status, Json(json!({"error":msg}))).into_response()
    }
}
pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
pub fn id() -> String {
    uuid::Uuid::now_v7().to_string()
}
#[derive(Clone)]
pub struct App {
    pub db: Arc<db::manager::Manager>,
    pub auth: auth::Auth,
    pub push: Arc<push::Push>,
    pub blobs: Arc<dyn storage::BlobStore>,
}
impl App {
    pub fn open(data: PathBuf, secret: &str) -> anyhow::Result<Self> {
        anyhow::ensure!(
            secret.len() >= 32,
            "JWT secret must contain at least 32 bytes"
        );
        std::fs::create_dir_all(&data)?;
        Ok(Self {
            db: Arc::new(db::manager::Manager::open(&data)?),
            auth: auth::Auth::new(secret),
            push: Arc::new(push::Push::from_env()?),
            blobs: Arc::new(storage::LocalFsStore::new(data.join("blobs"))?),
        })
    }
}
