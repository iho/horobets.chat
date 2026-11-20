CREATE TABLE IF NOT EXISTS channels(id TEXT PRIMARY KEY,name TEXT NOT NULL,topic TEXT,is_private INTEGER NOT NULL DEFAULT 0,created_by TEXT NOT NULL,created_at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS channel_members(channel_id TEXT REFERENCES channels(id),user_id TEXT,last_read_seq INTEGER NOT NULL DEFAULT 0,notification_level TEXT DEFAULT 'all',PRIMARY KEY(channel_id,user_id));
CREATE TABLE IF NOT EXISTS messages(seq INTEGER PRIMARY KEY AUTOINCREMENT,id TEXT UNIQUE NOT NULL,channel_id TEXT NOT NULL REFERENCES channels(id),user_id TEXT NOT NULL,parent_id TEXT REFERENCES messages(id),topic_tag TEXT,body_md TEXT NOT NULL,edited_at INTEGER,deleted_at INTEGER,created_at INTEGER NOT NULL);
CREATE INDEX IF NOT EXISTS messages_channel_seq ON messages(channel_id,seq);
CREATE TABLE IF NOT EXISTS reactions(message_id TEXT REFERENCES messages(id),user_id TEXT NOT NULL,emoji TEXT NOT NULL,created_at INTEGER NOT NULL,PRIMARY KEY(message_id,user_id,emoji));
CREATE TABLE IF NOT EXISTS attachments(id TEXT PRIMARY KEY,message_id TEXT REFERENCES messages(id),uploader_id TEXT NOT NULL,filename TEXT NOT NULL,mime_type TEXT NOT NULL,size_bytes INTEGER NOT NULL,storage_key TEXT NOT NULL,created_at INTEGER NOT NULL);
CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(message_id UNINDEXED,channel_id UNINDEXED,body_md,tokenize='porter unicode61');
CREATE TRIGGER IF NOT EXISTS messages_ai AFTER INSERT ON messages WHEN new.deleted_at IS NULL BEGIN INSERT INTO messages_fts VALUES(new.id,new.channel_id,new.body_md); END;
CREATE TRIGGER IF NOT EXISTS messages_au AFTER UPDATE ON messages BEGIN DELETE FROM messages_fts WHERE message_id=old.id; INSERT INTO messages_fts SELECT new.id,new.channel_id,new.body_md WHERE new.deleted_at IS NULL; END;
CREATE TRIGGER IF NOT EXISTS messages_ad AFTER DELETE ON messages BEGIN DELETE FROM messages_fts WHERE message_id=old.id; END;
-- Every durable mutation has its own cursor, including edits and reactions.
CREATE TABLE IF NOT EXISTS events(seq INTEGER PRIMARY KEY AUTOINCREMENT,event_json TEXT NOT NULL);

CREATE TABLE IF NOT EXISTS attachment_previews(attachment_id TEXT PRIMARY KEY REFERENCES attachments(id),storage_key TEXT NOT NULL);

CREATE TABLE IF NOT EXISTS direct_channels(channel_id TEXT PRIMARY KEY REFERENCES channels(id),pair_key TEXT NOT NULL UNIQUE,user_a TEXT NOT NULL,user_b TEXT NOT NULL,CHECK(user_a<>user_b));

CREATE TABLE IF NOT EXISTS saved_channels(user_id TEXT PRIMARY KEY,channel_id TEXT NOT NULL UNIQUE REFERENCES channels(id));
CREATE TABLE IF NOT EXISTS message_pins(message_id TEXT PRIMARY KEY REFERENCES messages(id),pinned_by TEXT NOT NULL,created_at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS moderation_actions(seq INTEGER PRIMARY KEY AUTOINCREMENT,channel_id TEXT NOT NULL REFERENCES channels(id),message_id TEXT NOT NULL REFERENCES messages(id),actor_id TEXT NOT NULL,action TEXT NOT NULL,created_at INTEGER NOT NULL);
