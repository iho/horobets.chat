use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub email: String,
    pub display_name: String,
    #[serde(default)]
    pub handle: Option<String>,
    #[serde(default)]
    pub bio: String,
    pub avatar_url: Option<String>,
    pub created_at: i64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub created_at: i64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Channel {
    pub id: String,
    pub name: String,
    pub topic: Option<String>,
    pub is_private: bool,
    #[serde(default)]
    pub is_direct: bool,
    #[serde(default)]
    pub is_saved: bool,
    #[serde(default)]
    pub peer_user_id: Option<String>,
    pub created_by: String,
    pub created_at: i64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessageDto {
    pub seq: i64,
    pub id: String,
    pub channel_id: String,
    pub user_id: String,
    pub parent_id: Option<String>,
    pub topic_tag: Option<String>,
    pub body_md: String,
    pub edited_at: Option<i64>,
    pub deleted_at: Option<i64>,
    pub created_at: i64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reaction {
    pub message_id: String,
    pub user_id: String,
    pub emoji: String,
    pub created_at: i64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Attachment {
    pub id: String,
    pub message_id: Option<String>,
    pub uploader_id: String,
    pub filename: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub created_at: i64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresenceStatus {
    Online,
    Away,
    Offline,
}

#[cfg(test)]
mod compatibility_tests {
    use super::*;
    #[test]
    fn existing_cached_users_and_channels_accept_new_optional_fields() {
        let user:User=serde_json::from_str(r#"{"id":"u","email":"u@example.com","display_name":"User","avatar_url":null,"created_at":1}"#).unwrap();
        assert!(user.handle.is_none());
        let channel:Channel=serde_json::from_str(r#"{"id":"c","name":"general","topic":null,"is_private":false,"created_by":"u","created_at":1}"#).unwrap();
        assert!(!channel.is_direct);
        assert!(channel.peer_user_id.is_none());
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemberInfo {
    pub user: User,
    pub role: String,
    pub joined_at: i64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PinnedMessage {
    pub message: MessageDto,
    pub pinned_by: String,
    pub created_at: i64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModerationAction {
    pub seq: i64,
    pub channel_id: String,
    pub message_id: String,
    pub actor_id: String,
    pub action: String,
    pub created_at: i64,
}
