//! Durable per-device delivery. Credentials stay on the server, never in either app.
use crate::{App, Error, Result, db::queries};
use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

#[derive(Deserialize)]
pub struct Device {
    platform: String,
    token: String,
    #[serde(default = "production")]
    environment: String,
}
fn production() -> String {
    "production".into()
}
pub async fn register(
    State(app): State<App>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(v): Json<Device>,
) -> Result<Json<Value>> {
    let user = app.auth.user(&headers)?;
    uuid::Uuid::parse_str(&id).map_err(|_| Error::Bad("Invalid installation ID".into()))?;
    if !matches!(v.platform.as_str(), "ios" | "android")
        || !matches!(v.environment.as_str(), "development" | "production")
        || !(20..=4096).contains(&v.token.len())
        || !v.token.bytes().all(|c| c.is_ascii_graphic())
    {
        return Err(Error::Bad("Invalid push registration".into()));
    }
    if v.platform == "ios"
        && (!v.token.len().is_multiple_of(2) || !v.token.bytes().all(|c| c.is_ascii_hexdigit()))
    {
        return Err(Error::Bad("Invalid APNs device token".into()));
    }
    let configured = app.push.configured(&v.platform);
    let platform = if v.platform == "ios" { "apns" } else { "fcm" };
    app.db.global.write(move|c|{
        let tx=c.transaction()?;
        // A reinstall/account switch replaces ownership and discards old delivery jobs.
        tx.execute("DELETE FROM notification_jobs WHERE device_id=?",[&id])?;
        tx.execute("DELETE FROM push_tokens WHERE (platform=? AND token=?) OR id=?",params![platform,v.token,id])?;
        tx.execute("INSERT INTO push_tokens(id,user_id,platform,token,updated_at) VALUES(?,?,?,?,?)",params![id,user,platform,v.token,crate::now()])?;
        tx.execute("INSERT INTO push_environments VALUES(?,?) ON CONFLICT(device_id) DO UPDATE SET environment=excluded.environment",params![id,v.environment])?;
        tx.commit()?;Ok(())
    }).await?;
    Ok(Json(
        json!({"registered":true,"delivery_configured":configured}),
    ))
}
pub async fn unregister(
    State(app): State<App>,
    Path(id): Path<String>,
    h: HeaderMap,
) -> Result<Json<Value>> {
    let user = app.auth.user(&h)?;
    app.db
        .global
        .write(move |c| {
            c.execute(
                "DELETE FROM push_tokens WHERE id=? AND user_id=?",
                params![id, user],
            )?;
            Ok(())
        })
        .await?;
    Ok(Json(json!({"registered":false})))
}
#[derive(Deserialize)]
struct GoogleAccount {
    project_id: String,
    client_email: String,
    private_key: String,
}
struct Apns {
    key: jsonwebtoken::EncodingKey,
    key_id: String,
    team: String,
    topic: String,
}
impl Apns {
    fn from_env(prefix: &str) -> anyhow::Result<Option<Self>> {
        let Some(path) = std::env::var_os(format!("{prefix}_KEY_FILE")) else {
            return Ok(None);
        };
        let setting = |name: &str| {
            std::env::var(format!("{prefix}_{name}"))
                .or_else(|_| std::env::var(format!("MESSENGER_APNS_{name}")))
        };
        Ok(Some(Self {
            key: jsonwebtoken::EncodingKey::from_ec_pem(&std::fs::read(path)?)?,
            key_id: setting("KEY_ID")?,
            team: setting("TEAM_ID")?,
            topic: setting("TOPIC")?,
        }))
    }
}
pub struct Push {
    http: reqwest::Client,
    apns: Option<Apns>,
    development_apns: Option<Apns>,
    apns_tokens: tokio::sync::Mutex<std::collections::HashMap<String, (String, i64)>>,
    google: Option<GoogleAccount>,
    oauth: tokio::sync::Mutex<Option<(String, i64)>>,
    started: AtomicBool,
}
impl Push {
    pub fn from_env() -> anyhow::Result<Self> {
        let apns = Apns::from_env("MESSENGER_APNS")?;
        let development_apns = Apns::from_env("MESSENGER_APNS_DEVELOPMENT")?;
        let google = std::env::var_os("MESSENGER_FCM_SERVICE_ACCOUNT")
            .map(|p| -> anyhow::Result<GoogleAccount> {
                Ok(serde_json::from_slice(&std::fs::read(p)?)?)
            })
            .transpose()?;
        Ok(Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()?,
            apns,
            development_apns,
            apns_tokens: tokio::sync::Mutex::new(Default::default()),
            google,
            oauth: tokio::sync::Mutex::new(None),
            started: AtomicBool::new(false),
        })
    }
    pub fn configured(&self, platform: &str) -> bool {
        match platform {
            "ios" | "apns" => self.apns.is_some() || self.development_apns.is_some(),
            "android" | "fcm" => self.google.is_some(),
            _ => false,
        }
    }
    async fn google_token(&self, g: &GoogleAccount) -> anyhow::Result<String> {
        let mut cached = self.oauth.lock().await;
        let now = crate::now() / 1000;
        if let Some((token, expiry)) = &*cached
            && *expiry > now + 60
        {
            return Ok(token.clone());
        }
        #[derive(Serialize)]
        struct Claims<'a> {
            iss: &'a str,
            scope: &'a str,
            aud: &'a str,
            iat: i64,
            exp: i64,
        }
        let assertion = jsonwebtoken::encode(
            &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256),
            &Claims {
                iss: &g.client_email,
                scope: "https://www.googleapis.com/auth/firebase.messaging",
                aud: "https://oauth2.googleapis.com/token",
                iat: now,
                exp: now + 3600,
            },
            &jsonwebtoken::EncodingKey::from_rsa_pem(g.private_key.as_bytes())?,
        )?;
        let response = self
            .http
            .post("https://oauth2.googleapis.com/token")
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                ("assertion", &assertion),
            ])
            .send()
            .await?;
        anyhow::ensure!(
            response.status().is_success(),
            "FCM authentication rejected"
        );
        let v: Value = response.json().await?;
        let token = v["access_token"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("FCM token missing"))?
            .to_owned();
        *cached = Some((
            token.clone(),
            now + v["expires_in"].as_i64().unwrap_or(3600),
        ));
        Ok(token)
    }
    async fn apns_token(&self, a: &Apns) -> anyhow::Result<String> {
        let now = crate::now() / 1000;
        let mut tokens = self.apns_tokens.lock().await;
        if let Some((token, expiry)) = tokens.get(&a.key_id)
            && *expiry > now + 60
        {
            return Ok(token.clone());
        }
        #[derive(Serialize)]
        struct Claims<'a> {
            iss: &'a str,
            iat: i64,
        }
        let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::ES256);
        header.kid = Some(a.key_id.clone());
        let token = jsonwebtoken::encode(
            &header,
            &Claims {
                iss: &a.team,
                iat: now,
            },
            &a.key,
        )?;
        // APNs requires reuse for >=20 minutes and renewal within one hour.
        tokens.insert(a.key_id.clone(), (token.clone(), now + 3000));
        Ok(token)
    }
    async fn send(
        &self,
        platform: &str,
        token: &str,
        environment: &str,
        payload: &Payload,
    ) -> anyhow::Result<bool> {
        let response = if matches!(platform, "ios" | "apns") {
            let a = if environment == "development" {
                self.development_apns.as_ref().or(self.apns.as_ref())
            } else {
                self.apns.as_ref()
            }
            .ok_or_else(|| anyhow::anyhow!("APNs is not configured for this environment"))?;
            let jwt = self.apns_token(a).await?;
            let host = if environment == "development" {
                "api.sandbox.push.apple.com"
            } else {
                "api.push.apple.com"
            };
            self.http
                .post(format!("https://{host}/3/device/{token}"))
                .version(reqwest::Version::HTTP_2)
                .bearer_auth(jwt)
                .header("apns-topic", &a.topic)
                .header("apns-push-type", "alert")
                .header("apns-collapse-id", &payload.message)
                .header("apns-priority", "10")
                .json(&payload.apns())
                .send()
                .await?
        } else {
            let g = self
                .google
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("FCM is not configured"))?;
            self.http
                .post(format!(
                    "https://fcm.googleapis.com/v1/projects/{}/messages:send",
                    g.project_id
                ))
                .bearer_auth(self.google_token(g).await?)
                .json(&payload.fcm(token))
                .send()
                .await?
        };
        let status = response.status();
        // APNs Unregistered / FCM UNREGISTERED. Do not delete tokens for auth/configuration errors.
        if status == reqwest::StatusCode::GONE {
            return Ok(false);
        }
        if matches!(platform, "android" | "fcm") && status == reqwest::StatusCode::NOT_FOUND {
            let body: Value = response.json().await.unwrap_or_default();
            if body["error"]["details"]
                .as_array()
                .is_some_and(|items| items.iter().any(|v| v["errorCode"] == "UNREGISTERED"))
            {
                return Ok(false);
            }
            anyhow::bail!("FCM delivery rejected")
        }
        anyhow::ensure!(
            status.is_success(),
            "Push provider returned status {}",
            status.as_u16()
        );
        Ok(true)
    }
}
#[derive(Clone, Debug)]
pub struct Payload {
    pub workspace: String,
    pub channel: String,
    pub message: String,
    pub recipient: String,
    pub title: String,
    pub body: String,
}
impl Payload {
    fn data(&self) -> Value {
        json!({"workspace_id":self.workspace,"channel_id":self.channel,"message_id":self.message,"user_id":self.recipient})
    }
    pub fn apns(&self) -> Value {
        let mut v = self.data();
        v["aps"] = json!({"alert":{"title":self.title,"body":self.body},"sound":"default","thread-id":format!("{}:{}",self.workspace,self.channel)});
        v
    }
    pub fn fcm(&self, token: &str) -> Value {
        let mut data = self.data();
        data["title"] = json!(self.title);
        data["body"] = json!(self.body);
        // Data-only delivery lets the native service reject alerts after sign-out
        // before anything reaches Android's notification tray.
        json!({"message":{"token":token,"data":data,"android":{"priority":"high"}}})
    }
}
// Called after a committed message, before its live broadcast. Duplicate sends
// use the same message/device key and cannot enqueue a second delivery.
pub async fn enqueue(app: &App, workspace: &str, message: &shared_proto::MessageDto) -> Result<()> {
    let w = workspace.to_owned();
    let sender = message.user_id.clone();
    let ch = message.channel_id.clone();
    let devices:Vec<(String,String,String)>=app.db.global.read(move|c|{
        let mut q=c.prepare("SELECT p.id,p.user_id,p.platform FROM push_tokens p JOIN workspace_members m ON m.user_id=p.user_id WHERE m.workspace_id=? AND p.user_id!=?")?;
        Ok(q.query_map(params![w,sender],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?.collect::<rusqlite::Result<_>>()?)
    }).await?;
    let db = app.db.workspace(workspace).await?;
    for (device, user, platform) in devices {
        if !app.push.configured(&platform) {
            continue;
        }
        let uid = user.clone();
        let channel = ch.clone();
        if !db
            .read(move |c| queries::access(c, &channel, &uid))
            .await
            .is_ok()
        {
            continue;
        }
        let w = workspace.to_owned();
        let message = message.id.clone();
        let channel = ch.clone();
        app.db.global.write(move|c|{c.execute("INSERT OR IGNORE INTO notification_jobs(workspace_id,channel_id,message_id,user_id,device_id,created_at) VALUES(?,?,?,?,?,?)",params![w,channel,message,user,device,crate::now()])?;Ok(())}).await?;
    }
    Ok(())
}
pub fn start(app: App) {
    if app.push.started.swap(true, Ordering::SeqCst) {
        return;
    }
    tokio::spawn(async move {
        loop {
            if let Err(e) = deliver_batch(&app).await {
                tracing::warn!(error=%e,"push delivery scan failed");
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });
}
struct DeliveryJob {
    id: i64,
    workspace: String,
    message: String,
    user: String,
    device: String,
    platform: String,
    token: String,
    environment: String,
    attempts: i64,
}
async fn deliver_batch(app: &App) -> Result<()> {
    let jobs:Vec<DeliveryJob>=app.db.global.read(|c|{
        let mut q=c.prepare("SELECT j.rowid,j.workspace_id,j.message_id,j.user_id,p.id,p.platform,p.token,COALESCE(e.environment,'production'),j.attempts FROM notification_jobs j JOIN push_tokens p ON p.id=j.device_id AND p.user_id=j.user_id LEFT JOIN push_environments e ON e.device_id=p.id WHERE j.not_before<=? ORDER BY j.rowid LIMIT 16")?;
        Ok(q.query_map([crate::now()],|r|Ok(DeliveryJob{id:r.get(0)?,workspace:r.get(1)?,message:r.get(2)?,user:r.get(3)?,device:r.get(4)?,platform:r.get(5)?,token:r.get(6)?,environment:r.get(7)?,attempts:r.get(8)?}))?.collect::<rusqlite::Result<_>>()?)
    }).await?;
    for DeliveryJob {
        id,
        workspace: w,
        message,
        user,
        device,
        platform,
        token,
        environment,
        attempts,
    } in jobs
    {
        let result=async {
            match app.db.authorize(w.clone(),user.clone()).await {Ok(_)=>{},Err(Error::Forbidden|Error::NotFound)=>return Ok(true),Err(e)=>return Err(e.into())}
            let db=app.db.workspace(&w).await?;let uid=user.clone();
            let m=db.read(move|c|{
                let m=queries::get(c,&message)?;
                queries::access(c,&m.channel_id,&uid)?;
                if m.deleted_at.is_some(){return Err(Error::NotFound)}
                let read:i64=c.query_row("SELECT last_read_seq FROM channel_members WHERE channel_id=? AND user_id=?",params![m.channel_id,uid],|r|r.get(0)).optional()?.unwrap_or(0);
                if read>=m.seq{return Err(Error::NotFound)}Ok(m)
            }).await;
            let m=match m {Ok(m)=>m,Err(Error::Forbidden|Error::NotFound)=>return Ok(true),Err(e)=>return Err(e.into())};
            let sender=m.user_id.clone();
            let name:String=app.db.global.read(move|c|Ok(c.query_row("SELECT display_name FROM users WHERE id=?",[sender],|r|r.get(0))?)).await?;
            let body=if m.body_md.trim().is_empty(){"Sent an attachment".into()}else{m.body_md.chars().take(180).collect()};
            let payload=Payload{workspace:w,channel:m.channel_id,message:m.id,recipient:user,title:name,body};
            if !app.push.send(&platform,&token,&environment,&payload).await? {
                app.db.global.write(move|c|{c.execute("DELETE FROM push_tokens WHERE id=?",[device])?;Ok(())}).await?;
            }
            Ok::<bool,anyhow::Error>(true)
        }.await;
        let delete = result.is_ok() || attempts >= 5;
        if result.is_err() {
            tracing::warn!(
                platform,
                attempt = attempts + 1,
                retrying = !delete,
                "push delivery failed"
            );
        }
        app.db.global.write(move|c|{
            if delete {c.execute("DELETE FROM notification_jobs WHERE rowid=?",[id])?;}else{c.execute("UPDATE notification_jobs SET attempts=attempts+1,not_before=? WHERE rowid=?",params![crate::now()+(1_i64<<attempts.min(5))*5000,id])?;}
            Ok(())
        }).await?;
    }
    Ok(())
}
