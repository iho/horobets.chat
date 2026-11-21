use crate::{MessageDto, PresenceStatus};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum ClientEvent {
    PinMessage {
        message_id: String,
        pinned: bool,
    },
    Ping {
        ts: i64,
    },
    SendMessage {
        client_msg_id: String,
        channel_id: String,
        parent_id: Option<String>,
        topic_tag: Option<String>,
        body_md: String,
        attachment_ids: Vec<String>,
    },
    EditMessage {
        message_id: String,
        body_md: String,
    },
    DeleteMessage {
        message_id: String,
    },
    ToggleReaction {
        message_id: String,
        emoji: String,
    },
    MarkRead {
        channel_id: String,
        last_read_seq: i64,
    },
    Typing {
        channel_id: String,
        is_typing: bool,
    },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum ServerEvent {
    Notification {
        workspace_id: String,
        message: MessageDto,
        author: String,
    },
    PinUpdated {
        channel_id: String,
        message_id: String,
        user_id: String,
        pinned: bool,
        created_at: i64,
    },
    ChannelsChanged,
    Pong {
        ts: i64,
    },
    Hello {
        user_id: String,
        latest_seq: i64,
    },
    MessageCreated {
        client_msg_id: Option<String>,
        message: MessageDto,
    },
    MessageUpdated {
        message: MessageDto,
    },
    MessageDeleted {
        channel_id: String,
        message_id: String,
        seq: i64,
    },
    ReactionUpdated {
        channel_id: String,
        message_id: String,
        emoji: String,
        user_id: String,
        added: bool,
    },
    ReadStateUpdated {
        channel_id: String,
        user_id: String,
        last_read_seq: i64,
    },
    UserTyping {
        channel_id: String,
        user_id: String,
        is_typing: bool,
    },
    PresenceChanged {
        user_id: String,
        status: PresenceStatus,
    },
    Error {
        code: String,
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        client_msg_id: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_variants_round_trip() {
        let s = String::from("id");
        let m = MessageDto {
            seq: 1,
            id: s.clone(),
            channel_id: s.clone(),
            user_id: s.clone(),
            parent_id: None,
            topic_tag: Some("rust".into()),
            body_md: "hello 🌍".into(),
            edited_at: None,
            deleted_at: None,
            created_at: 1,
        };
        let clients = vec![
            ClientEvent::PinMessage {
                message_id: s.clone(),
                pinned: true,
            },
            ClientEvent::Ping { ts: 1 },
            ClientEvent::SendMessage {
                client_msg_id: s.clone(),
                channel_id: s.clone(),
                parent_id: None,
                topic_tag: None,
                body_md: s.clone(),
                attachment_ids: vec![],
            },
            ClientEvent::EditMessage {
                message_id: s.clone(),
                body_md: s.clone(),
            },
            ClientEvent::DeleteMessage {
                message_id: s.clone(),
            },
            ClientEvent::ToggleReaction {
                message_id: s.clone(),
                emoji: "👍".into(),
            },
            ClientEvent::MarkRead {
                channel_id: s.clone(),
                last_read_seq: 1,
            },
            ClientEvent::Typing {
                channel_id: s.clone(),
                is_typing: true,
            },
        ];
        for e in clients {
            assert_eq!(
                e,
                serde_json::from_str(&serde_json::to_string(&e).unwrap()).unwrap()
            );
        }
        let events = vec![
            ServerEvent::Notification {
                workspace_id: s.clone(),
                message: m.clone(),
                author: s.clone(),
            },
            ServerEvent::PinUpdated {
                channel_id: s.clone(),
                message_id: s.clone(),
                user_id: s.clone(),
                pinned: true,
                created_at: 1,
            },
            ServerEvent::ChannelsChanged,
            ServerEvent::Pong { ts: 1 },
            ServerEvent::Hello {
                user_id: s.clone(),
                latest_seq: 1,
            },
            ServerEvent::MessageCreated {
                client_msg_id: Some(s.clone()),
                message: m.clone(),
            },
            ServerEvent::MessageUpdated { message: m },
            ServerEvent::MessageDeleted {
                channel_id: s.clone(),
                message_id: s.clone(),
                seq: 1,
            },
            ServerEvent::ReactionUpdated {
                channel_id: s.clone(),
                message_id: s.clone(),
                emoji: s.clone(),
                user_id: s.clone(),
                added: true,
            },
            ServerEvent::ReadStateUpdated {
                channel_id: s.clone(),
                user_id: s.clone(),
                last_read_seq: 1,
            },
            ServerEvent::UserTyping {
                channel_id: s.clone(),
                user_id: s.clone(),
                is_typing: true,
            },
            ServerEvent::PresenceChanged {
                user_id: s.clone(),
                status: PresenceStatus::Online,
            },
            ServerEvent::Error {
                client_msg_id: None,
                code: s.clone(),
                message: s,
            },
        ];
        for e in events {
            assert_eq!(
                e,
                serde_json::from_str(&serde_json::to_string(&e).unwrap()).unwrap()
            );
        }
    }
}
