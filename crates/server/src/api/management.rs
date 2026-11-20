use super::*;
use shared_proto::{MemberInfo, ModerationAction, PinnedMessage};
#[derive(Deserialize)]
pub struct WorkspaceEdit {
    name: String,
    slug: String,
}
pub async fn workspace(
    State(app): State<App>,
    Path(w): Path<String>,
    h: HeaderMap,
    Json(v): Json<WorkspaceEdit>,
) -> Result<Json<Workspace>> {
    let u = app.auth.user(&h)?;
    let role = app.db.authorize(w.clone(), u).await?;
    if role != "owner" && role != "admin" {
        return Err(Error::Forbidden);
    }
    let name = v.name.trim().to_owned();
    let slug = v.slug.trim().to_owned();
    if name.is_empty()
        || name.len() > 200
        || slug.is_empty()
        || slug.len() > 64
        || !slug
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
    {
        return Err(Error::Bad(
            "Enter a workspace name and a lowercase slug".into(),
        ));
    }
    let id = w.clone();
    let out = app
        .db
        .global
        .write(move |c| {
            c.execute(
                "UPDATE workspaces SET name=?,slug=? WHERE id=?",
                params![name, slug, id],
            )?;
            Ok(Workspace {
                id: id.clone(),
                name,
                slug,
                created_at: c.query_row(
                    "SELECT created_at FROM workspaces WHERE id=?",
                    [id],
                    |r| r.get(0),
                )?,
            })
        })
        .await?;
    let _ = app
        .db
        .workspace(&w)
        .await?
        .events
        .send(shared_proto::ServerEvent::ChannelsChanged);
    Ok(Json(out))
}
#[derive(Deserialize)]
pub struct ChannelEdit {
    name: String,
    topic: Option<String>,
}
pub async fn channel(
    State(app): State<App>,
    Path((w, ch)): Path<(String, String)>,
    h: HeaderMap,
    Json(v): Json<ChannelEdit>,
) -> Result<Json<Value>> {
    let u = app.auth.user(&h)?;
    let role = app.db.authorize(w.clone(), u.clone()).await?;
    let db = app.db.workspace(&w).await?;
    let hub = db.events.clone();
    let name = v.name.trim().to_owned();
    if name.is_empty() || name.len() > 100 || v.topic.as_ref().is_some_and(|s| s.len() > 1000) {
        return Err(Error::Bad(
            "Enter a channel name up to 100 characters and a topic up to 1000 characters".into(),
        ));
    }
    db.write(move |c| {
        let tx = c.transaction()?;
        q::access(&tx, &ch, &u)?;
        if q::personal(&tx, &ch)? {
            return Err(Error::Bad(
                "Personal conversations cannot be renamed".into(),
            ));
        }
        let owner: String =
            tx.query_row("SELECT created_by FROM channels WHERE id=?", [&ch], |r| {
                r.get(0)
            })?;
        if owner != u && !is_moderator(&role) {
            return Err(Error::Forbidden);
        }
        tx.execute(
            "UPDATE channels SET name=?,topic=? WHERE id=?",
            params![name, v.topic, ch],
        )?;
        tx.commit()?;
        let _ = hub.send(shared_proto::ServerEvent::ChannelsChanged);
        Ok(Json(json!({"ok":true})))
    })
    .await
}
pub fn is_moderator(role: &str) -> bool {
    matches!(role, "owner" | "admin" | "moderator")
}
async fn directory(app: &App, w: String) -> Result<Vec<MemberInfo>> {
    app.db.global.read(move|c|{let mut s=c.prepare("SELECT u.id,u.email,u.display_name,u.avatar_url,u.created_at,h.handle,COALESCE(d.bio,''),CASE WHEN m.role IN ('owner','admin') THEN m.role WHEN EXISTS(SELECT 1 FROM workspace_moderators x WHERE x.workspace_id=m.workspace_id AND x.user_id=m.user_id) THEN 'moderator' ELSE m.role END,m.joined_at FROM users u JOIN workspace_members m ON m.user_id=u.id LEFT JOIN user_handles h ON h.user_id=u.id LEFT JOIN user_details d ON d.user_id=u.id WHERE m.workspace_id=? ORDER BY u.display_name")?;
 Ok(s.query_map([w],|r|Ok(MemberInfo{user:shared_proto::User{id:r.get(0)?,email:r.get(1)?,display_name:r.get(2)?,avatar_url:r.get(3)?,created_at:r.get(4)?,handle:r.get(5)?,bio:r.get(6)?},role:r.get(7)?,joined_at:r.get(8)?}))?.collect::<rusqlite::Result<_>>()?)}).await
}
pub async fn people(
    State(app): State<App>,
    Path(w): Path<String>,
    h: HeaderMap,
) -> Result<Json<Vec<MemberInfo>>> {
    let u = app.auth.user(&h)?;
    app.db.authorize(w.clone(), u).await?;
    Ok(Json(directory(&app, w).await?))
}
pub async fn profile(
    State(app): State<App>,
    Path((w, user)): Path<(String, String)>,
    h: HeaderMap,
) -> Result<Json<MemberInfo>> {
    let u = app.auth.user(&h)?;
    app.db.authorize(w.clone(), u).await?;
    Ok(Json(
        directory(&app, w)
            .await?
            .into_iter()
            .find(|m| m.user.id == user)
            .ok_or(Error::NotFound)?,
    ))
}
#[derive(Deserialize)]
pub struct RoleEdit {
    role: String,
}
pub async fn role(
    State(app): State<App>,
    Path((w, target)): Path<(String, String)>,
    h: HeaderMap,
    Json(v): Json<RoleEdit>,
) -> Result<Json<Value>> {
    let u = app.auth.user(&h)?;
    let caller = app.db.authorize(w.clone(), u.clone()).await?;
    if caller != "owner" && caller != "admin" {
        return Err(Error::Forbidden);
    }
    if !matches!(v.role.as_str(), "member" | "moderator") {
        return Err(Error::Bad("Choose member or moderator".into()));
    }
    let workspace = w.clone();
    app.db
        .global
        .write(move |c| {
            let tx = c.transaction()?;
            let base: String = tx
                .query_row(
                    "SELECT role FROM workspace_members WHERE workspace_id=? AND user_id=?",
                    params![workspace, target],
                    |r| r.get(0),
                )
                .optional()?
                .ok_or(Error::NotFound)?;
            if base != "member" {
                return Err(Error::Forbidden);
            }
            if v.role == "moderator" {
                tx.execute(
                    "INSERT INTO workspace_moderators VALUES(?,?,?,?) ON CONFLICT DO NOTHING",
                    params![workspace, target, u, crate::now()],
                )?;
            } else {
                tx.execute(
                    "DELETE FROM workspace_moderators WHERE workspace_id=? AND user_id=?",
                    params![workspace, target],
                )?;
            }
            tx.commit()?;
            Ok(())
        })
        .await?;
    let _ = app
        .db
        .workspace(&w)
        .await?
        .events
        .send(shared_proto::ServerEvent::ChannelsChanged);
    Ok(Json(json!({"ok":true})))
}
pub async fn pins(
    State(app): State<App>,
    Path(w): Path<String>,
    h: HeaderMap,
) -> Result<Json<Vec<PinnedMessage>>> {
    let (u, db) = context(&app, &h, &w).await?;
    db.read(move|c|{let mut s=c.prepare("SELECT m.id,p.pinned_by,p.created_at FROM message_pins p JOIN messages m ON m.id=p.message_id WHERE m.deleted_at IS NULL ORDER BY p.created_at DESC")?;let rows=s.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,i64>(2)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;let mut out=vec![];for(id,user,at)in rows{let message=q::get(c,&id)?;if q::access(c,&message.channel_id,&u).is_ok(){out.push(PinnedMessage{message,pinned_by:user,created_at:at});}}Ok(Json(out))}).await
}
pub async fn activity(
    State(app): State<App>,
    Path(w): Path<String>,
    h: HeaderMap,
) -> Result<Json<Vec<ModerationAction>>> {
    let u = app.auth.user(&h)?;
    let role = app.db.authorize(w.clone(), u.clone()).await?;
    if !is_moderator(&role) {
        return Err(Error::Forbidden);
    }
    let db = app.db.workspace(&w).await?;
    db.read(move|c|{let mut s=c.prepare("SELECT seq,channel_id,message_id,actor_id,action,created_at FROM moderation_actions ORDER BY seq DESC LIMIT 200")?;let rows=s.query_map([],|r|Ok(ModerationAction{seq:r.get(0)?,channel_id:r.get(1)?,message_id:r.get(2)?,actor_id:r.get(3)?,action:r.get(4)?,created_at:r.get(5)?}))?.collect::<rusqlite::Result<Vec<_>>>()?;Ok(Json(rows.into_iter().filter(|a|q::access(c,&a.channel_id,&u).is_ok()).collect()))}).await
}
