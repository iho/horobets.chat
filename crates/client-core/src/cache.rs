//! SQLite is owned by the engine thread. Every mutation and sync cursor commits atomically.
use crate::{CachedMessage, OutboxStatus, ReactionCount};
use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};
use shared_proto::{Channel, ClientEvent, MessageDto, ServerEvent, Workspace};
pub struct Cache(Connection);
impl Cache {
    pub fn open(path: &std::path::Path) -> Result<Self> {
        let c = Connection::open(path)?;
        c.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA busy_timeout=5000;
        CREATE TABLE IF NOT EXISTS metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS channels(workspace TEXT,id TEXT,json TEXT NOT NULL,PRIMARY KEY(workspace,id));
        CREATE TABLE IF NOT EXISTS messages(rowid INTEGER PRIMARY KEY,workspace TEXT NOT NULL,id TEXT NOT NULL,channel TEXT NOT NULL,parent TEXT,seq INTEGER NOT NULL,body TEXT NOT NULL,deleted INTEGER NOT NULL,json TEXT NOT NULL,UNIQUE(workspace,id));
        CREATE INDEX IF NOT EXISTS messages_channel ON messages(workspace,channel,seq);
        CREATE TABLE IF NOT EXISTS outbox(workspace TEXT,id TEXT,event TEXT NOT NULL,status TEXT NOT NULL DEFAULT 'pending',error TEXT,PRIMARY KEY(workspace,id));
        CREATE TABLE IF NOT EXISTS pins(workspace TEXT,message TEXT,channel TEXT,pinned_by TEXT,created_at INTEGER NOT NULL,PRIMARY KEY(workspace,message));
        CREATE TABLE IF NOT EXISTS reactions(workspace TEXT,message TEXT,user TEXT,emoji TEXT,PRIMARY KEY(workspace,message,user,emoji));
        CREATE TABLE IF NOT EXISTS reads(workspace TEXT,channel TEXT,seq INTEGER NOT NULL,PRIMARY KEY(workspace,channel));
        CREATE TABLE IF NOT EXISTS pending_reads(workspace TEXT,channel TEXT,seq INTEGER NOT NULL,PRIMARY KEY(workspace,channel));
        CREATE TABLE IF NOT EXISTS notification_seen(workspace TEXT,id TEXT,created_at INTEGER NOT NULL,PRIMARY KEY(workspace,id));
        CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(body,tokenize='porter unicode61');
        CREATE TRIGGER IF NOT EXISTS messages_ai AFTER INSERT ON messages WHEN new.deleted=0 BEGIN INSERT INTO messages_fts(rowid,body) VALUES(new.rowid,new.body); END;
        CREATE TRIGGER IF NOT EXISTS messages_au AFTER UPDATE ON messages BEGIN DELETE FROM messages_fts WHERE rowid=old.rowid; INSERT INTO messages_fts(rowid,body) SELECT new.rowid,new.body WHERE new.deleted=0; END;
        CREATE TRIGGER IF NOT EXISTS messages_ad AFTER DELETE ON messages BEGIN DELETE FROM messages_fts WHERE rowid=old.rowid; END;")?;
        Ok(Self(c))
    }
    pub fn metadata<T: serde::de::DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let raw: Option<String> = self
            .0
            .query_row("SELECT value FROM metadata WHERE key=?", [key], |r| {
                r.get(0)
            })
            .optional()?;
        raw.map(|s| serde_json::from_str(&s).map_err(Into::into))
            .transpose()
    }
    pub fn set_metadata(&self, key: &str, value: &impl serde::Serialize) -> Result<()> {
        self.0.execute(
            "INSERT INTO metadata VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, serde_json::to_string(value)?],
        )?;
        Ok(())
    }
    pub fn workspaces(&self) -> Result<Vec<Workspace>> {
        Ok(self.metadata("workspaces")?.unwrap_or_default())
    }
    pub fn set_channels(&mut self, w: &str, channels: &[Channel]) -> Result<()> {
        let tx = self.0.transaction()?;
        let previous = {
            let mut statement = tx.prepare("SELECT id FROM channels WHERE workspace=?")?;
            statement
                .query_map([w], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        for removed in previous
            .into_iter()
            .filter(|id| !channels.iter().any(|c| c.id == *id))
        {
            tx.execute(
                "DELETE FROM pins WHERE workspace=? AND channel=?",
                params![w, removed],
            )?;
            tx.execute("DELETE FROM reactions WHERE workspace=?1 AND message IN (SELECT id FROM messages WHERE workspace=?1 AND channel=?2)",params![w,removed])?;
            tx.execute(
                "DELETE FROM messages WHERE workspace=? AND channel=?",
                params![w, removed],
            )?;
            tx.execute("DELETE FROM outbox WHERE workspace=? AND json_extract(event,'$.payload.channel_id')=?",params![w,removed])?;
            tx.execute(
                "DELETE FROM pending_reads WHERE workspace=? AND channel=?",
                params![w, removed],
            )?;
            tx.execute(
                "DELETE FROM reads WHERE workspace=? AND channel=?",
                params![w, removed],
            )?;
            tx.execute(
                "DELETE FROM metadata WHERE key=?",
                [format!("attachments:{w}:{removed}")],
            )?;
        }
        tx.execute("DELETE FROM channels WHERE workspace=?", [w])?;
        for ch in channels {
            tx.execute(
                "INSERT INTO channels VALUES(?,?,?)",
                params![w, ch.id, serde_json::to_string(ch)?],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn channels(&self, w: &str) -> Result<Vec<Channel>> {
        let mut s=self.0.prepare("SELECT json FROM channels WHERE workspace=? ORDER BY json_extract(json,'$.created_at')")?;
        let raw = s
            .query_map([w], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        raw.into_iter()
            .map(|s| Ok(serde_json::from_str(&s)?))
            .collect()
    }
    pub fn cursor(&self, w: &str) -> Result<i64> {
        Ok(self.metadata(&format!("cursor:{w}"))?.unwrap_or(0))
    }
    pub fn apply_page(&mut self, w: &str, events: &[ServerEvent], cursor: i64) -> Result<()> {
        let own_user: Option<String> = self.metadata("user_id")?;
        let tx = self.0.transaction()?;
        for event in events {
            match event {
                ServerEvent::ReadStateUpdated {
                    channel_id,
                    user_id,
                    last_read_seq,
                } if Some(user_id) == own_user.as_ref() => {
                    tx.execute("INSERT INTO reads VALUES(?,?,?) ON CONFLICT(workspace,channel) DO UPDATE SET seq=MAX(seq,excluded.seq)",params![w,channel_id,last_read_seq])?;
                    tx.execute(
                        "DELETE FROM pending_reads WHERE workspace=? AND channel=? AND seq<=?",
                        params![w, channel_id, last_read_seq],
                    )?;
                }
                ServerEvent::PinUpdated {
                    channel_id,
                    message_id,
                    user_id,
                    pinned,
                    created_at,
                } => {
                    if *pinned {
                        tx.execute("INSERT INTO pins VALUES(?,?,?,?,?) ON CONFLICT(workspace,message) DO UPDATE SET pinned_by=excluded.pinned_by,created_at=excluded.created_at",params![w,message_id,channel_id,user_id,created_at])?;
                    } else {
                        tx.execute(
                            "DELETE FROM pins WHERE workspace=? AND message=?",
                            params![w, message_id],
                        )?;
                    }
                }
                ServerEvent::MessageCreated { message, .. }
                | ServerEvent::MessageUpdated { message } => {
                    tx.execute("INSERT INTO messages(workspace,id,channel,parent,seq,body,deleted,json) VALUES(?,?,?,?,?,?,?,?) ON CONFLICT(workspace,id) DO UPDATE SET channel=excluded.channel,parent=excluded.parent,seq=excluded.seq,body=excluded.body,deleted=excluded.deleted,json=excluded.json",params![w,message.id,message.channel_id,message.parent_id,message.seq,message.body_md,message.deleted_at.is_some(),serde_json::to_string(message)?])?;
                    tx.execute(
                        "DELETE FROM outbox WHERE workspace=? AND id=?",
                        params![w, message.id],
                    )?;
                }
                ServerEvent::MessageDeleted { message_id, .. } => {
                    tx.execute(
                        "DELETE FROM pins WHERE workspace=? AND message=?",
                        params![w, message_id],
                    )?;
                    let raw: Option<String> = tx
                        .query_row(
                            "SELECT json FROM messages WHERE workspace=? AND id=?",
                            params![w, message_id],
                            |r| r.get(0),
                        )
                        .optional()?;
                    if let Some(raw) = raw {
                        let mut m: MessageDto = serde_json::from_str(&raw)?;
                        m.body_md.clear();
                        m.deleted_at = Some(1);
                        tx.execute("UPDATE messages SET deleted=1,body='',json=? WHERE workspace=? AND id=?",params![serde_json::to_string(&m)?,w,message_id])?;
                    }
                }
                ServerEvent::ReactionUpdated {
                    message_id,
                    emoji,
                    user_id,
                    added,
                    ..
                } => {
                    if *added {
                        tx.execute(
                            "INSERT OR IGNORE INTO reactions VALUES(?,?,?,?)",
                            params![w, message_id, user_id, emoji],
                        )?;
                    } else {
                        tx.execute("DELETE FROM reactions WHERE workspace=? AND message=? AND user=? AND emoji=?",params![w,message_id,user_id,emoji])?;
                    }
                }
                _ => {}
            }
        }
        tx.execute(
            "INSERT INTO metadata VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![format!("cursor:{w}"), cursor.to_string()],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn enqueue(&mut self, w: &str, user: &str, event: &ClientEvent) -> Result<String> {
        let ClientEvent::SendMessage {
            client_msg_id,
            channel_id,
            parent_id,
            topic_tag,
            body_md,
            ..
        } = event
        else {
            anyhow::bail!("only messages belong in the outbox")
        };
        let m = MessageDto {
            seq: 0,
            id: client_msg_id.clone(),
            channel_id: channel_id.clone(),
            user_id: user.into(),
            parent_id: parent_id.clone(),
            topic_tag: topic_tag.clone(),
            body_md: body_md.clone(),
            created_at: crate::timestamp(),
            edited_at: None,
            deleted_at: None,
        };
        let tx = self.0.transaction()?;
        tx.execute("INSERT INTO messages(workspace,id,channel,parent,seq,body,deleted,json) VALUES(?,?,?,?,0,?,0,?)",params![w,m.id,m.channel_id,m.parent_id,m.body_md,serde_json::to_string(&m)?])?;
        tx.execute(
            "INSERT INTO outbox(workspace,id,event) VALUES(?,?,?)",
            params![w, m.id, serde_json::to_string(event)?],
        )?;
        tx.commit()?;
        Ok(m.id)
    }
    pub fn next_pending(&self, w: &str) -> Result<Option<(String, ClientEvent)>> {
        let raw:Option<(String,String)>=self.0.query_row("SELECT id,event FROM outbox WHERE workspace=? AND status='pending' ORDER BY rowid LIMIT 1",[w],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        raw.map(|(id, s)| Ok((id, serde_json::from_str(&s)?)))
            .transpose()
    }
    pub fn fail(&self, w: &str, id: &str, error: &str) -> Result<()> {
        self.0.execute(
            "UPDATE outbox SET status='failed',error=? WHERE workspace=? AND id=?",
            params![error, w, id],
        )?;
        Ok(())
    }
    pub fn retry(&self, w: &str, id: &str) -> Result<()> {
        self.0.execute(
            "UPDATE outbox SET status='pending',error=NULL WHERE workspace=? AND id=?",
            params![w, id],
        )?;
        Ok(())
    }
    pub fn acknowledge(&self, w: &str, id: &str) -> Result<()> {
        self.0.execute("DELETE FROM outbox WHERE workspace=? AND id=? AND EXISTS(SELECT 1 FROM messages m WHERE m.workspace=outbox.workspace AND m.id=outbox.id AND m.seq>0)",params![w,id])?;
        Ok(())
    }
    pub fn messages(
        &self,
        w: &str,
        channel: &str,
        limit: usize,
        user: &str,
    ) -> Result<Vec<CachedMessage>> {
        let mut s=self.0.prepare("SELECT m.json,o.status,o.error FROM messages m LEFT JOIN outbox o ON o.workspace=m.workspace AND o.id=m.id WHERE m.workspace=? AND m.channel=? AND m.parent IS NULL ORDER BY CASE WHEN m.seq=0 THEN 1 ELSE 0 END DESC,m.seq DESC,m.rowid DESC LIMIT ?")?;
        let rows = s
            .query_map(params![w, channel, limit as i64], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, Option<String>>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        self.decode_rows(w, user, rows)
    }
    fn decode_rows(
        &self,
        w: &str,
        user: &str,
        rows: Vec<(String, Option<String>, Option<String>)>,
    ) -> Result<Vec<CachedMessage>> {
        let mut out = vec![];
        for (raw, status, error) in rows.into_iter().rev() {
            let message: MessageDto = serde_json::from_str(&raw)?;
            let mut s=self.0.prepare("SELECT emoji,COUNT(*),MAX(user=?1) FROM reactions WHERE workspace=?2 AND message=?3 GROUP BY emoji ORDER BY emoji")?;
            let reactions = s
                .query_map(params![user, w, message.id], |r| {
                    Ok(ReactionCount {
                        emoji: r.get(0)?,
                        count: r.get(1)?,
                        mine: r.get(2)?,
                    })
                })?
                .collect::<rusqlite::Result<_>>()?;
            out.push(CachedMessage {
                message,
                status: match status.as_deref() {
                    Some("failed") => OutboxStatus::Failed(error.unwrap_or_default()),
                    Some(_) => OutboxStatus::Pending,
                    None => OutboxStatus::Sent,
                },
                reactions,
            });
        }
        Ok(out)
    }
    pub fn thread(&self, w: &str, parent: &str, user: &str) -> Result<Vec<CachedMessage>> {
        let mut s=self.0.prepare("SELECT m.json,o.status,o.error FROM messages m LEFT JOIN outbox o ON o.workspace=m.workspace AND o.id=m.id WHERE m.workspace=? AND (m.id=? OR m.parent=?) ORDER BY m.id!=? ,m.seq=0,m.seq,m.rowid")?;
        let rows = s
            .query_map(params![w, parent, parent, parent], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        self.decode_rows(w, user, rows.into_iter().rev().collect())
    }
    pub fn reply_counts(&self, w: &str) -> Result<std::collections::BTreeMap<String, usize>> {
        let mut s=self.0.prepare("SELECT parent,COUNT(*) FROM messages WHERE workspace=? AND parent IS NOT NULL AND deleted=0 GROUP BY parent")?;
        Ok(
            s.query_map([w], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as usize)))?
                .collect::<rusqlite::Result<_>>()?,
        )
    }
    pub fn pins(&self, w: &str) -> Result<Vec<shared_proto::PinnedMessage>> {
        let mut s=self.0.prepare("SELECT m.json,p.pinned_by,p.created_at FROM pins p JOIN messages m ON m.workspace=p.workspace AND m.id=p.message WHERE p.workspace=? AND m.deleted=0 ORDER BY p.created_at DESC")?;
        let rows = s
            .query_map([w], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|(raw, user, created_at)| {
                Ok(shared_proto::PinnedMessage {
                    message: serde_json::from_str(&raw)?,
                    pinned_by: user,
                    created_at,
                })
            })
            .collect()
    }
    pub fn search(&self, w: &str, text: &str) -> Result<Vec<MessageDto>> {
        if text.trim().is_empty() {
            return Ok(vec![]);
        }
        let query = format!("\"{}\"", text.replace('"', "\"\""));
        let mut s=self.0.prepare("SELECT m.json FROM messages m JOIN messages_fts f ON f.rowid=m.rowid WHERE m.workspace=? AND messages_fts MATCH ? ORDER BY m.seq DESC LIMIT 50")?;
        let raw = s
            .query_map(params![w, query], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        raw.into_iter()
            .map(|s| Ok(serde_json::from_str(&s)?))
            .collect()
    }
    pub fn mark_read(&self, w: &str, ch: &str) -> Result<()> {
        let tx = self.0.unchecked_transaction()?;
        tx.execute("INSERT INTO reads VALUES(?,?,COALESCE((SELECT MAX(seq) FROM messages WHERE workspace=?1 AND channel=?2),0)) ON CONFLICT(workspace,channel) DO UPDATE SET seq=MAX(seq,excluded.seq)",params![w,ch])?;
        tx.execute("INSERT INTO pending_reads SELECT workspace,channel,seq FROM reads WHERE workspace=? AND channel=? ON CONFLICT(workspace,channel) DO UPDATE SET seq=MAX(seq,excluded.seq)",params![w,ch])?;
        tx.commit()?;
        Ok(())
    }
    pub fn pending_reads(&self, w: &str) -> Result<Vec<(String, i64)>> {
        let mut q = self
            .0
            .prepare("SELECT channel,seq FROM pending_reads WHERE workspace=?")?;
        Ok(q.query_map([w], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?)
    }
    pub fn needs_read(&self, w: &str, ch: &str) -> Result<bool> {
        Ok(self.0.query_row("SELECT COALESCE((SELECT MAX(seq) FROM messages WHERE workspace=?1 AND channel=?2),0)>COALESCE((SELECT seq FROM reads WHERE workspace=?1 AND channel=?2),0)",params![w,ch],|r|r.get(0))?)
    }
    pub fn claim_notification(&self, w: &str, m: &MessageDto) -> Result<bool> {
        let fresh = self.0.execute(
            "INSERT OR IGNORE INTO notification_seen VALUES(?,?,?)",
            params![w, m.id, m.created_at],
        )? > 0;
        self.0.execute("DELETE FROM notification_seen WHERE rowid IN (SELECT rowid FROM notification_seen WHERE workspace=? ORDER BY created_at DESC LIMIT -1 OFFSET 1000)",[w])?;
        Ok(fresh)
    }
    pub fn message(&self, w: &str, id: &str) -> Result<Option<MessageDto>> {
        let raw: Option<String> = self
            .0
            .query_row(
                "SELECT json FROM messages WHERE workspace=? AND id=?",
                params![w, id],
                |r| r.get(0),
            )
            .optional()?;
        raw.map(|s| Ok(serde_json::from_str(&s)?)).transpose()
    }
    pub fn unread_messages(&self, w: &str, user: &str) -> Result<Vec<MessageDto>> {
        let mut q=self.0.prepare("SELECT m.json FROM messages m JOIN channels ch ON ch.workspace=m.workspace AND ch.id=m.channel LEFT JOIN reads r ON r.workspace=m.workspace AND r.channel=m.channel WHERE m.workspace=? AND m.seq>COALESCE(r.seq,0) AND m.deleted=0 AND json_extract(m.json,'$.user_id')!=? ORDER BY m.seq DESC LIMIT 200")?;
        let rows = q
            .query_map(params![w, user], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|raw| Ok(serde_json::from_str(&raw)?))
            .collect()
    }
    pub fn unread(&self, w: &str, user: &str) -> Result<std::collections::BTreeMap<String, i64>> {
        let mut s=self.0.prepare("SELECT m.channel,COUNT(*) FROM messages m LEFT JOIN reads r ON r.workspace=m.workspace AND r.channel=m.channel WHERE m.workspace=? AND m.seq>COALESCE(r.seq,0) AND m.deleted=0 AND json_extract(m.json,'$.user_id')!=? GROUP BY m.channel")?;
        Ok(
            s.query_map(params![w, user], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?,
        )
    }
}
