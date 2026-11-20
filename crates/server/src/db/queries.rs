use crate::{Error, Result};
use rusqlite::{Connection, OptionalExtension, params};
use shared_proto::{ClientEvent, MessageDto, ServerEvent};
pub fn message(r: &rusqlite::Row<'_>) -> rusqlite::Result<MessageDto> {
    Ok(MessageDto {
        seq: r.get(0)?,
        id: r.get(1)?,
        channel_id: r.get(2)?,
        user_id: r.get(3)?,
        parent_id: r.get(4)?,
        topic_tag: r.get(5)?,
        body_md: r.get(6)?,
        edited_at: r.get(7)?,
        deleted_at: r.get(8)?,
        created_at: r.get(9)?,
    })
}
pub const COLS: &str =
    "seq,id,channel_id,user_id,parent_id,topic_tag,body_md,edited_at,deleted_at,created_at";
pub fn get(c: &Connection, id: &str) -> Result<MessageDto> {
    c.query_row(
        &format!("SELECT {COLS} FROM messages WHERE id=?"),
        [id],
        message,
    )
    .optional()?
    .ok_or(Error::NotFound)
}
pub fn access(c: &Connection, channel: &str, user: &str) -> Result<()> {
    let allowed:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM channels c WHERE c.id=?1 AND (c.is_private=0 OR EXISTS(SELECT 1 FROM channel_members m WHERE m.channel_id=c.id AND m.user_id=?2)))",[channel,user],|r|r.get(0))?;
    if allowed {
        Ok(())
    } else {
        Err(Error::Forbidden)
    }
}
pub fn event_channel(e: &ServerEvent) -> Option<&str> {
    match e {
        ServerEvent::MessageCreated { message, .. } | ServerEvent::MessageUpdated { message } => {
            Some(&message.channel_id)
        }
        ServerEvent::MessageDeleted { channel_id, .. }
        | ServerEvent::ReactionUpdated { channel_id, .. }
        | ServerEvent::ReadStateUpdated { channel_id, .. }
        | ServerEvent::PinUpdated { channel_id, .. }
        | ServerEvent::UserTyping { channel_id, .. } => Some(channel_id),
        _ => None,
    }
}
pub fn apply(c: &mut Connection, user: &str, event: ClientEvent) -> Result<(ServerEvent, bool)> {
    apply_as(c, user, false, event)
}
pub fn personal(c: &Connection, ch: &str) -> Result<bool> {
    Ok(c.query_row("SELECT EXISTS(SELECT 1 FROM direct_channels WHERE channel_id=?1) OR EXISTS(SELECT 1 FROM saved_channels WHERE channel_id=?1)",[ch],|r|r.get(0))?)
}
pub fn apply_as(
    c: &mut Connection,
    user: &str,
    moderator: bool,
    event: ClientEvent,
) -> Result<(ServerEvent, bool)> {
    let tx = c.transaction()?;
    let e = match event {
        ClientEvent::PinMessage { message_id, pinned } => {
            let m = get(&tx, &message_id)?;
            access(&tx, &m.channel_id, user)?;
            let private = personal(&tx, &m.channel_id)?;
            if !private && !moderator {
                return Err(Error::Forbidden);
            }
            if m.deleted_at.is_some() {
                return Err(Error::Bad("Message is deleted".into()));
            }
            let at = crate::now();
            if pinned {
                tx.execute("INSERT INTO message_pins VALUES(?,?,?) ON CONFLICT(message_id) DO UPDATE SET pinned_by=excluded.pinned_by,created_at=excluded.created_at",params![message_id,user,at])?;
            } else {
                tx.execute("DELETE FROM message_pins WHERE message_id=?", [&message_id])?;
            }
            if !private {
                tx.execute("INSERT INTO moderation_actions(channel_id,message_id,actor_id,action,created_at) VALUES(?,?,?,?,?)",params![m.channel_id,message_id,user,if pinned{"message_pinned"}else{"message_unpinned"},at])?;
            }
            ServerEvent::PinUpdated {
                channel_id: m.channel_id,
                message_id,
                user_id: user.into(),
                pinned,
                created_at: at,
            }
        }
        ClientEvent::Ping { ts } => return Ok((ServerEvent::Pong { ts }, false)),
        ClientEvent::SendMessage {
            client_msg_id,
            channel_id,
            parent_id,
            topic_tag,
            body_md,
            attachment_ids,
        } => {
            if client_msg_id.is_empty()
                || client_msg_id.len() > 128
                || (body_md.trim().is_empty() && attachment_ids.is_empty())
                || body_md.len() > 64000
                || attachment_ids.len() > 20
                || topic_tag.as_ref().is_some_and(|s| s.len() > 200)
            {
                return Err(Error::Bad("invalid message".into()));
            }
            access(&tx, &channel_id, user)?;
            if let Some(existing) = tx
                .query_row(
                    &format!("SELECT {COLS} FROM messages WHERE id=?"),
                    [&client_msg_id],
                    message,
                )
                .optional()?
            {
                if existing.user_id != user || existing.channel_id != channel_id {
                    return Err(Error::Forbidden);
                }
                return Ok((
                    ServerEvent::MessageCreated {
                        client_msg_id: Some(client_msg_id),
                        message: existing,
                    },
                    false,
                ));
            }
            if let Some(p) = &parent_id {
                let parent = get(&tx, p)?;
                if parent.channel_id != channel_id || parent.deleted_at.is_some() {
                    return Err(Error::Bad("invalid thread parent".into()));
                }
            }
            tx.execute("INSERT INTO messages(id,channel_id,user_id,parent_id,topic_tag,body_md,created_at) VALUES(?,?,?,?,?,?,?)",params![client_msg_id,channel_id,user,parent_id,topic_tag,body_md,crate::now()])?;
            for attachment in attachment_ids {
                let n=tx.execute("UPDATE attachments SET message_id=? WHERE id=? AND uploader_id=? AND message_id IS NULL",params![client_msg_id,attachment,user])?;
                if n != 1 {
                    return Err(Error::Bad("invalid attachment".into()));
                }
            }
            ServerEvent::MessageCreated {
                client_msg_id: Some(client_msg_id.clone()),
                message: get(&tx, &client_msg_id)?,
            }
        }
        ClientEvent::EditMessage {
            message_id,
            body_md,
        } => {
            if body_md.trim().is_empty() || body_md.len() > 64000 {
                return Err(Error::Bad("invalid message".into()));
            }
            let m = get(&tx, &message_id)?;
            access(&tx, &m.channel_id, user)?;
            if m.user_id != user || m.deleted_at.is_some() {
                return Err(Error::Forbidden);
            }
            tx.execute(
                "UPDATE messages SET body_md=?,edited_at=? WHERE id=?",
                params![body_md, crate::now(), message_id],
            )?;
            ServerEvent::MessageUpdated {
                message: get(&tx, &message_id)?,
            }
        }
        ClientEvent::DeleteMessage { message_id } => {
            let m = get(&tx, &message_id)?;
            access(&tx, &m.channel_id, user)?;
            let private = personal(&tx, &m.channel_id)?;
            if m.user_id != user && (!moderator || private) {
                return Err(Error::Forbidden);
            }
            tx.execute(
                "UPDATE messages SET body_md='',deleted_at=? WHERE id=?",
                params![crate::now(), message_id],
            )?;
            tx.execute("DELETE FROM message_pins WHERE message_id=?", [&message_id])?;
            if !private {
                tx.execute("INSERT INTO moderation_actions(channel_id,message_id,actor_id,action,created_at) VALUES(?,?,?,'message_deleted',?)",params![m.channel_id,message_id,user,crate::now()])?;
            }
            ServerEvent::MessageDeleted {
                channel_id: m.channel_id,
                message_id,
                seq: m.seq,
            }
        }
        ClientEvent::ToggleReaction { message_id, emoji } => {
            if emoji.is_empty() || emoji.len() > 64 {
                return Err(Error::Bad("invalid emoji".into()));
            }
            let m = get(&tx, &message_id)?;
            access(&tx, &m.channel_id, user)?;
            if m.deleted_at.is_some() {
                return Err(Error::Bad("message deleted".into()));
            }
            let removed = tx.execute(
                "DELETE FROM reactions WHERE message_id=? AND user_id=? AND emoji=?",
                params![message_id, user, emoji],
            )?;
            if removed == 0 {
                tx.execute(
                    "INSERT INTO reactions VALUES(?,?,?,?)",
                    params![message_id, user, emoji, crate::now()],
                )?;
            }
            ServerEvent::ReactionUpdated {
                channel_id: m.channel_id,
                message_id,
                emoji,
                user_id: user.into(),
                added: removed == 0,
            }
        }
        ClientEvent::MarkRead {
            channel_id,
            last_read_seq,
        } => {
            access(&tx, &channel_id, user)?;
            let max: i64 = tx.query_row(
                "SELECT COALESCE(MAX(seq),0) FROM messages WHERE channel_id=?",
                [&channel_id],
                |r| r.get(0),
            )?;
            if last_read_seq < 0 || last_read_seq > max {
                return Err(Error::Bad("invalid read cursor".into()));
            }
            tx.execute("INSERT INTO channel_members(channel_id,user_id,last_read_seq) VALUES(?,?,?) ON CONFLICT(channel_id,user_id) DO UPDATE SET last_read_seq=MAX(last_read_seq,excluded.last_read_seq)",params![channel_id,user,last_read_seq])?;
            let actual = tx.query_row(
                "SELECT last_read_seq FROM channel_members WHERE channel_id=? AND user_id=?",
                params![channel_id, user],
                |r| r.get(0),
            )?;
            ServerEvent::ReadStateUpdated {
                channel_id,
                user_id: user.into(),
                last_read_seq: actual,
            }
        }
        ClientEvent::Typing {
            channel_id,
            is_typing,
        } => {
            access(&tx, &channel_id, user)?;
            return Ok((
                ServerEvent::UserTyping {
                    channel_id,
                    user_id: user.into(),
                    is_typing,
                },
                true,
            ));
        }
    };
    tx.execute(
        "INSERT INTO events(event_json) VALUES(?)",
        [serde_json::to_string(&e).map_err(|e| Error::Internal(e.into()))?],
    )?;
    tx.commit()?;
    Ok((e, true))
}
