use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub email: String,
    pub display_name: String,
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
