use super::*;
pub async fn me(State(app): State<App>, h: HeaderMap) -> Result<Json<shared_proto::User>> {
    let u = app.auth.user(&h)?;
    Ok(Json(user(&app, u).await?))
}
async fn user(app: &App, u: String) -> Result<shared_proto::User> {
    app.db
        .global
        .read(move |c| {
            c.query_row(
                "SELECT id,email,display_name,avatar_url,created_at FROM users WHERE id=?",
                [u],
                |r| {
                    Ok(shared_proto::User {
                        id: r.get(0)?,
                        email: r.get(1)?,
                        display_name: r.get(2)?,
                        avatar_url: r.get(3)?,
                        created_at: r.get(4)?,
                    })
                },
            )
            .optional()?
            .ok_or(Error::NotFound)
        })
        .await
}
pub async fn upload_avatar(
    State(app): State<App>,
    h: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<shared_proto::User>> {
    let u = app.auth.user(&h)?;
    let field = multipart
        .next_field()
        .await
        .map_err(|e| Error::Bad(e.to_string()))?
        .ok_or_else(|| Error::Bad("Image required".into()))?;
    let bytes = field.bytes().await.map_err(|e| Error::Bad(e.to_string()))?;
    if bytes.len() > 5 * 1024 * 1024 {
        return Err(Error::Bad("Profile picture must be at most 5 MiB".into()));
    }
    let png = tokio::task::spawn_blocking(move || crate::media::thumbnail(&bytes, true))
        .await
        .map_err(|e| Error::Internal(e.into()))??;
    let key = crate::id();
    app.blobs.put(&key, &png).await?;
    let uid = u.clone();
    let url = format!("/api/v1/users/{u}/avatar/{key}");
    app.db.global.write(move|c|{let tx=c.transaction()?;tx.execute("INSERT INTO user_avatars VALUES(?,?,?) ON CONFLICT(user_id) DO UPDATE SET storage_key=excluded.storage_key,updated_at=excluded.updated_at",params![uid,key,crate::now()])?;tx.execute("UPDATE users SET avatar_url=? WHERE id=?",params![url,uid])?;tx.commit()?;Ok(())}).await?;
    Ok(Json(user(&app, u).await?))
}
pub async fn avatar(
    State(app): State<App>,
    Path((owner, key)): Path<(String, String)>,
    h: HeaderMap,
) -> Result<impl IntoResponse> {
    let viewer = app.auth.user(&h)?;
    let lookup = key.clone();
    app.db.global.read(move|c|{
  let allowed:bool=c.query_row("SELECT ?1=?2 OR EXISTS(SELECT 1 FROM workspace_members a JOIN workspace_members b ON b.workspace_id=a.workspace_id WHERE a.user_id=?1 AND b.user_id=?2)",params![viewer,owner],|r|r.get(0))?;
  if !allowed{return Err(Error::Forbidden)}
  let exists:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM user_avatars WHERE user_id=? AND storage_key=?)",params![owner,lookup],|r|r.get(0))?;if !exists{return Err(Error::NotFound)}Ok(())
 }).await?;
    Ok((
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "private, max-age=3600"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        app.blobs.get(&key).await?,
    ))
}
pub async fn preview(
    State(app): State<App>,
    Path((w, a)): Path<(String, String)>,
    h: HeaderMap,
) -> Result<impl IntoResponse> {
    let (u, db) = context(&app, &h, &w).await?;
    let key=db.read(move|c|{
 let row:Option<(String,Option<String>,String)>=c.query_row("SELECT p.storage_key,a.message_id,a.uploader_id FROM attachment_previews p JOIN attachments a ON a.id=p.attachment_id WHERE a.id=?",[a],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
 let(key,message,owner)=row.ok_or(Error::NotFound)?;
 if let Some(message)=message{let m=q::get(c,&message)?;q::access(c,&m.channel_id,&u)?;if m.deleted_at.is_some(){return Err(Error::NotFound)}}else if owner!=u{return Err(Error::Forbidden)}Ok(key)
 }).await?;
    Ok((
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "private, max-age=3600"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        app.blobs.get(&key).await?,
    ))
}
