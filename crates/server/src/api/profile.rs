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
                "SELECT id,email,display_name,avatar_url,created_at,(SELECT handle FROM user_handles WHERE user_id=users.id),COALESCE((SELECT bio FROM user_details WHERE user_id=users.id),'') FROM users WHERE id=?",
                [u],
                |r| {
                    Ok(shared_proto::User {
                        id: r.get(0)?,
                        email: r.get(1)?,
                        display_name: r.get(2)?,
                        avatar_url: r.get(3)?,
                        created_at: r.get(4)?,handle:r.get(5)?,bio:r.get(6)?,
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

#[derive(Deserialize)]
pub struct ProfileUpdate {
    display_name: String,
    handle: Option<String>,
    bio: Option<String>,
}
pub async fn update(
    State(app): State<App>,
    h: HeaderMap,
    Json(v): Json<ProfileUpdate>,
) -> Result<Json<shared_proto::User>> {
    let u = app.auth.user(&h)?;
    let name = v.display_name.trim().to_owned();
    if name.is_empty() || name.len() > 200 {
        return Err(Error::Bad(
            "Display name must contain 1–200 characters".into(),
        ));
    }
    let handle = v
        .handle
        .unwrap_or_default()
        .trim()
        .trim_start_matches('@')
        .to_ascii_lowercase();
    if !handle.is_empty()
        && (!(3..=32).contains(&handle.len())
            || !handle.as_bytes()[0].is_ascii_lowercase()
            || !handle
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_'))
    {
        return Err(Error::Bad(
            "Use 3–32 letters, numbers, or underscores, starting with a letter".into(),
        ));
    }
    if v.bio.as_ref().is_some_and(|bio| bio.len() > 2000) {
        return Err(Error::Bad("Bio must be at most 2000 bytes".into()));
    }
    let uid = u.clone();
    app.db.global.write(move|c|{let tx=c.transaction()?;
 if let Some(bio)=v.bio {tx.execute("INSERT INTO user_details VALUES(?,?) ON CONFLICT(user_id) DO UPDATE SET bio=excluded.bio",params![uid,bio.trim()])?;}
 if handle.is_empty(){tx.execute("DELETE FROM user_handles WHERE user_id=?",[&uid])?;}else{
 tx.execute("INSERT INTO user_handles VALUES(?,?) ON CONFLICT(user_id) DO UPDATE SET handle=excluded.handle",params![uid,handle]).map_err(|e|match e {rusqlite::Error::SqliteFailure(ref code,_) if code.code==rusqlite::ErrorCode::ConstraintViolation=>Error::Bad("That handle is already taken".into()),_=>e.into()})?;
 }
 tx.execute("UPDATE users SET display_name=? WHERE id=?",params![name,uid])?;tx.commit()?;Ok(())}).await?;
    Ok(Json(user(&app, u).await?))
}
