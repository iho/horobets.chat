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
#[derive(Clone, Debug, PartialEq)]
pub enum OutboxStatus {
    Sent,
    Pending,
    Failed(String),
}
#[derive(Clone, Debug)]
pub struct ReactionCount {
    pub emoji: String,
    pub count: i64,
    pub mine: bool,
}
#[derive(Clone, Debug)]
pub struct CachedMessage {
    pub message: MessageDto,
    pub status: OutboxStatus,
    pub reactions: Vec<ReactionCount>,
}
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub server: String,
    pub user_id: Option<String>,
    pub workspaces: Vec<Workspace>,
    pub workspace: Option<String>,
    pub channels: Vec<Channel>,
    pub users: Vec<shared_proto::User>,
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
pub enum Command {
    Authenticate {
        server: String,
        email: String,
        password: String,
        register: bool,
    },
    Logout,
    SelectWorkspace(String),
    SelectChannel(String),
    SelectThread(Option<String>),
    CreateWorkspace {
        name: String,
        slug: String,
    },
    CreateChannel(String),
    Invite(String),
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
            if let Some(ch) = &self.view.channel {
                cache.mark_read(w, ch)?;
            }
            self.view.unread = cache.unread(w, &session.user_id)?;
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
                command=commands.recv()=>{let Ok(command)=command else{break};self.view.error=None;let result=self.command(command).await;if let Err(e)=result{self.view.error=Some(e.to_string());}self.publish();},
                frame=async{match &mut self.socket{Some(s)=>s.next().await,None=>std::future::pending().await}}=>{let result=self.on_frame(frame).await;if let Err(e)=result{self.offline(e);}self.publish();},
                _=tick.tick()=>{if self.socket.is_some(){if let Err(e)=self.heartbeat().await{self.offline(e);self.publish();}}else if self.session.is_some()&&self.socket.is_none()&&Instant::now()>=self.retry_at{if let Err(e)=self.connect().await{self.offline(e);}self.publish();}}
            }
        }
        if let Some(mut socket) = self.socket.take() {
            let _ = socket.close(None).await;
        }
    }
    async fn heartbeat(&mut self) -> Result<()> {
        if self.last_directory_refresh.elapsed() >= Duration::from_secs(30) {
            if let (Some(session), Some(w)) = (&self.session, &self.view.workspace) {
                let users: Vec<shared_proto::User> =
                    sync::json(session.get(&self.http, &format!("/workspaces/{w}/members")))
                        .await?;
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
            let users: Vec<shared_proto::User> =
                sync::json(session.get(&self.http, &format!("/workspaces/{w}/members"))).await?;
            self.cache
                .as_ref()
                .unwrap()
                .set_metadata(&format!("members:{w}"), &users)?;
            self.view.users = users;
            let channels: Vec<Channel> =
                sync::json(session.get(&self.http, &format!("/workspaces/{w}/channels"))).await?;
            self.cache.as_mut().unwrap().set_channels(w, &channels)?;
            if !channels
                .iter()
                .any(|c| Some(&c.id) == self.view.channel.as_ref())
            {
                self.view.channel = channels.first().map(|c| c.id.clone());
            }
            self.view.channels = channels;
        }
        self.refresh_avatars().await;
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
            Command::Logout => {
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
                anyhow::ensure!(
                    self.view.channels.iter().any(|c| c.id == ch),
                    "Unknown channel"
                );
                self.view.channel = Some(ch.clone());
                self.view.thread = None;
                self.view.attachments.clear();
                self.limit = 200;
                if let (Some(cache), Some(w)) = (&self.cache, &self.view.workspace) {
                    cache.mark_read(w, &ch)?;
                }
                if self.view.connected {
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
            Command::CreateChannel(name) => {
                let (session, w) = self.auth_workspace()?;
                let ch: Channel = sync::json(
                    session
                        .post(&self.http, &format!("/workspaces/{w}/channels"))
                        .json(&json!({"name":name})),
                )
                .await?;
                self.refresh_catalog().await?;
                self.view.channel = Some(ch.id);
                self.view.thread = None;
                self.limit = 200;
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
