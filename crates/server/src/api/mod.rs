mod conversations;
mod management;
mod profile;
use crate::{
    App, Error, Result,
    db::{manager::Database, queries as q},
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Multipart, Path, Query, State},
    http::{HeaderMap, header},
    response::IntoResponse,
    routing::{get, post},
};
use rusqlite::{OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};
use shared_proto::{Channel, Workspace};
pub fn router(app: App) -> Router {
    Router::new()
        .route("/health", get(|| async { Json(json!({"status":"ok"})) }))
        .route("/api/v1/auth/register", post(register))
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/users/me", get(profile::me).patch(profile::update))
        .route("/api/v1/users/me/avatar", post(profile::upload_avatar))
        .route("/api/v1/users/{u}/avatar/{key}", get(profile::avatar))
        .route(
            "/api/v1/workspaces/{w}/attachments/{a}/preview",
            get(profile::preview),
        )
        .route("/api/v1/workspaces", post(create_workspace).get(workspaces))
        .route(
            "/api/v1/workspaces/{w}/members",
            post(add_member).get(members),
        )
        .route(
            "/api/v1/workspaces/{w}/channels",
            post(create_channel).get(channels),
        )
        .route("/api/v1/workspaces/{w}/channels/{c}/messages", get(history))
        .route("/api/v1/workspaces/{w}/search", get(search))
        .route("/api/v1/workspaces/{w}/sync", get(sync))
        .route(
            "/api/v1/workspaces/{w}/attachments",
            post(upload).get(attachments),
        )
        .route("/api/v1/workspaces/{w}/attachments/{a}", get(download))
        .route(
            "/api/v1/workspaces/{w}/directs",
            post(conversations::direct),
        )
        .route(
            "/api/v1/workspaces/{w}/channels/{c}/members",
            get(conversations::members).post(conversations::add_member),
        )
        .route(
            "/api/v1/workspaces/{w}/channels/{c}/members/{u}",
            axum::routing::delete(conversations::remove_member),
        )
        .route(
            "/api/v1/workspaces/{w}",
            axum::routing::patch(management::workspace),
        )
        .route(
            "/api/v1/workspaces/{w}/channels/{c}",
            axum::routing::patch(management::channel),
        )
        .route("/api/v1/workspaces/{w}/people", get(management::people))
        .route(
            "/api/v1/workspaces/{w}/people/{u}",
            get(management::profile),
        )
        .route(
            "/api/v1/workspaces/{w}/people/{u}/role",
            axum::routing::patch(management::role),
        )
        .route("/api/v1/workspaces/{w}/pins", get(management::pins))
        .route(
            "/api/v1/workspaces/{w}/moderation",
            get(management::activity),
        )
        .route("/api/v1/workspaces/{w}/ws", get(crate::ws::upgrade))
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024))
        .with_state(app)
}
pub async fn context(app: &App, h: &HeaderMap, w: &str) -> Result<(String, Database)> {
    let u = app.auth.user(h)?;
    app.db.authorize(w.into(), u.clone()).await?;
    Ok((u, app.db.workspace(w).await?))
}
#[derive(Deserialize)]
struct Credentials {
    email: String,
    password: String,
    display_name: Option<String>,
}
async fn register(State(app): State<App>, Json(v): Json<Credentials>) -> Result<Json<Value>> {
    let email = v.email.trim().to_lowercase();
    if !email.contains('@') || email.len() > 254 || v.password.len() < 12 || v.password.len() > 1024
    {
        return Err(Error::Bad(
            "valid email and password of 12–1024 bytes required".into(),
        ));
    }
    let name = v.display_name.unwrap_or_else(|| email.clone());
    if name.trim().is_empty() || name.len() > 200 {
        return Err(Error::Bad("invalid display name".into()));
    }
    let hash = crate::auth::hash(v.password).await?;
    let id = crate::id();
    let uid = id.clone();
    app.db.global.write(move|c|{c.execute("INSERT INTO users(id,email,password_hash,display_name,created_at) VALUES(?,?,?,?,?)",params![uid,email,hash,name,crate::now()])?;Ok(())}).await?;
    Ok(Json(
        json!({"user_id":id,"token":app.auth.token(id.clone())?}),
    ))
}
async fn login(State(app): State<App>, Json(v): Json<Credentials>) -> Result<Json<Value>> {
    if v.password.len() > 1024 {
        return Err(Error::Unauthorized);
    }
    let email = v.email.trim().to_lowercase();
    let user: Option<(String, String)> = app
        .db
        .global
        .read(move |c| {
            Ok(c.query_row(
                "SELECT id,password_hash FROM users WHERE email=?",
                [email],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
        })
        .await?;
    let (id, hash) = user.ok_or(Error::Unauthorized)?;
    crate::auth::verify(v.password, hash).await?;
    Ok(Json(
        json!({"user_id":id,"token":app.auth.token(id.clone())?}),
    ))
}
#[derive(Deserialize)]
struct NewWorkspace {
    slug: String,
    name: String,
}
async fn create_workspace(
    State(app): State<App>,
    h: HeaderMap,
    Json(v): Json<NewWorkspace>,
) -> Result<Json<Workspace>> {
    let u = app.auth.user(&h)?;
    if v.slug.is_empty()
        || v.slug.len() > 64
        || !v
            .slug
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        || v.name.trim().is_empty()
        || v.name.len() > 200
    {
        return Err(Error::Bad("invalid workspace name or slug".into()));
    }
    let w = Workspace {
        id: crate::id(),
        slug: v.slug,
        name: v.name,
        created_at: crate::now(),
    };
    app.db.workspace(&w.id).await?;
    let out = w.clone();
    app.db
        .global
        .write(move |c| {
            let tx = c.transaction()?;
            tx.execute(
                "INSERT INTO workspaces VALUES(?,?,?,?,?)",
                params![
                    w.id,
                    w.slug,
                    w.name,
                    format!("workspaces/{}.db", w.id),
                    w.created_at
                ],
            )?;
            tx.execute(
                "INSERT INTO workspace_members VALUES(?,?,'owner',?)",
                params![w.id, u, crate::now()],
            )?;
            tx.commit()?;
            Ok(())
        })
        .await?;
    Ok(Json(out))
}
async fn workspaces(State(app): State<App>, h: HeaderMap) -> Result<Json<Vec<Workspace>>> {
    let u = app.auth.user(&h)?;
    app.db.global.read(move|c|{let mut s=c.prepare("SELECT w.id,w.slug,w.name,w.created_at FROM workspaces w JOIN workspace_members m ON m.workspace_id=w.id WHERE m.user_id=? ORDER BY w.created_at")?;Ok(Json(s.query_map([u],|r|Ok(Workspace{id:r.get(0)?,slug:r.get(1)?,name:r.get(2)?,created_at:r.get(3)?}))?.collect::<std::result::Result<_,_>>()?))}).await
}
#[derive(Deserialize)]
struct Member {
    email: String,
}
async fn add_member(
    State(app): State<App>,
    Path(w): Path<String>,
    h: HeaderMap,
    Json(v): Json<Member>,
) -> Result<Json<Value>> {
    let u = app.auth.user(&h)?;
    let role = app.db.authorize(w.clone(), u).await?;
    if role != "owner" && role != "admin" {
        return Err(Error::Forbidden);
    }
    let email = v.email.trim().to_lowercase();
    app.db
        .global
        .write(move |c| {
            let id: String = c
                .query_row("SELECT id FROM users WHERE email=?", [email], |r| r.get(0))
                .optional()?
                .ok_or(Error::NotFound)?;
            c.execute(
                "INSERT INTO workspace_members VALUES(?,?,'member',?) ON CONFLICT DO NOTHING",
                params![w, id, crate::now()],
            )?;
            Ok(Json(json!({"user_id":id})))
        })
        .await
}
#[derive(Deserialize)]
struct NewChannel {
    name: String,
    topic: Option<String>,
    #[serde(default)]
    is_private: bool,
}
async fn create_channel(
    State(app): State<App>,
    Path(w): Path<String>,
    h: HeaderMap,
    Json(v): Json<NewChannel>,
) -> Result<Json<Channel>> {
    let (u, db) = context(&app, &h, &w).await?;
    if v.name.trim().is_empty()
        || v.name.len() > 100
        || v.topic.as_ref().is_some_and(|s| s.len() > 1000)
    {
        return Err(Error::Bad("invalid channel".into()));
    }
    let ch = Channel {
        id: crate::id(),
        name: v.name,
        topic: v.topic,
        is_private: v.is_private,
        is_direct: false,
        is_saved: false,
        peer_user_id: None,
        created_by: u.clone(),
        created_at: crate::now(),
    };
    let out = ch.clone();
    let hub = db.events.clone();
    db.write(move |c| {
        let tx = c.transaction()?;
        tx.execute(
            "INSERT INTO channels VALUES(?,?,?,?,?,?)",
            params![
                ch.id,
                ch.name,
                ch.topic,
                ch.is_private,
                ch.created_by,
                ch.created_at
            ],
        )?;
        tx.execute(
            "INSERT INTO channel_members(channel_id,user_id) VALUES(?,?)",
            params![ch.id, u],
        )?;
        tx.commit()?;
        let _ = hub.send(shared_proto::ServerEvent::ChannelsChanged);
        Ok(())
    })
    .await?;
    Ok(Json(out))
}
async fn channels(
    State(app): State<App>,
    Path(w): Path<String>,
    h: HeaderMap,
) -> Result<Json<Vec<Channel>>> {
    let (u, db) = context(&app, &h, &w).await?;
    db.read(move|c|{let mut s=c.prepare("SELECT c.id,c.name,c.topic,c.is_private,c.created_by,c.created_at,d.channel_id IS NOT NULL,CASE WHEN d.user_a=?1 THEN d.user_b ELSE d.user_a END,EXISTS(SELECT 1 FROM saved_channels s WHERE s.channel_id=c.id) FROM channels c LEFT JOIN direct_channels d ON d.channel_id=c.id WHERE c.is_private=0 OR EXISTS(SELECT 1 FROM channel_members m WHERE m.channel_id=c.id AND m.user_id=?1) ORDER BY c.created_at")?;Ok(Json(s.query_map([u],conversations::channel)?.collect::<rusqlite::Result<_>>()?))}).await
}
#[derive(Deserialize, Default)]
struct Page {
    before_seq: Option<i64>,
    since_seq: Option<i64>,
    limit: Option<i64>,
    q: Option<String>,
}
async fn history(
    State(app): State<App>,
    Path((w, ch)): Path<(String, String)>,
    h: HeaderMap,
    Query(p): Query<Page>,
) -> Result<Json<Vec<shared_proto::MessageDto>>> {
    let (u, db) = context(&app, &h, &w).await?;
    db.read(move |c| {
        q::access(c, &ch, &u)?;
        let mut s = c.prepare(&format!(
            "SELECT {} FROM messages WHERE channel_id=? AND seq<? ORDER BY seq DESC LIMIT ?",
            q::COLS
        ))?;
        Ok(Json(
            s.query_map(
                params![
                    ch,
                    p.before_seq.unwrap_or(i64::MAX),
                    p.limit.unwrap_or(50).clamp(1, 200)
                ],
                q::message,
            )?
            .collect::<std::result::Result<_, _>>()?,
        ))
    })
    .await
}
async fn search(
    State(app): State<App>,
    Path(w): Path<String>,
    h: HeaderMap,
    Query(p): Query<Page>,
) -> Result<Json<Vec<shared_proto::MessageDto>>> {
    let (u, db) = context(&app, &h, &w).await?;
    let text = p.q.unwrap_or_default();
    if text.trim().is_empty() || text.len() > 500 {
        return Err(Error::Bad("search query required (max 500 bytes)".into()));
    }
    let query = format!("\"{}\"", text.replace('"', "\"\""));
    db.read(move|c|{let mut s=c.prepare(&format!("SELECT {} FROM messages WHERE id IN (SELECT message_id FROM messages_fts WHERE messages_fts MATCH ?1) AND channel_id IN (SELECT id FROM channels c WHERE is_private=0 OR EXISTS(SELECT 1 FROM channel_members m WHERE m.channel_id=c.id AND m.user_id=?2)) ORDER BY seq DESC LIMIT ?3",q::COLS))?;Ok(Json(s.query_map(params![query,u,p.limit.unwrap_or(50).clamp(1,200)],q::message)?.collect::<std::result::Result<_,_>>()?))}).await
}
async fn sync(
    State(app): State<App>,
    Path(w): Path<String>,
    h: HeaderMap,
    Query(p): Query<Page>,
) -> Result<Json<Value>> {
    let (u, db) = context(&app, &h, &w).await?;
    db.read(move |c| {
        let since = p.since_seq.unwrap_or(0).max(0);
        let mut s =
            c.prepare("SELECT seq,event_json FROM events WHERE seq>? ORDER BY seq LIMIT ?")?;
        let rows = s
            .query_map(params![since, p.limit.unwrap_or(200).clamp(1, 1000)], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut cursor = since;
        let mut events = vec![];
        for (seq, raw) in rows {
            cursor = seq;
            let e: shared_proto::ServerEvent =
                serde_json::from_str(&raw).map_err(|e| Error::Internal(e.into()))?;
            if q::event_channel(&e).is_none_or(|ch| q::access(c, ch, &u).is_ok()) {
                events.push(json!({"seq":seq,"event":e}));
            }
        }
        let latest: i64 =
            c.query_row("SELECT COALESCE(MAX(seq),0) FROM events", [], |r| r.get(0))?;
        Ok(Json(
            json!({"events":events,"next_seq":cursor,"has_more":cursor<latest}),
        ))
    })
    .await
}
async fn upload(
    State(app): State<App>,
    Path(w): Path<String>,
    h: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<shared_proto::Attachment>> {
    let (u, db) = context(&app, &h, &w).await?;
    let field = multipart
        .next_field()
        .await
        .map_err(|e| Error::Bad(e.to_string()))?
        .ok_or_else(|| Error::Bad("file required".into()))?;
    let filename = field.file_name().unwrap_or("upload").to_owned();
    if filename.len() > 255 {
        return Err(Error::Bad("filename too long".into()));
    }
    let claimed_mime = field
        .content_type()
        .unwrap_or("application/octet-stream")
        .to_owned();
    let data = field.bytes().await.map_err(|e| Error::Bad(e.to_string()))?;
    let mime = crate::media::image_mime(&data)
        .map(str::to_owned)
        .unwrap_or_else(|| {
            if claimed_mime.starts_with("image/") {
                "application/octet-stream".into()
            } else {
                claimed_mime
            }
        });
    let preview = if crate::media::image_mime(&data).is_some() {
        let bytes = data.clone();
        Some(
            tokio::task::spawn_blocking(move || crate::media::thumbnail(&bytes, false))
                .await
                .map_err(|e| Error::Internal(e.into()))??,
        )
    } else {
        None
    };
    let a = shared_proto::Attachment {
        id: crate::id(),
        message_id: None,
        uploader_id: u,
        filename,
        mime_type: mime,
        size_bytes: data.len() as i64,
        created_at: crate::now(),
    };
    app.blobs.put(&a.id, &data).await?;
    let preview_key = if let Some(png) = preview {
        let key = crate::id();
        app.blobs.put(&key, &png).await?;
        Some(key)
    } else {
        None
    };
    let out = a.clone();
    db.write(move |c| {
        let tx = c.transaction()?;
        tx.execute(
            "INSERT INTO attachments VALUES(?,NULL,?,?,?,?,?,?)",
            params![
                a.id,
                a.uploader_id,
                a.filename,
                a.mime_type,
                a.size_bytes,
                a.id,
                a.created_at
            ],
        )?;
        if let Some(key) = preview_key {
            tx.execute(
                "INSERT INTO attachment_previews VALUES(?,?)",
                params![a.id, key],
            )?;
        }
        tx.commit()?;
        Ok(())
    })
    .await?;
    Ok(Json(out))
}
async fn download(
    State(app): State<App>,
    Path((w, a)): Path<(String, String)>,
    h: HeaderMap,
) -> Result<impl IntoResponse> {
    let (u, db) = context(&app, &h, &w).await?;
    let key = db
        .read(move |c| {
            let row: Option<(String, Option<String>, String)> = c
                .query_row(
                    "SELECT storage_key,message_id,uploader_id FROM attachments WHERE id=?",
                    [a],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?;
            let (key, msg, owner) = row.ok_or(Error::NotFound)?;
            if let Some(msg) = msg {
                let m = q::get(c, &msg)?;
                q::access(c, &m.channel_id, &u)?;
            } else if owner != u {
                return Err(Error::Forbidden);
            }
            Ok(key)
        })
        .await?;
    let bytes = app.blobs.get(&key).await?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/octet-stream"),
            (header::CONTENT_DISPOSITION, "attachment"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        bytes,
    ))
}

/// Workspace-scoped directory; password hashes never leave the control database.
async fn members(
    State(app): State<App>,
    Path(w): Path<String>,
    h: HeaderMap,
) -> Result<Json<Vec<shared_proto::User>>> {
    let u = app.auth.user(&h)?;
    app.db.authorize(w.clone(), u).await?;
    app.db.global.read(move|c|{let mut s=c.prepare("SELECT u.id,u.email,u.display_name,u.avatar_url,u.created_at,(SELECT handle FROM user_handles WHERE user_id=u.id),COALESCE((SELECT bio FROM user_details WHERE user_id=u.id),'') FROM users u JOIN workspace_members m ON m.user_id=u.id WHERE m.workspace_id=? ORDER BY u.display_name")?;
 Ok(Json(s.query_map([w],|r|Ok(shared_proto::User{id:r.get(0)?,email:r.get(1)?,display_name:r.get(2)?,avatar_url:r.get(3)?,created_at:r.get(4)?,handle:r.get(5)?,bio:r.get(6)?}))?.collect::<rusqlite::Result<_>>()?))}).await
}

#[derive(Deserialize)]
struct AttachmentPage {
    channel_id: String,
    #[serde(default)]
    since_seq: i64,
}
async fn attachments(
    State(app): State<App>,
    Path(w): Path<String>,
    h: HeaderMap,
    Query(p): Query<AttachmentPage>,
) -> Result<Json<Vec<shared_proto::Attachment>>> {
    let (u, db) = context(&app, &h, &w).await?;
    db.read(move|c|{q::access(c,&p.channel_id,&u)?;let mut s=c.prepare("SELECT a.id,a.message_id,a.uploader_id,a.filename,a.mime_type,a.size_bytes,a.created_at FROM attachments a JOIN messages m ON m.id=a.message_id WHERE m.channel_id=? AND m.seq>=? AND m.deleted_at IS NULL ORDER BY m.seq,a.created_at")?;
 Ok(Json(s.query_map(params![p.channel_id,p.since_seq],|r|Ok(shared_proto::Attachment{id:r.get(0)?,message_id:r.get(1)?,uploader_id:r.get(2)?,filename:r.get(3)?,mime_type:r.get(4)?,size_bytes:r.get(5)?,created_at:r.get(6)?}))?.collect::<rusqlite::Result<_>>()?))}).await
}
