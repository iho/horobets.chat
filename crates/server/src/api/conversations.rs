use super::*;
pub fn channel(r: &rusqlite::Row<'_>) -> rusqlite::Result<Channel> {
    Ok(Channel {
        id: r.get(0)?,
        name: r.get(1)?,
        topic: r.get(2)?,
        is_private: r.get(3)?,
        created_by: r.get(4)?,
        created_at: r.get(5)?,
        is_direct: r.get(6)?,
        is_saved: r.get(8)?,
        peer_user_id: r.get(7)?,
    })
}
#[derive(Deserialize)]
pub struct Target {
    user_id: String,
}
pub async fn direct(
    State(app): State<App>,
    Path(w): Path<String>,
    h: HeaderMap,
    Json(target): Json<Target>,
) -> Result<Json<Channel>> {
    let (u, db) = context(&app, &h, &w).await?;
    if target.user_id == u {
        return saved(db, u).await;
    }
    app.db.authorize(w, target.user_id.clone()).await?;
    let hub = db.events.clone();
    db.write(move|c|{let(a,b)=if u<target.user_id{(u.clone(),target.user_id.clone())}else{(target.user_id.clone(),u.clone())};let pair=format!("{a}:{b}");let tx=c.transaction()?;
 let existing:Option<String>=tx.query_row("SELECT channel_id FROM direct_channels WHERE pair_key=?",[&pair],|r|r.get(0)).optional()?;
 let id=if let Some(id)=existing{id}else{let id=crate::id();tx.execute("INSERT INTO channels VALUES(?,'Direct message',NULL,1,?,?)",params![id,u,crate::now()])?;tx.execute("INSERT INTO direct_channels VALUES(?,?,?,?)",params![id,pair,a,b])?;for member in [&u,&target.user_id]{tx.execute("INSERT INTO channel_members(channel_id,user_id) VALUES(?,?)",params![id,member])?;}id};
 let out=tx.query_row("SELECT c.id,c.name,c.topic,c.is_private,c.created_by,c.created_at,1,CASE WHEN d.user_a=?1 THEN d.user_b ELSE d.user_a END,0 FROM channels c JOIN direct_channels d ON d.channel_id=c.id WHERE c.id=?2",params![u,id],channel)?;tx.commit()?;let _=hub.send(shared_proto::ServerEvent::ChannelsChanged);Ok(Json(out))}).await
}
pub async fn members(
    State(app): State<App>,
    Path((w, ch)): Path<(String, String)>,
    h: HeaderMap,
) -> Result<Json<Vec<String>>> {
    let (u, db) = context(&app, &h, &w).await?;
    db.read(move |c| {
        q::access(c, &ch, &u)?;
        let mut s =
            c.prepare("SELECT user_id FROM channel_members WHERE channel_id=? ORDER BY user_id")?;
        Ok(Json(
            s.query_map([ch], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?,
        ))
    })
    .await
}
fn can_manage(c: &rusqlite::Connection, ch: &str, user: &str, role: &str) -> Result<()> {
    q::access(c, ch, user)?;
    let(owner,direct):(String,bool)=c.query_row("SELECT created_by,(EXISTS(SELECT 1 FROM direct_channels d WHERE d.channel_id=c.id) OR EXISTS(SELECT 1 FROM saved_channels s WHERE s.channel_id=c.id)) FROM channels c WHERE id=?",[ch],|r|Ok((r.get(0)?,r.get(1)?)))?;
    if direct {
        return Err(Error::Bad(
            "Direct-message participants cannot be changed".into(),
        ));
    }
    if owner != user && role != "owner" && role != "admin" && role != "moderator" {
        return Err(Error::Forbidden);
    }
    Ok(())
}
pub async fn add_member(
    State(app): State<App>,
    Path((w, ch)): Path<(String, String)>,
    h: HeaderMap,
    Json(target): Json<Target>,
) -> Result<Json<Value>> {
    let u = app.auth.user(&h)?;
    let role = app.db.authorize(w.clone(), u.clone()).await?;
    app.db.authorize(w.clone(), target.user_id.clone()).await?;
    let db = app.db.workspace(&w).await?;
    let hub = db.events.clone();
    db.write(move |c| {
        let tx = c.transaction()?;
        can_manage(&tx, &ch, &u, &role)?;
        tx.execute(
            "INSERT INTO channel_members(channel_id,user_id) VALUES(?,?) ON CONFLICT DO NOTHING",
            params![ch, target.user_id],
        )?;
        tx.commit()?;
        let _ = hub.send(shared_proto::ServerEvent::ChannelsChanged);
        Ok(Json(json!({"ok":true})))
    })
    .await
}
pub async fn remove_member(
    State(app): State<App>,
    Path((w, ch, target)): Path<(String, String, String)>,
    h: HeaderMap,
) -> Result<Json<Value>> {
    let u = app.auth.user(&h)?;
    let role = app.db.authorize(w.clone(), u.clone()).await?;
    let db = app.db.workspace(&w).await?;
    let hub = db.events.clone();
    db.write(move |c| {
        let tx = c.transaction()?;
        can_manage(&tx, &ch, &u, &role)?;
        let owner: String =
            tx.query_row("SELECT created_by FROM channels WHERE id=?", [&ch], |r| {
                r.get(0)
            })?;
        if target == owner {
            return Err(Error::Bad(
                "The channel creator must remain a member".into(),
            ));
        }
        tx.execute(
            "DELETE FROM channel_members WHERE channel_id=? AND user_id=?",
            params![ch, target],
        )?;
        tx.commit()?;
        let _ = hub.send(shared_proto::ServerEvent::ChannelsChanged);
        Ok(Json(json!({"ok":true})))
    })
    .await
}

async fn saved(db: Database, u: String) -> Result<Json<Channel>> {
    let hub = db.events.clone();
    db.write(move|c|{let tx=c.transaction()?;let existing:Option<String>=tx.query_row("SELECT channel_id FROM saved_channels WHERE user_id=?",[&u],|r|r.get(0)).optional()?;
 let id=if let Some(id)=existing{id}else{let id=crate::id();tx.execute("INSERT INTO channels VALUES(?,'Saved messages',NULL,1,?,?)",params![id,u,crate::now()])?;tx.execute("INSERT INTO saved_channels VALUES(?,?)",params![u,id])?;tx.execute("INSERT INTO channel_members(channel_id,user_id) VALUES(?,?)",params![id,u])?;id};
 let out=tx.query_row("SELECT id,name,topic,is_private,created_by,created_at,0,NULL,1 FROM channels WHERE id=?",[id],channel)?;tx.commit()?;let _=hub.send(shared_proto::ServerEvent::ChannelsChanged);Ok(Json(out))}).await
}
