use crate::{Error, Result};
use argon2::{
    Argon2, PasswordHasher, PasswordVerifier,
    password_hash::{PasswordHash, SaltString},
};
use axum::http::HeaderMap;
use serde::{Deserialize, Serialize};
#[derive(Clone)]
pub struct Auth {
    secret: Vec<u8>,
}
#[derive(Serialize, Deserialize)]
struct Claims {
    sub: String,
    exp: u64,
}
impl Auth {
    pub fn new(secret: &str) -> Self {
        Self {
            secret: secret.as_bytes().to_vec(),
        }
    }
    pub fn token(&self, user: String) -> Result<String> {
        jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &Claims {
                sub: user,
                exp: (crate::now() / 1000 + 86400 * 7) as u64,
            },
            &jsonwebtoken::EncodingKey::from_secret(&self.secret),
        )
        .map_err(|e| Error::Internal(e.into()))
    }
    pub fn user(&self, h: &HeaderMap) -> Result<String> {
        let token = h
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or(Error::Unauthorized)?;
        jsonwebtoken::decode::<Claims>(
            token,
            &jsonwebtoken::DecodingKey::from_secret(&self.secret),
            &jsonwebtoken::Validation::default(),
        )
        .map(|d| d.claims.sub)
        .map_err(|_| Error::Unauthorized)
    }
}
pub async fn hash(password: String) -> Result<String> {
    tokio::task::spawn_blocking(move || {
        Argon2::default()
            .hash_password(
                password.as_bytes(),
                &SaltString::generate(&mut rand_core::OsRng),
            )
            .map(|p| p.to_string())
            .map_err(|e| Error::Internal(anyhow::anyhow!(e.to_string())))
    })
    .await
    .map_err(|e| Error::Internal(e.into()))?
}
pub async fn verify(password: String, hash: String) -> Result<()> {
    tokio::task::spawn_blocking(move || {
        let hash = PasswordHash::new(&hash).map_err(|_| Error::Unauthorized)?;
        Argon2::default()
            .verify_password(password.as_bytes(), &hash)
            .map_err(|_| Error::Unauthorized)
    })
    .await
    .map_err(|e| Error::Internal(e.into()))?
}
