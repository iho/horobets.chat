use crate::cache::Cache;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use shared_proto::ServerEvent;
#[derive(Clone, Serialize, Deserialize)]
pub struct Session {
    pub server: String,
    pub user_id: String,
    pub token: String,
}
impl Session {
    pub fn url(&self, path: &str) -> String {
        format!("{}/api/v1{path}", self.server)
    }
    pub fn get(&self, http: &reqwest::Client, path: &str) -> reqwest::RequestBuilder {
        http.get(self.url(path)).bearer_auth(&self.token)
    }
    pub fn post(&self, http: &reqwest::Client, path: &str) -> reqwest::RequestBuilder {
        http.post(self.url(path)).bearer_auth(&self.token)
    }
}
pub async fn json<T: serde::de::DeserializeOwned>(request: reqwest::RequestBuilder) -> Result<T> {
    let response = request.send().await.context("Cannot reach server")?;
    let status = response.status();
    let bytes = response.bytes().await?;
    if !status.is_success() {
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_default();
        anyhow::bail!("{}", v["error"].as_str().unwrap_or(status.as_str()))
    }
    Ok(serde_json::from_slice(&bytes)?)
}
#[derive(Deserialize)]
struct SyncPage {
    events: Vec<SyncEvent>,
    next_seq: i64,
    has_more: bool,
}
#[derive(Deserialize)]
struct SyncEvent {
    event: ServerEvent,
}
/// The HTTP event log is authoritative. WS frames wake sync rather than replaying
/// unsequenced live frames over newer history (which could resurrect edited content).
pub async fn backfill(
    http: &reqwest::Client,
    session: &Session,
    w: &str,
    cache: &mut Cache,
) -> Result<()> {
    loop {
        let cursor = cache.cursor(w)?;
        let page: SyncPage = json(
            session
                .get(http, &format!("/workspaces/{w}/sync"))
                .query(&[("since_seq", cursor), ("limit", 1000)]),
        )
        .await?;
        anyhow::ensure!(page.next_seq >= cursor, "server cursor moved backwards");
        let events: Vec<_> = page.events.into_iter().map(|e| e.event).collect();
        cache.apply_page(w, &events, page.next_seq)?;
        if !page.has_more {
            break;
        }
        anyhow::ensure!(page.next_seq > cursor, "sync made no progress");
    }
    Ok(())
}
pub fn normalize_server(value: &str) -> Result<String> {
    let url = url::Url::parse(value.trim())?;
    anyhow::ensure!(
        matches!(url.scheme(), "http" | "https")
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "Use an http:// or https:// server URL without credentials or query"
    );
    Ok(url.to_string().trim_end_matches('/').to_owned())
}
