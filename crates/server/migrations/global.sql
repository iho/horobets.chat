CREATE TABLE IF NOT EXISTS users(id TEXT PRIMARY KEY,email TEXT UNIQUE NOT NULL,password_hash TEXT NOT NULL,display_name TEXT NOT NULL,avatar_url TEXT,created_at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS workspaces(id TEXT PRIMARY KEY,slug TEXT UNIQUE NOT NULL,name TEXT NOT NULL,db_path TEXT NOT NULL,created_at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS workspace_members(workspace_id TEXT REFERENCES workspaces(id),user_id TEXT REFERENCES users(id),role TEXT NOT NULL CHECK(role IN ('owner','admin','member','guest')),joined_at INTEGER NOT NULL,PRIMARY KEY(workspace_id,user_id));
CREATE TABLE IF NOT EXISTS push_tokens(id TEXT PRIMARY KEY,user_id TEXT NOT NULL REFERENCES users(id),platform TEXT CHECK(platform IN ('apns','fcm')),token TEXT NOT NULL,updated_at INTEGER NOT NULL);

CREATE TABLE IF NOT EXISTS user_avatars(user_id TEXT PRIMARY KEY REFERENCES users(id),storage_key TEXT NOT NULL,updated_at INTEGER NOT NULL);

CREATE TABLE IF NOT EXISTS user_handles(user_id TEXT PRIMARY KEY REFERENCES users(id),handle TEXT NOT NULL UNIQUE COLLATE NOCASE);

CREATE TABLE IF NOT EXISTS workspace_moderators(workspace_id TEXT NOT NULL,user_id TEXT NOT NULL,assigned_by TEXT NOT NULL,created_at INTEGER NOT NULL,PRIMARY KEY(workspace_id,user_id),FOREIGN KEY(workspace_id,user_id) REFERENCES workspace_members(workspace_id,user_id));
CREATE TABLE IF NOT EXISTS user_details(user_id TEXT PRIMARY KEY REFERENCES users(id),bio TEXT NOT NULL DEFAULT '');

CREATE TABLE IF NOT EXISTS push_environments(device_id TEXT PRIMARY KEY REFERENCES push_tokens(id) ON DELETE CASCADE,environment TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS notification_jobs(id INTEGER PRIMARY KEY AUTOINCREMENT,workspace_id TEXT NOT NULL,channel_id TEXT NOT NULL,message_id TEXT NOT NULL,user_id TEXT NOT NULL,device_id TEXT NOT NULL REFERENCES push_tokens(id) ON DELETE CASCADE,created_at INTEGER NOT NULL,attempts INTEGER NOT NULL DEFAULT 0,not_before INTEGER NOT NULL DEFAULT 0,UNIQUE(workspace_id,message_id,device_id));
CREATE INDEX IF NOT EXISTS notification_jobs_ready ON notification_jobs(not_before);
