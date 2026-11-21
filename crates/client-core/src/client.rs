use crate::{
    cache::Cache,
    sync::{self, Session},
};
use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::json;
use shared_proto::{Attachment, Channel, ClientEvent, MessageDto, ServerEvent, Workspace};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    time::{Duration, Instant},
};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream,
    tungstenite::{Message, client::IntoClientRequest},
};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum OutboxStatus {
    Sent,
    Pending,
    Failed(String),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReactionCount {
    pub emoji: String,
    pub count: i64,
    pub mine: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CachedMessage {
    pub message: MessageDto,
    pub status: OutboxStatus,
    pub reactions: Vec<ReactionCount>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MessageNotification {
    pub workspace_id: String,
    pub message: MessageDto,
    pub author: String,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Snapshot {
    #[serde(default)]
    pub unread_messages: Vec<MessageDto>,
    pub opened_message: Option<String>,
    #[serde(default)]
    pub notifications: Vec<MessageNotification>,
    pub server: String,
    pub user_id: Option<String>,
    pub workspaces: Vec<Workspace>,
    pub workspace: Option<String>,
    pub channels: Vec<Channel>,
    pub channel_members: Vec<String>,
    pub users: Vec<shared_proto::User>,
    pub people: Vec<shared_proto::MemberInfo>,
    pub pins: Vec<shared_proto::PinnedMessage>,
    pub moderation: Vec<shared_proto::ModerationAction>,
    pub me: Option<shared_proto::User>,
    pub avatars: BTreeMap<String, PathBuf>,
    pub image_previews: BTreeMap<String, PathBuf>,
    pub channel: Option<String>,
    pub messages: Vec<CachedMessage>,
    pub thread: Option<String>,
    pub thread_messages: Vec<CachedMessage>,
    pub unread: BTreeMap<String, i64>,
    pub reply_counts: BTreeMap<String, usize>,
    pub search_results: Vec<MessageDto>,
    pub search_query: String,
    pub attachments: Vec<Attachment>,
    pub message_attachments: BTreeMap<String, Vec<Attachment>>,
    pub connected: bool,
    pub busy: bool,
    pub error: Option<String>,
    pub notice: Option<String>,
    pub has_older: bool,
}
impl Snapshot {
    pub fn role(&self) -> &str {
        self.people
            .iter()
            .find(|m| Some(&m.user.id) == self.user_id.as_ref())
            .map(|m| m.role.as_str())
            .unwrap_or("member")
    }
    pub fn can_moderate(&self) -> bool {
        matches!(self.role(), "owner" | "admin" | "moderator")
    }
    pub fn can_manage_workspace(&self) -> bool {
        matches!(self.role(), "owner" | "admin")
    }
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum Command {
    Authenticate {
        server: String,
        email: String,
        password: String,
        register: bool,
    },
    Logout,
    SetActivity {
        active: bool,
        reading: bool,
    },
    MarkChannelRead(String),
    MarkAllRead,
    OpenMessage {
        workspace_id: String,
        channel_id: String,
        message_id: String,
    },
    RegisterDevice {
        device_id: String,
        platform: String,
        token: String,
        environment: String,
    },
    UnregisterDevice(String),
    SelectWorkspace(String),
    SelectChannel(String),
    SelectThread(Option<String>),
    CreateWorkspace {
        name: String,
        slug: String,
    },
    CreateChannel(String),
    CreatePrivateChannel(String),
    OpenDirect(String),
    ChannelMember {
        user_id: String,
        add: bool,
    },
    SaveProfile {
        display_name: String,
        handle: String,
        bio: Option<String>,
    },
    Invite(String),
    EditWorkspace {
        name: String,
        slug: String,
    },
    EditChannel {
        id: String,
        name: String,
        topic: String,
    },
    SetRole {
        user_id: String,
        role: String,
    },
    Send {
        body: String,
        parent_id: Option<String>,
        topic_tag: Option<String>,
    },
    Event(ClientEvent),
    Retry(String),
    Search(String),
    LoadOlder,
    Upload(PathBuf),
    UploadAvatar(PathBuf),
    RemoveAttachment(String),
    Download {
        id: String,
        path: PathBuf,
    },
    Refresh,
    ClearError,
}
pub struct ClientEngine {
    commands: async_channel::Sender<Command>,
    pub updates: async_channel::Receiver<Snapshot>,
}
impl ClientEngine {
    pub fn start(root: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&root)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))?;
        }
        let (tx, rx) = async_channel::bounded(128);
        let (updates_tx, updates) = async_channel::bounded(1);
        let drain = updates.clone();
        std::thread::Builder::new()
            .name("messenger-client".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build();
                match runtime {
                    Ok(rt) => rt.block_on(async {
                        let mut worker = Worker::new(root, updates_tx, drain);
                        if let Err(e) = worker.restore().await {
                            worker.view.error = Some(e.to_string());
                        }
                        worker.publish();
                        worker.run(rx).await;
                    }),
                    Err(e) => {
                        let _ = updates_tx.try_send(Snapshot {
                            error: Some(e.to_string()),
                            ..Default::default()
                        });
                    }
                }
            })?;
        Ok(Self {
            commands: tx,
            updates,
        })
    }
    pub fn send(&self, command: Command) -> Result<()> {
        self.commands
            .try_send(command)
            .map_err(|_| anyhow::anyhow!("Client command queue is full or the client has stopped"))
    }
}
pub fn timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
type Socket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;
struct Worker {
    root: PathBuf,
    http: reqwest::Client,
    session: Option<Session>,
    cache: Option<Cache>,
    socket: Option<Socket>,
    notification_socket: Option<Socket>,
    notification_retry: Instant,
    view: Snapshot,
    updates: async_channel::Sender<Snapshot>,
    drain: async_channel::Receiver<Snapshot>,
    retry_at: Instant,
    backoff: u64,
    in_flight: Option<String>,
    limit: usize,
    media: PathBuf,
    last_directory_refresh: Instant,
    last_frame: Instant,
    last_ping: Instant,
    active: bool,
    reading: bool,
}
impl Worker {
    fn new(
        root: PathBuf,
        updates: async_channel::Sender<Snapshot>,
        drain: async_channel::Receiver<Snapshot>,
    ) -> Self {
        Self {
            root,
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .expect("initialize HTTP client"),
            session: None,
            cache: None,
            socket: None,
            notification_socket: None,
            notification_retry: Instant::now(),
            view: Snapshot::default(),
            updates,
            drain,
            retry_at: Instant::now(),
            backoff: 1,
            in_flight: None,
            limit: 200,
            media: PathBuf::new(),
            last_directory_refresh: Instant::now(),
            last_frame: Instant::now(),
            last_ping: Instant::now(),
            active: false,
            reading: false,
        }
    }
    fn publish(&mut self) {
        if let Err(e) = self.refresh_view() {
            self.view.error = Some(e.to_string());
        }
        let _ = self.drain.try_recv();
        let _ = self.updates.try_send(self.view.clone());
    }
    fn refresh_view(&mut self) -> Result<()> {
        self.view.avatars.clear();
        for user in self.view.users.iter().chain(self.view.me.iter()) {
            if let Some((_, path)) = self.avatar_location(user)
                && path.is_file()
            {
                self.view.avatars.insert(user.id.clone(), path);
            }
        }
        self.view.image_previews.clear();
        for a in self
            .view
            .attachments
            .iter()
            .chain(self.view.message_attachments.values().flatten())
        {
            if let Some(path) = self.preview_path(a)
                && path.is_file()
            {
                self.view.image_previews.insert(a.id.clone(), path);
            }
        }
        if let (Some(cache), Some(session), Some(w)) =
            (&self.cache, &self.session, &self.view.workspace)
        {
            self.view.channels = cache.channels(w)?;
            self.view.people = cache.metadata(&format!("people:{w}"))?.unwrap_or_default();
            self.view.pins = cache.pins(w)?;
            self.view.moderation = if self.view.can_moderate() {
                cache
                    .metadata(&format!("moderation:{w}"))?
                    .unwrap_or_default()
            } else {
                vec![]
            };
            self.view.channel_members = if let Some(ch) = &self.view.channel {
                cache
                    .metadata(&format!("channel-members:{w}:{ch}"))?
                    .unwrap_or_default()
            } else {
                vec![]
            };
            self.view.unread = cache.unread(w, &session.user_id)?;
            self.view.unread_messages = cache.unread_messages(w, &session.user_id)?;
            self.view.reply_counts = cache.reply_counts(w)?;
            if let Some(ch) = &self.view.channel {
                self.view.messages = cache.messages(w, ch, self.limit, &session.user_id)?;
                self.view.has_older = self.view.messages.len() >= self.limit;
                let attachments: Vec<Attachment> = cache
                    .metadata(&format!("attachments:{w}:{ch}"))?
                    .unwrap_or_default();
                self.view.message_attachments.clear();
                for a in attachments {
                    if let Some(message) = a.message_id.clone() {
                        self.view
                            .message_attachments
                            .entry(message)
                            .or_default()
                            .push(a);
                    }
                }
            }
            for a in self.view.message_attachments.values().flatten() {
                if let Some(path) = self.preview_path(a)
                    && path.is_file()
                {
                    self.view.image_previews.insert(a.id.clone(), path);
                }
            }
            self.view.thread_messages = if let Some(parent) = &self.view.thread {
                cache.thread(w, parent, &session.user_id)?
            } else {
                vec![]
            };
            for cached in self
                .view
                .messages
                .iter()
                .chain(self.view.thread_messages.iter())
                .filter(|m| m.status != OutboxStatus::Sent)
            {
                if let Some(attachments) = cache.metadata::<Vec<Attachment>>(&format!(
                    "pending-attachments:{w}:{}",
                    cached.message.id
                ))? {
                    self.view
                        .message_attachments
                        .insert(cached.message.id.clone(), attachments);
                }
            }
            for a in self.view.message_attachments.values().flatten() {
                if let Some(path) = self.preview_path(a)
                    && path.is_file()
                {
                    self.view.image_previews.insert(a.id.clone(), path);
                }
            }
            self.view.search_results = cache.search(w, &self.view.search_query)?;
        }
        Ok(())
    }
    fn open_session(&mut self, session: Session) -> Result<()> {
        use std::hash::{Hash, Hasher};
        uuid::Uuid::parse_str(&session.user_id)?;
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        session.server.hash(&mut hash);
        let account = self
            .root
            .join(format!("{:x}-{}", hash.finish(), session.user_id));
        std::fs::create_dir_all(&account)?;
        self.media = account.join("media");
        std::fs::create_dir_all(&self.media)?;
        let cache = Cache::open(&account.join("cache.db"))?;
        cache.set_metadata("user_id", &session.user_id)?;
        self.view = Snapshot {
            server: session.server.clone(),
            user_id: Some(session.user_id.clone()),
            workspaces: cache.workspaces()?,
            me: cache.metadata("me")?,
            ..Default::default()
        };
        self.view.workspace = cache.metadata("workspace")?;
        self.session = Some(session);
        self.cache = Some(cache);
        if self.view.workspace.is_none() {
            self.view.workspace = self.view.workspaces.first().map(|w| w.id.clone());
        }
        if let Some(w) = &self.view.workspace {
            self.view.channels = self.cache.as_ref().unwrap().channels(w)?;
            self.view.users = self
                .cache
                .as_ref()
                .unwrap()
                .metadata(&format!("members:{w}"))?
                .unwrap_or_default();
            self.view.channel = self.view.channels.first().map(|c| c.id.clone());
        }
        self.retry_at = Instant::now();
        self.backoff = 1;
        Ok(())
    }
    async fn restore(&mut self) -> Result<()> {
        let path = self.root.join("session.json");
        if path.exists() {
            let session: Session = serde_json::from_slice(&std::fs::read(path)?)?;
            sync::normalize_server(&session.server)?;
            uuid::Uuid::parse_str(&session.user_id)?;
            self.open_session(session)?;
        }
        Ok(())
    }
    fn save_session(&self) -> Result<()> {
        use std::io::Write;
        let path = self.root.join("session.json");
        let mut options = std::fs::OpenOptions::new();
        options.create(true).truncate(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut f = options.open(&path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        }
        f.write_all(&serde_json::to_vec(
            self.session.as_ref().context("No session")?,
        )?)?;
        Ok(())
    }
    fn offline(&mut self, error: anyhow::Error) {
        self.socket = None;
        self.view.connected = false;
        self.in_flight = None;
        self.view.notice = Some(format!("Offline · retrying in {}s ({error})", self.backoff));
        self.retry_at = Instant::now() + Duration::from_secs(self.backoff);
        self.backoff = (self.backoff * 2).min(30);
    }
    async fn run(&mut self, commands: async_channel::Receiver<Command>) {
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {
                command=commands.recv()=>{let Ok(command)=command else{break};self.view.error=None;let result=self.command(command).await.and(self.mark_current_read().await);if let Err(e)=result{self.view.error=Some(e.to_string());}self.publish();},
                frame=async{match &mut self.socket{Some(s)=>s.next().await,None=>std::future::pending().await}}=>{let result=self.on_frame(frame).await;if let Err(e)=result{self.offline(e);}self.publish();},
                alert=async{match &mut self.notification_socket{Some(s)=>s.next().await,None=>std::future::pending().await}}=>{
                    if let Err(_error)=self.on_notification_frame(alert).await{self.notification_socket=None;self.notification_retry=Instant::now()+Duration::from_secs(3);}
                    self.publish();
                },
                _=tick.tick()=>{self.tick().await;}
            }
        }
        if let Some(mut socket) = self.socket.take() {
            let _ = socket.close(None).await;
        }
    }
    async fn tick(&mut self) {
        if self.session.is_some()
            && self.notification_socket.is_none()
            && Instant::now() >= self.notification_retry
            && self.connect_notifications().await.is_err()
        {
            self.notification_retry = Instant::now() + Duration::from_secs(15);
        }
        if self.socket.is_some() {
            match self.heartbeat().await {
                Ok(()) => {}
                Err(e) => {
                    self.offline(e);
                    self.publish();
                }
            }
        } else if self.session.is_some() && Instant::now() >= self.retry_at {
            if let Err(e) = self.connect().await {
                self.offline(e);
            }
            self.publish();
        }
    }
    async fn connect_notifications(&mut self) -> Result<()> {
        let session = self.session.as_ref().context("Sign in first")?;
        let mut url = url::Url::parse(&session.url("/users/me/notifications/ws"))?;
        url.set_scheme(if url.scheme() == "https" { "wss" } else { "ws" })
            .map_err(|_| anyhow::anyhow!("Invalid WebSocket URL"))?;
        let mut request = url.as_str().into_client_request()?;
        request.headers_mut().insert(
            "authorization",
            format!("Bearer {}", session.token).parse()?,
        );
        self.notification_socket = Some(
            tokio::time::timeout(
                Duration::from_secs(10),
                tokio_tungstenite::connect_async(request),
            )
            .await??
            .0,
        );
        Ok(())
    }
    fn add_notification(
        &mut self,
        workspace: &str,
        message: &MessageDto,
        author: String,
    ) -> Result<()> {
        if let (Some(cache), Some(user)) = (&self.cache, &self.view.user_id)
            && message.user_id != *user
            && cache.claim_notification(workspace, message)?
            && !(self.active
                && self.reading
                && self.view.workspace.as_deref() == Some(workspace)
                && self.view.channel.as_ref() == Some(&message.channel_id))
        {
            self.view.notifications.push(MessageNotification {
                workspace_id: workspace.into(),
                message: message.clone(),
                author,
            });
            if self.view.notifications.len() > 32 {
                self.view.notifications.remove(0);
            }
        }
        Ok(())
    }
    async fn on_notification_frame(
        &mut self,
        frame: Option<std::result::Result<Message, tokio_tungstenite::tungstenite::Error>>,
    ) -> Result<()> {
        match frame.context("Notification stream closed")?? {
            Message::Text(raw) => {
                if let ServerEvent::Notification {
                    workspace_id,
                    message,
                    author,
                } = serde_json::from_str(&raw)?
                {
                    self.add_notification(&workspace_id, &message, author)?;
                    if self.view.workspace.as_ref() == Some(&workspace_id) {
                        self.backfill().await?;
                        self.mark_current_read().await?;
                    }
                }
            }
            Message::Ping(data) => {
                if let Some(socket) = &mut self.notification_socket {
                    socket.send(Message::Pong(data)).await?;
                }
            }
            Message::Close(_) => anyhow::bail!("Notification stream closed"),
            _ => {}
        }
        Ok(())
    }
    async fn mark_current_read(&mut self) -> Result<()> {
        if let (Some(cache), Some(w), Some(ch)) =
            (&self.cache, &self.view.workspace, &self.view.channel)
        {
            if self.active && self.reading && cache.needs_read(w, ch)? {
                cache.mark_read(w, ch)?;
            }
            if let Some(socket) = &mut self.socket {
                for (channel_id, last_read_seq) in cache.pending_reads(w)? {
                    socket
                        .send(Message::Text(
                            serde_json::to_string(&ClientEvent::MarkRead {
                                channel_id,
                                last_read_seq,
                            })?
                            .into(),
                        ))
                        .await?;
                }
            }
        }
        Ok(())
    }
    async fn heartbeat(&mut self) -> Result<()> {
        self.mark_current_read().await?;

        if self.last_directory_refresh.elapsed() >= Duration::from_secs(30) {
            if let (Some(session), Some(w)) = (&self.session, &self.view.workspace) {
                let people: Vec<shared_proto::MemberInfo> =
                    sync::json(session.get(&self.http, &format!("/workspaces/{w}/people"))).await?;
                let users: Vec<_> = people.iter().map(|m| m.user.clone()).collect();
                self.cache
                    .as_ref()
                    .unwrap()
                    .set_metadata(&format!("people:{w}"), &people)?;
                self.view.people = people;
                self.cache
                    .as_ref()
                    .unwrap()
                    .set_metadata(&format!("members:{w}"), &users)?;
                self.view.users = users;
                if let Some(me) = self
                    .view
                    .users
                    .iter()
                    .find(|u| Some(&u.id) == self.view.user_id.as_ref())
                    .cloned()
                {
                    self.cache.as_ref().unwrap().set_metadata("me", &me)?;
                    self.view.me = Some(me);
                }
                self.refresh_avatars().await;
                self.publish();
            }
            self.last_directory_refresh = Instant::now();
        }
        anyhow::ensure!(
            self.last_frame.elapsed() < Duration::from_secs(45),
            "Connection heartbeat timed out"
        );
        if self.last_ping.elapsed() >= Duration::from_secs(15) {
            if let Some(socket) = &mut self.socket {
                socket
                    .send(Message::Text(
                        serde_json::to_string(&ClientEvent::Ping { ts: timestamp() })?.into(),
                    ))
                    .await?;
            }
            self.last_ping = Instant::now();
        }
        Ok(())
    }
    async fn refresh_catalog(&mut self) -> Result<()> {
        let session = self.session.as_ref().context("Sign in first")?;
        let me: shared_proto::User = sync::json(session.get(&self.http, "/users/me")).await?;
        self.cache.as_ref().unwrap().set_metadata("me", &me)?;
        self.view.me = Some(me);
        let workspaces: Vec<Workspace> = sync::json(session.get(&self.http, "/workspaces")).await?;
        self.cache
            .as_ref()
            .unwrap()
            .set_metadata("workspaces", &workspaces)?;
        self.view.workspaces = workspaces;
        if !self
            .view
            .workspaces
            .iter()
            .any(|w| Some(&w.id) == self.view.workspace.as_ref())
        {
            self.view.workspace = self.view.workspaces.first().map(|w| w.id.clone());
            self.view.channel = None;
            self.view.messages.clear();
        }
        if let Some(w) = &self.view.workspace {
            let people: Vec<shared_proto::MemberInfo> =
                sync::json(session.get(&self.http, &format!("/workspaces/{w}/people"))).await?;
            let users: Vec<_> = people.iter().map(|m| m.user.clone()).collect();
            self.cache
                .as_ref()
                .unwrap()
                .set_metadata(&format!("people:{w}"), &people)?;
            self.view.people = people;
            self.cache
                .as_ref()
                .unwrap()
                .set_metadata(&format!("members:{w}"), &users)?;
            self.view.users = users;
            let channels: Vec<Channel> =
                sync::json(session.get(&self.http, &format!("/workspaces/{w}/channels"))).await?;
            let previous = self.cache.as_ref().unwrap().channels(w)?;
            if channels
                .iter()
                .any(|ch| !previous.iter().any(|old| old.id == ch.id))
            {
                self.cache
                    .as_ref()
                    .unwrap()
                    .set_metadata(&format!("cursor:{w}"), &0i64)?;
            }
            self.cache.as_mut().unwrap().set_channels(w, &channels)?;
            if !channels
                .iter()
                .any(|c| Some(&c.id) == self.view.channel.as_ref())
            {
                self.view.channel = channels.first().map(|c| c.id.clone());
                self.view.messages.clear();
                self.view.thread = None;
                self.view.thread_messages.clear();
                self.view.attachments.clear();
                self.view.message_attachments.clear();
            }
            self.view.channels = channels;
        }
        self.refresh_avatars().await;
        self.refresh_channel_members().await?;
        Ok(())
    }
    async fn refresh_channel_members(&mut self) -> Result<()> {
        if let (Some(session), Some(w), Some(ch)) =
            (&self.session, &self.view.workspace, &self.view.channel)
        {
            let members: Vec<String> = sync::json(session.get(
                &self.http,
                &format!("/workspaces/{w}/channels/{ch}/members"),
            ))
            .await?;
            self.cache
                .as_ref()
                .unwrap()
                .set_metadata(&format!("channel-members:{w}:{ch}"), &members)?;
            self.view.channel_members = members;
        }
        Ok(())
    }
    async fn connect(&mut self) -> Result<()> {
        self.view.busy = true;
        self.publish();
        let result = self.connect_inner().await;
        self.view.busy = false;
        result
    }
    async fn connect_inner(&mut self) -> Result<()> {
        self.refresh_catalog().await?;
        let Some(w) = self.view.workspace.clone() else {
            self.retry_at = Instant::now() + Duration::from_secs(10);
            self.view.notice = Some("Create a workspace to get started".into());
            return Ok(());
        };
        let session = self.session.as_ref().unwrap();
        let mut url = url::Url::parse(&session.url(&format!("/workspaces/{w}/ws")))?;
        url.set_scheme(if url.scheme() == "https" { "wss" } else { "ws" })
            .map_err(|_| anyhow::anyhow!("invalid WebSocket scheme"))?;
        let mut request = url.as_str().into_client_request()?;
        request.headers_mut().insert(
            "authorization",
            format!("Bearer {}", session.token).parse()?,
        );
        let (socket, _) = tokio::time::timeout(
            Duration::from_secs(15),
            tokio_tungstenite::connect_async(request),
        )
        .await??;
        self.socket = Some(socket);
        self.last_frame = Instant::now();
        self.last_ping = Instant::now();
        self.backfill().await?;
        self.view.connected = true;
        self.view.notice = None;
        self.backoff = 1;
        self.flush().await?;
        Ok(())
    }
    async fn backfill(&mut self) -> Result<()> {
        if let (Some(session), Some(w), Some(cache)) =
            (&self.session, &self.view.workspace, &mut self.cache)
        {
            sync::backfill(&self.http, session, w, cache).await?;
        }
        self.refresh_attachments().await?;
        self.refresh_moderation().await?;
        Ok(())
    }
    async fn refresh_moderation(&mut self) -> Result<()> {
        if let (Some(session), Some(w)) = (&self.session, &self.view.workspace) {
            let actions: Vec<shared_proto::ModerationAction> = if self.view.can_moderate() {
                sync::json(session.get(&self.http, &format!("/workspaces/{w}/moderation"))).await?
            } else {
                vec![]
            };
            self.cache
                .as_ref()
                .unwrap()
                .set_metadata(&format!("moderation:{w}"), &actions)?;
            self.view.moderation = actions;
        }
        Ok(())
    }
    async fn refresh_attachments(&mut self) -> Result<()> {
        if let (Some(session), Some(w), Some(ch), Some(cache)) = (
            &self.session,
            &self.view.workspace,
            &self.view.channel,
            &self.cache,
        ) {
            let messages = cache.messages(w, ch, self.limit, &session.user_id)?;
            let mut since = messages
                .iter()
                .filter_map(|m| (m.message.seq > 0).then_some(m.message.seq))
                .min()
                .unwrap_or(0);
            if let Some(parent) = &self.view.thread
                && let Some(seq) = cache
                    .thread(w, parent, &session.user_id)?
                    .iter()
                    .filter_map(|m| (m.message.seq > 0).then_some(m.message.seq))
                    .min()
            {
                since = since.min(seq);
            }
            let attachments: Vec<Attachment> = sync::json(
                session
                    .get(&self.http, &format!("/workspaces/{w}/attachments"))
                    .query(&[("channel_id", ch.clone()), ("since_seq", since.to_string())]),
            )
            .await?;
            cache.set_metadata(&format!("attachments:{w}:{ch}"), &attachments)?;
            for a in &attachments {
                if let Some(path) = self.preview_path(a) {
                    let url = format!("/api/v1/workspaces/{w}/attachments/{}/preview", a.id);
                    let _ = self.cache_media(&url, &path).await;
                }
            }
        }
        Ok(())
    }
    async fn flush(&mut self) -> Result<()> {
        if self.in_flight.is_some() {
            return Ok(());
        }
        if let (Some(socket), Some(cache), Some(w)) =
            (&mut self.socket, &self.cache, &self.view.workspace)
            && let Some((id, event)) = cache.next_pending(w)?
        {
            socket
                .send(Message::Text(serde_json::to_string(&event)?.into()))
                .await?;
            self.in_flight = Some(id);
        }
        Ok(())
    }
    async fn on_frame(
        &mut self,
        frame: Option<std::result::Result<Message, tokio_tungstenite::tungstenite::Error>>,
    ) -> Result<()> {
        let message = frame.context("Connection closed")??;
        self.last_frame = Instant::now();
        match message {
            Message::Text(text) => {
                let event: ServerEvent = serde_json::from_str(&text)?;
                match event {
                    ServerEvent::ChannelsChanged => {
                        self.refresh_catalog().await?;
                        self.backfill().await?;
                        self.in_flight = None;
                        self.flush().await?;
                    }
                    ServerEvent::Hello { .. }
                    | ServerEvent::Pong { .. }
                    | ServerEvent::UserTyping { .. }
                    | ServerEvent::PresenceChanged { .. } => {}
                    ServerEvent::Error {
                        code,
                        message,
                        client_msg_id,
                    } => {
                        if code == "resync_required" {
                            anyhow::bail!("Server requested reconnect")
                        }
                        if let (Some(id), Some(cache), Some(w)) =
                            (client_msg_id, &self.cache, &self.view.workspace)
                        {
                            cache.fail(w, &id, &message)?;
                            if self.in_flight.as_ref() == Some(&id) {
                                self.in_flight = None;
                            }
                        }
                        self.view.error = Some(message);
                        self.flush().await?;
                    }
                    _ => {
                        self.backfill().await?;
                        if let ServerEvent::MessageCreated { message, .. } = &event
                            && let Some(w) = self.view.workspace.clone()
                        {
                            let author = self
                                .view
                                .users
                                .iter()
                                .find(|u| u.id == message.user_id)
                                .map(|u| u.display_name.clone())
                                .unwrap_or_else(|| "New message".into());
                            self.add_notification(&w, message, author)?;
                        }
                        self.mark_current_read().await?;
                        if let ServerEvent::MessageCreated {
                            client_msg_id: Some(id),
                            ..
                        } = &event
                        {
                            if let (Some(cache), Some(w)) = (&self.cache, &self.view.workspace) {
                                cache.acknowledge(w, id)?;
                            }
                            if self.in_flight.as_ref() == Some(id) {
                                self.in_flight = None;
                                self.flush().await?;
                            }
                        }
                    }
                }
            }
            Message::Close(_) => anyhow::bail!("Server closed connection"),
            Message::Ping(bytes) => {
                if let Some(socket) = &mut self.socket {
                    socket.send(Message::Pong(bytes)).await?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    async fn command(&mut self, command: Command) -> Result<()> {
        match command {
            Command::Authenticate {
                server,
                email,
                password,
                register,
            } => {
                self.view.busy = true;
                self.publish();
                let result = self.authenticate(server, email, password, register).await;
                self.view.busy = false;
                result?;
            }
            Command::SetActivity { active, reading } => {
                self.active = active;
                self.reading = reading;
            }
            Command::OpenMessage {
                workspace_id,
                channel_id,
                message_id,
            } => {
                self.refresh_catalog().await?;
                anyhow::ensure!(
                    self.view.workspaces.iter().any(|w| w.id == workspace_id),
                    "Workspace is not available for this account"
                );
                if self.view.workspace.as_ref() != Some(&workspace_id) {
                    self.socket = None;
                    self.view.connected = false;
                    self.view.workspace = Some(workspace_id.clone());
                    self.view.channel = None;
                    self.cache
                        .as_ref()
                        .unwrap()
                        .set_metadata("workspace", &workspace_id)?;
                    self.connect().await?;
                }
                anyhow::ensure!(
                    self.view.channels.iter().any(|c| c.id == channel_id),
                    "Conversation is no longer available"
                );
                self.view.channel = Some(channel_id.clone());
                self.backfill().await?;
                let m = self
                    .cache
                    .as_ref()
                    .unwrap()
                    .message(&workspace_id, &message_id)?
                    .context("Message is no longer available")?;
                anyhow::ensure!(
                    m.channel_id == channel_id && m.deleted_at.is_none(),
                    "Message is no longer available"
                );
                self.view.thread = Some(m.parent_id.unwrap_or(m.id));
                self.view.opened_message = Some(message_id);
                self.refresh_attachments().await?;
            }
            Command::MarkAllRead => {
                if let (Some(cache), Some(w)) = (&self.cache, &self.view.workspace) {
                    for channel in &self.view.channels {
                        cache.mark_read(w, &channel.id)?;
                    }
                }
            }
            Command::MarkChannelRead(ch) => {
                anyhow::ensure!(
                    self.view.channels.iter().any(|c| c.id == ch),
                    "Unknown channel"
                );
                if let (Some(cache), Some(w)) = (&self.cache, &self.view.workspace) {
                    cache.mark_read(w, &ch)?;
                }
            }
            Command::RegisterDevice {
                device_id,
                platform,
                token,
                environment,
            } => {
                let session = self.session.as_ref().context("Sign in first")?;
                let response: serde_json::Value = sync::json(
                    self.http
                        .put(session.url(&format!("/users/me/devices/{device_id}")))
                        .bearer_auth(&session.token)
                        .json(
                            &json!({"platform":platform,"token":token,"environment":environment}),
                        ),
                )
                .await?;
                self.view.notice = Some(
                    if response["delivery_configured"].as_bool() == Some(true) {
                        "Push notifications registered"
                    } else {
                        "Push device registered; server push configuration is required"
                    }
                    .into(),
                );
            }
            Command::UnregisterDevice(id) => {
                if let Some(session) = &self.session {
                    let _: serde_json::Value = sync::json(
                        self.http
                            .delete(session.url(&format!("/users/me/devices/{id}")))
                            .bearer_auth(&session.token),
                    )
                    .await?;
                }
            }
            Command::Logout => {
                self.notification_socket = None;
                self.socket = None;
                self.in_flight = None;
                self.session = None;
                self.cache = None;
                let path = self.root.join("session.json");
                if path.exists() {
                    std::fs::remove_file(path)?;
                }
                self.view = Snapshot::default();
            }
            Command::SelectWorkspace(w) => {
                anyhow::ensure!(
                    self.view.workspaces.iter().any(|x| x.id == w),
                    "Unknown workspace"
                );
                self.socket = None;
                self.in_flight = None;
                self.view.connected = false;
                self.view.workspace = Some(w.clone());
                self.view.channel = None;
                self.view.notifications.clear();
                self.view.thread = None;
                self.view.messages.clear();
                self.view.attachments.clear();
                self.view.search_query.clear();
                self.cache
                    .as_ref()
                    .context("Sign in first")?
                    .set_metadata("workspace", &w)?;
                self.view.channels = self.cache.as_ref().unwrap().channels(&w)?;
                self.view.channel = self.view.channels.first().map(|c| c.id.clone());
                self.retry_at = Instant::now();
                self.limit = 200;
            }
            Command::SelectChannel(ch) => {
                self.view.opened_message = None;
                anyhow::ensure!(
                    self.view.channels.iter().any(|c| c.id == ch),
                    "Unknown channel"
                );
                self.view.channel = Some(ch.clone());
                self.view.thread = None;
                self.view.attachments.clear();
                self.limit = 200;
                if self.view.connected {
                    self.refresh_channel_members().await?;
                    self.refresh_attachments().await?;
                }
            }
            Command::SelectThread(parent) => {
                self.view.thread = parent;
                if self.view.connected {
                    self.refresh_attachments().await?;
                }
            }
            Command::CreateWorkspace { name, slug } => {
                let session = self.session.as_ref().context("Sign in first")?;
                let w: Workspace = sync::json(
                    session
                        .post(&self.http, "/workspaces")
                        .json(&json!({"name":name,"slug":slug})),
                )
                .await?;
                self.view.workspace = Some(w.id);
                self.view.channel = None;
                self.view.thread = None;
                self.view.messages.clear();
                self.view.attachments.clear();
                self.socket = None;
                self.view.connected = false;
                self.in_flight = None;
                self.retry_at = Instant::now();
                self.refresh_catalog().await?;
            }
            command @ (Command::CreateChannel(_) | Command::CreatePrivateChannel(_)) => {
                let (name, is_private) = match command {
                    Command::CreateChannel(name) => (name, false),
                    Command::CreatePrivateChannel(name) => (name, true),
                    _ => unreachable!(),
                };
                let (session, w) = self.auth_workspace()?;
                let ch: Channel = sync::json(
                    session
                        .post(&self.http, &format!("/workspaces/{w}/channels"))
                        .json(&json!({"name":name,"is_private":is_private})),
                )
                .await?;
                self.refresh_catalog().await?;
                self.view.channel = Some(ch.id);
                self.view.thread = None;
                self.limit = 200;
                self.refresh_channel_members().await?;
            }
            Command::OpenDirect(user_id) => {
                let (session, w) = self.auth_workspace()?;
                let ch: Channel = sync::json(
                    session
                        .post(&self.http, &format!("/workspaces/{w}/directs"))
                        .json(&json!({"user_id":user_id})),
                )
                .await?;
                self.refresh_catalog().await?;
                self.view.channel = Some(ch.id);
                self.view.thread = None;
                self.view.attachments.clear();
                self.limit = 200;
                self.backfill().await?;
                self.refresh_channel_members().await?;
            }
            Command::ChannelMember { user_id, add } => {
                let (session, w) = self.auth_workspace()?;
                let ch = self.view.channel.as_ref().context("Select a channel")?;
                let request = if add {
                    session
                        .post(
                            &self.http,
                            &format!("/workspaces/{w}/channels/{ch}/members"),
                        )
                        .json(&json!({"user_id":user_id}))
                } else {
                    self.http
                        .delete(
                            session
                                .url(&format!("/workspaces/{w}/channels/{ch}/members/{user_id}")),
                        )
                        .bearer_auth(&session.token)
                };
                let _: serde_json::Value = sync::json(request).await?;
                self.refresh_channel_members().await?;
            }
            Command::SaveProfile {
                display_name,
                handle,
                bio,
            } => {
                let session = self.session.as_ref().context("Sign in first")?;
                let user: shared_proto::User = sync::json(
                    self.http
                        .patch(session.url("/users/me"))
                        .bearer_auth(&session.token)
                        .json(&json!({"display_name":display_name,"handle":handle,"bio":bio})),
                )
                .await?;
                self.cache.as_ref().unwrap().set_metadata("me", &user)?;
                for member in &mut self.view.users {
                    if member.id == user.id {
                        *member = user.clone();
                    }
                }
                if let Some(w) = &self.view.workspace {
                    self.cache
                        .as_ref()
                        .unwrap()
                        .set_metadata(&format!("members:{w}"), &self.view.users)?;
                }
                for person in &mut self.view.people {
                    if person.user.id == user.id {
                        person.user = user.clone();
                    }
                }
                if let Some(w) = &self.view.workspace {
                    self.cache
                        .as_ref()
                        .unwrap()
                        .set_metadata(&format!("people:{w}"), &self.view.people)?;
                }
                self.view.me = Some(user);
                self.view.notice = Some("Profile saved".into());
            }
            Command::EditWorkspace { name, slug } => {
                let (session, w) = self.auth_workspace()?;
                let _: Workspace = sync::json(
                    self.http
                        .patch(session.url(&format!("/workspaces/{w}")))
                        .bearer_auth(&session.token)
                        .json(&json!({"name":name,"slug":slug})),
                )
                .await?;
                self.refresh_catalog().await?;
                self.view.notice = Some("Workspace saved".into());
            }
            Command::EditChannel { id, name, topic } => {
                let (session, w) = self.auth_workspace()?;
                let _: serde_json::Value = sync::json(
                    self.http
                        .patch(session.url(&format!("/workspaces/{w}/channels/{id}")))
                        .bearer_auth(&session.token)
                        .json(&json!({"name":name,"topic":topic})),
                )
                .await?;
                self.refresh_catalog().await?;
                self.view.notice = Some("Channel saved".into());
            }
            Command::SetRole { user_id, role } => {
                let (session, w) = self.auth_workspace()?;
                let _: serde_json::Value = sync::json(
                    self.http
                        .patch(session.url(&format!("/workspaces/{w}/people/{user_id}/role")))
                        .bearer_auth(&session.token)
                        .json(&json!({"role":role})),
                )
                .await?;
                self.refresh_catalog().await?;
                self.view.notice = Some("Role updated".into());
            }
            Command::Invite(email) => {
                let (session, w) = self.auth_workspace()?;
                let _: serde_json::Value = sync::json(
                    session
                        .post(&self.http, &format!("/workspaces/{w}/members"))
                        .json(&json!({"email":email})),
                )
                .await?;
                self.view.notice = Some("Member added to workspace".into());
            }
            Command::Send {
                body,
                parent_id,
                topic_tag,
            } => {
                anyhow::ensure!(
                    (!body.trim().is_empty() || !self.view.attachments.is_empty())
                        && body.len() <= 64000,
                    "Add text or an attachment; text must be at most 64 KB"
                );
                let user = self
                    .session
                    .as_ref()
                    .context("Sign in first")?
                    .user_id
                    .clone();
                let w = self.view.workspace.clone().context("Select a workspace")?;
                let ch = self.view.channel.clone().context("Select a channel")?;
                let event = ClientEvent::SendMessage {
                    client_msg_id: uuid::Uuid::now_v7().to_string(),
                    channel_id: ch,
                    parent_id,
                    topic_tag,
                    body_md: body,
                    attachment_ids: self.view.attachments.iter().map(|a| a.id.clone()).collect(),
                };
                if let ClientEvent::SendMessage { client_msg_id, .. } = &event {
                    self.cache.as_ref().unwrap().set_metadata(
                        &format!("pending-attachments:{w}:{client_msg_id}"),
                        &self.view.attachments,
                    )?;
                }
                self.cache.as_mut().unwrap().enqueue(&w, &user, &event)?;
                self.view.attachments.clear();
                if let Err(e) = self.flush().await {
                    self.offline(e);
                }
            }
            Command::Event(event) => {
                anyhow::ensure!(
                    !matches!(event, ClientEvent::SendMessage { .. }),
                    "Use the durable send command"
                );
                let socket = self
                    .socket
                    .as_mut()
                    .context("Reconnect before changing messages or reactions")?;
                socket
                    .send(Message::Text(serde_json::to_string(&event)?.into()))
                    .await?;
            }
            Command::Retry(id) => {
                if let (Some(cache), Some(w)) = (&self.cache, &self.view.workspace) {
                    cache.retry(w, &id)?;
                }
                self.flush().await?;
            }
            Command::Download { id, path } => {
                let (session, w) = self.auth_workspace()?;
                let response = session
                    .get(&self.http, &format!("/workspaces/{w}/attachments/{id}"))
                    .send()
                    .await?
                    .error_for_status()?;
                let data = response.bytes().await?;
                tokio::fs::write(path, &data).await?;
                self.view.notice = Some("File saved".into());
            }
            Command::Search(text) => {
                self.view.search_query = text;
            }
            Command::LoadOlder => self.limit = self.limit.saturating_add(200),
            Command::UploadAvatar(path) => {
                let session = self.session.as_ref().context("Sign in first")?;
                let meta = tokio::fs::metadata(&path).await?;
                anyhow::ensure!(
                    meta.is_file() && meta.len() <= 5 * 1024 * 1024,
                    "Choose a profile image up to 5 MiB"
                );
                let filename = path
                    .file_name()
                    .context("Invalid filename")?
                    .to_string_lossy()
                    .to_string();
                let part = reqwest::multipart::Part::bytes(tokio::fs::read(path).await?)
                    .file_name(filename);
                let user: shared_proto::User = sync::json(
                    session
                        .post(&self.http, "/users/me/avatar")
                        .multipart(reqwest::multipart::Form::new().part("file", part)),
                )
                .await?;
                self.cache.as_ref().unwrap().set_metadata("me", &user)?;
                for member in &mut self.view.users {
                    if member.id == user.id {
                        *member = user.clone();
                    }
                }
                if let Some(w) = &self.view.workspace {
                    self.cache
                        .as_ref()
                        .unwrap()
                        .set_metadata(&format!("members:{w}"), &self.view.users)?;
                }
                self.view.me = Some(user);
                self.refresh_avatars().await;
                self.view.notice = Some("Profile picture updated".into());
            }
            Command::Upload(path) => {
                let (session, w) = self.auth_workspace()?;
                let meta = tokio::fs::metadata(&path).await?;
                anyhow::ensure!(
                    meta.is_file() && meta.len() < 10 * 1024 * 1024,
                    "Choose a file smaller than 10 MiB"
                );
                let filename = path
                    .file_name()
                    .context("Invalid file name")?
                    .to_string_lossy()
                    .to_string();
                let bytes = tokio::fs::read(path).await?;
                let part = reqwest::multipart::Part::bytes(bytes).file_name(filename);
                let a: Attachment = sync::json(
                    session
                        .post(&self.http, &format!("/workspaces/{w}/attachments"))
                        .multipart(reqwest::multipart::Form::new().part("file", part)),
                )
                .await?;
                if let Some(path) = self.preview_path(&a) {
                    let url = format!("/api/v1/workspaces/{w}/attachments/{}/preview", a.id);
                    let _ = self.cache_media(&url, &path).await;
                }
                self.view.attachments.push(a);
            }
            Command::RemoveAttachment(id) => self.view.attachments.retain(|a| a.id != id),
            Command::Refresh => {
                self.refresh_catalog().await?;
                self.backfill().await?;
            }
            Command::ClearError => {
                self.view.error = None;
                self.view.notice = None;
            }
        }
        Ok(())
    }
    fn preview_path(&self, a: &Attachment) -> Option<PathBuf> {
        if !matches!(
            a.mime_type.as_str(),
            "image/png" | "image/jpeg" | "image/gif" | "image/webp"
        ) || uuid::Uuid::parse_str(&a.id).is_err()
        {
            return None;
        }
        Some(self.media.join(format!("image-{}.png", a.id)))
    }
    fn avatar_location(&self, user: &shared_proto::User) -> Option<(String, PathBuf)> {
        let url = user.avatar_url.as_ref()?;
        let key = url.rsplit('/').next()?;
        uuid::Uuid::parse_str(key).ok()?;
        if *url != format!("/api/v1/users/{}/avatar/{key}", user.id) {
            return None;
        }
        Some((url.clone(), self.media.join(format!("avatar-{key}.png"))))
    }
    async fn refresh_avatars(&mut self) {
        let users: Vec<_> = self
            .view
            .users
            .iter()
            .chain(self.view.me.iter())
            .cloned()
            .collect();
        for user in users {
            if let Some((url, path)) = self.avatar_location(&user) {
                // Optional previews never block delivery; originals remain available through Download.
                let _ = self.cache_media(&url, &path).await;
            }
        }
    }
    async fn cache_media(&self, url: &str, path: &std::path::Path) -> Result<()> {
        if path.is_file() {
            return Ok(());
        }
        let session = self.session.as_ref().context("Sign in first")?;
        let mut response = self
            .http
            .get(format!("{}{url}", session.server))
            .bearer_auth(&session.token)
            .send()
            .await?
            .error_for_status()?;
        let mut data = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            anyhow::ensure!(
                data.len() + chunk.len() <= 4 * 1024 * 1024,
                "Image preview exceeds 4 MiB"
            );
            data.extend_from_slice(&chunk);
        }
        let temporary = path.with_extension("tmp");
        tokio::fs::write(&temporary, data).await?;
        tokio::fs::rename(temporary, path).await?;
        Ok(())
    }
    fn auth_workspace(&self) -> Result<(&Session, &str)> {
        Ok((
            self.session.as_ref().context("Sign in first")?,
            self.view
                .workspace
                .as_deref()
                .context("Select a workspace")?,
        ))
    }
    async fn authenticate(
        &mut self,
        server: String,
        email: String,
        password: String,
        register: bool,
    ) -> Result<()> {
        #[derive(Deserialize, Serialize)]
        struct AuthResponse {
            user_id: String,
            token: String,
        }
        let server = sync::normalize_server(&server)?;
        let response: AuthResponse = sync::json(
            self.http
                .post(format!(
                    "{server}/api/v1/auth/{}",
                    if register { "register" } else { "login" }
                ))
                .json(&json!({"email":email,"password":password})),
        )
        .await?;
        self.socket = None;
        self.open_session(Session {
            server,
            user_id: response.user_id,
            token: response.token,
        })?;
        self.save_session()?;
        if let Err(e) = self.connect().await {
            self.offline(e);
        }
        Ok(())
    }
}
