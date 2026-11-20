use crate::ui;
use client_core::{ClientEngine, Command, Snapshot};
use gpui::prelude::*;
use gpui::*;
use gpui_component::input::{InputEvent, InputState};
use shared_proto::ClientEvent;
actions!(messenger, [QuickSearch, CloseOverlay, Quit]);
#[derive(Clone, PartialEq)]
pub enum Modal {
    None,
    Search,
    Workspace,
    Channel,
    Invite,
    Edit(String),
    Direct,
    Members,
    Settings,
    Reaction(String),
    Manage,
    Pins,
    Profile(String),
    ChannelEdit(String),
    ConfirmDelete(String),
}
#[derive(serde::Serialize, serde::Deserialize)]
pub struct PaneSizes {
    #[serde(default = "default_dark_mode")]
    pub dark_mode: bool,
    pub sidebar: f32,
    pub thread: f32,
}
fn default_dark_mode() -> bool {
    true
}
impl Default for PaneSizes {
    fn default() -> Self {
        Self {
            dark_mode: true,
            sidebar: 240.,
            thread: 360.,
        }
    }
}
pub struct Messenger {
    pub pane_sizes: PaneSizes,
    layout_path: std::path::PathBuf,
    pub engine: ClientEngine,
    pub snapshot: Snapshot,
    pub focus: FocusHandle,
    pub server: Entity<InputState>,
    pub email: Entity<InputState>,
    pub password: Entity<InputState>,
    pub composer: Entity<InputState>,
    pub reply: Entity<InputState>,
    pub markdown_preview: [bool; 2],
    pub completion_selection: [usize; 2],
    pub completion_scroll: [UniformListScrollHandle; 2],
    pub completion_dismissed: [bool; 2],
    pub topic: Entity<InputState>,
    pub search: Entity<InputState>,
    pub name: Entity<InputState>,
    pub slug: Entity<InputState>,
    pub profile_name: Entity<InputState>,
    pub profile_handle: Entity<InputState>,
    pub profile_bio: Entity<InputState>,
    pub manage_name: Entity<InputState>,
    pub manage_slug: Entity<InputState>,
    pub edit_topic: Entity<InputState>,
    pub private_channel: bool,
    pub reaction_selection: usize,
    pub reaction_focus: FocusHandle,
    _modal_subscription: Option<Subscription>,
    pub register: bool,
    pub modal: Modal,
    pub feed: ListState,
    pub thread_list: ListState,
    pub visible: Vec<usize>,
    pub local_error: Option<String>,
    pub drafts: std::collections::HashMap<String, String>,
    pub search_selection: usize,
    pub search_scroll: UniformListScrollHandle,
    _subscriptions: Vec<Subscription>,
    _updates: Task<()>,
}
impl Messenger {
    pub fn new(
        engine: ClientEngine,
        layout_path: std::path::PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut pane_sizes: PaneSizes = std::fs::read(&layout_path)
            .ok()
            .and_then(|s| serde_json::from_slice(&s).ok())
            .unwrap_or_default();
        pane_sizes.sidebar = if pane_sizes.sidebar.is_finite() {
            pane_sizes.sidebar.clamp(180., 420.)
        } else {
            240.
        };
        pane_sizes.thread = if pane_sizes.thread.is_finite() {
            pane_sizes.thread.clamp(260., 600.)
        } else {
            360.
        };
        gpui_component::Theme::change(
            if pane_sizes.dark_mode {
                gpui_component::ThemeMode::Dark
            } else {
                gpui_component::ThemeMode::Light
            },
            Some(window),
            cx,
        );
        ui::configure_controls(cx);
        let profile_name = cx.new(|cx| InputState::new(window, cx).placeholder("Display name"));
        let profile_handle = cx.new(|cx| InputState::new(window, cx).placeholder("ihor"));
        let profile_bio = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("About you")
                .auto_grow(3, 8)
        });
        let manage_name = cx.new(|cx| InputState::new(window, cx).placeholder("Workspace name"));
        let manage_slug = cx.new(|cx| InputState::new(window, cx).placeholder("workspace-slug"));
        let edit_topic = cx.new(|cx| InputState::new(window, cx).placeholder("Channel topic"));
        let server = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Server URL")
                .default_value("http://127.0.0.1:3000")
        });
        let email = cx.new(|cx| InputState::new(window, cx).placeholder("you@company.com"));
        let password = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Password")
                .masked(true)
        });
        let composer = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Write a message…")
                .auto_grow(2, 8)
        });
        let reply = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Reply in thread…")
                .auto_grow(2, 6)
        });
        let topic = cx.new(|cx| InputState::new(window, cx).placeholder("Optional topic"));
        let search =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search messages or #channels…"));
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("Name"));
        let slug = cx.new(|cx| InputState::new(window, cx).placeholder("workspace-slug"));
        let subscriptions = vec![
            cx.observe(&composer, |_, _, cx| cx.notify()),
            cx.observe(&reply, |_, _, cx| cx.notify()),
            cx.subscribe_in(&composer, window, |this, _, event, _, cx| {
                if matches!(event, InputEvent::Change) {
                    this.completion_selection[0] = 0;
                    this.completion_scroll[0].scroll_to_item(0, ScrollStrategy::Center);
                    this.completion_dismissed[0] = false;
                    cx.notify();
                }
            }),
            cx.subscribe_in(&reply, window, |this, _, event, _, cx| {
                if matches!(event, InputEvent::Change) {
                    this.completion_selection[1] = 0;
                    this.completion_scroll[1].scroll_to_item(0, ScrollStrategy::Center);
                    this.completion_dismissed[1] = false;
                    cx.notify();
                }
            }),
            cx.subscribe_in(&search, window, |this, input, event, _, cx| {
                if matches!(event, InputEvent::Change) {
                    this.search_selection = 0;
                    this.command(Command::Search(input.read(cx).value().to_string()), cx);
                }
            }),
            cx.subscribe_in(&password, window, |this, _, event, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.authenticate(window, cx);
                }
            }),
        ];
        let updates = engine.updates.clone();
        let task = cx.spawn(async move |entity, cx| {
            while let Ok(snapshot) = updates.recv().await {
                if entity
                    .update(cx, |this, cx| this.apply(snapshot, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        Self {
            pane_sizes,
            layout_path,
            engine,
            snapshot: Snapshot::default(),
            focus: cx.focus_handle(),
            server,
            email,
            password,
            composer,
            reply,
            markdown_preview: [false; 2],
            completion_selection: [0; 2],
            completion_scroll: [
                UniformListScrollHandle::new(),
                UniformListScrollHandle::new(),
            ],
            completion_dismissed: [false; 2],
            topic,
            search,
            name,
            slug,
            profile_name,
            profile_handle,
            profile_bio,
            manage_name,
            manage_slug,
            edit_topic,
            private_channel: false,
            reaction_selection: 0,
            reaction_focus: cx.focus_handle(),
            _modal_subscription: None,
            search_selection: 0,
            search_scroll: UniformListScrollHandle::new(),
            register: false,
            modal: Modal::None,
            feed: ListState::new(0, ListAlignment::Bottom, px(500.)),
            thread_list: ListState::new(0, ListAlignment::Bottom, px(300.)),
            visible: vec![],
            local_error: None,
            drafts: Default::default(),
            _subscriptions: subscriptions,
            _updates: task,
        }
    }
    fn apply(&mut self, snapshot: Snapshot, cx: &mut Context<Self>) {
        let previous = self.visible.len();
        let channel_changed = self.snapshot.channel != snapshot.channel;
        let stick = self.feed.logical_scroll_top().item_ix.saturating_add(8) >= previous;
        self.visible = snapshot
            .messages
            .iter()
            .enumerate()
            .filter_map(|(i, m)| m.message.parent_id.is_none().then_some(i))
            .collect();
        self.feed.reset(self.visible.len());
        if (channel_changed || stick) && !self.visible.is_empty() {
            self.feed.scroll_to_reveal_item(self.visible.len() - 1);
        }
        let thread_changed = self.snapshot.thread != snapshot.thread;
        self.thread_list.reset(snapshot.thread_messages.len());
        if thread_changed && !snapshot.thread_messages.is_empty() {
            self.thread_list
                .scroll_to_reveal_item(snapshot.thread_messages.len() - 1);
        }
        self.snapshot = snapshot;
        cx.notify();
    }
    pub fn command(&mut self, command: Command, cx: &mut Context<Self>) -> bool {
        match self.engine.send(command) {
            Ok(()) => {
                self.local_error = None;
                cx.notify();
                true
            }
            Err(e) => {
                self.local_error = Some(e.to_string());
                cx.notify();
                false
            }
        }
    }
    pub fn authenticate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.snapshot.busy {
            return;
        }
        let command = Command::Authenticate {
            server: self.server.read(cx).value().to_string(),
            email: self.email.read(cx).value().to_string(),
            password: self.password.read(cx).value().to_string(),
            register: self.register,
        };
        if self.command(command, cx) {
            self.snapshot.busy = true;
            self.password
                .update(cx, |input, cx| input.set_value("", window, cx));
        }
    }
    pub fn select_channel(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(current) = &self.snapshot.channel {
            self.drafts
                .insert(current.clone(), self.composer.read(cx).value().to_string());
        }
        let draft = self.drafts.get(&id).cloned().unwrap_or_default();
        self.composer
            .update(cx, |input, cx| input.set_value(draft, window, cx));
        self.topic
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.command(Command::SelectChannel(id), cx);
        self.modal = Modal::None;
        self.composer
            .update(cx, |input, cx| input.focus(window, cx));
    }
    pub fn open_modal(&mut self, modal: Modal, window: &mut Window, cx: &mut Context<Self>) {
        self.modal = modal.clone();
        if matches!(modal, Modal::Manage | Modal::Pins | Modal::Profile(_)) {
            if modal == Modal::Manage
                && let Some(w) = self
                    .snapshot
                    .workspaces
                    .iter()
                    .find(|w| Some(&w.id) == self.snapshot.workspace.as_ref())
                    .cloned()
            {
                self.manage_name
                    .update(cx, |i, cx| i.set_value(w.name, window, cx));
                self.manage_slug
                    .update(cx, |i, cx| i.set_value(w.slug, window, cx));
            }
            self.focus.focus(window);
            cx.notify();
            return;
        }
        if matches!(modal, Modal::ConfirmDelete(_)) {
            self.focus.focus(window);
            cx.notify();
            return;
        }
        if matches!(modal, Modal::Reaction(_)) {
            self.reaction_selection = 0;
            self.reaction_focus.focus(window);
            cx.notify();
            return;
        }
        if modal == Modal::Settings {
            let name = self
                .snapshot
                .me
                .as_ref()
                .map(|u| u.display_name.clone())
                .unwrap_or_default();
            let handle = self
                .snapshot
                .me
                .as_ref()
                .and_then(|u| u.handle.clone())
                .unwrap_or_default();
            self.profile_name
                .update(cx, |i, cx| i.set_value(name, window, cx));
            self.profile_handle
                .update(cx, |i, cx| i.set_value(handle, window, cx));
            let bio = self
                .snapshot
                .me
                .as_ref()
                .map(|u| u.bio.clone())
                .unwrap_or_default();
            self.profile_bio
                .update(cx, |i, cx| i.set_value(bio, window, cx));
            self.profile_name.update(cx, |i, cx| i.focus(window, cx));
            cx.notify();
            return;
        }
        if modal == Modal::Channel {
            self.private_channel = false;
        }
        self.search_selection = 0;
        if modal != Modal::Search {
            self.name = cx.new(|cx| {
                if matches!(modal, Modal::Edit(_)) {
                    InputState::new(window, cx)
                        .placeholder("Message")
                        .auto_grow(3, 10)
                } else {
                    InputState::new(window, cx).placeholder(if modal == Modal::Invite {
                        "teammate@example.com"
                    } else if modal == Modal::Direct {
                        "Search teammates or @handle"
                    } else if modal == Modal::Members {
                        "Filter members by name or @handle"
                    } else {
                        "Name"
                    })
                }
            });
        }
        self._modal_subscription = Some(cx.subscribe_in(
            &self.name,
            window,
            |_, _, _: &InputEvent, _, cx| cx.notify(),
        ));
        if modal == Modal::Workspace {
            self.slug.update(cx, |i, cx| i.set_value("", window, cx));
        }
        let input = if modal == Modal::Search {
            self.search.clone()
        } else {
            self.name.clone()
        };
        input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        if let Modal::ChannelEdit(id) = modal
            && let Some(ch) = self
                .snapshot
                .channels
                .iter()
                .find(|ch| ch.id == id)
                .cloned()
        {
            self.name
                .update(cx, |i, cx| i.set_value(ch.name, window, cx));
            self.edit_topic.update(cx, |i, cx| {
                i.set_value(ch.topic.unwrap_or_default(), window, cx)
            });
        }
        cx.notify();
    }
    pub fn submit_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.name.read(cx).value().to_string();
        if name.trim().is_empty() {
            return;
        }
        let command = match &self.modal {
            Modal::Workspace => Command::CreateWorkspace {
                name,
                slug: self.slug.read(cx).value().to_string(),
            },
            Modal::Channel => {
                if self.private_channel {
                    Command::CreatePrivateChannel(name)
                } else {
                    Command::CreateChannel(name)
                }
            }
            Modal::Invite => Command::Invite(name),
            Modal::ChannelEdit(id) => Command::EditChannel {
                id: id.clone(),
                name,
                topic: self.edit_topic.read(cx).value().to_string(),
            },
            Modal::Edit(id) => Command::Event(ClientEvent::EditMessage {
                message_id: id.clone(),
                body_md: name,
            }),
            _ => return,
        };
        if self.command(command, cx) {
            self.modal = Modal::None;
            self.focus.focus(window);
            cx.notify();
        }
    }
    pub fn submit_message(&mut self, thread: bool, window: &mut Window, cx: &mut Context<Self>) {
        let input = if thread {
            self.reply.clone()
        } else {
            self.composer.clone()
        };
        let body = input.read(cx).value().to_string();
        if body.trim().is_empty() && self.snapshot.attachments.is_empty() {
            return;
        }
        if body.len() > 64000 {
            self.local_error = Some("Message exceeds 64000 bytes".into());
            cx.notify();
            return;
        }
        let parent_id = if thread {
            self.snapshot.thread.clone()
        } else {
            None
        };
        let topic = self.topic.read(cx).value().to_string();
        if body.starts_with("/join ") && !thread {
            let channel = body.trim_start_matches("/join ").trim();
            if let Some(ch) = self
                .snapshot
                .channels
                .iter()
                .find(|c| c.name == channel)
                .cloned()
            {
                self.select_channel(ch.id, window, cx);
                return;
            } else {
                self.local_error = Some("Channel not found".into());
                cx.notify();
                return;
            }
        }
        if self.command(
            Command::Send {
                body,
                parent_id,
                topic_tag: (!topic.trim().is_empty()).then_some(topic),
            },
            cx,
        ) {
            self.markdown_preview[usize::from(thread)] = false;
            input.update(cx, |input, cx| input.set_value("", window, cx));
            if !thread && let Some(ch) = &self.snapshot.channel {
                self.drafts.remove(ch);
            }
            if !self.visible.is_empty() {
                self.feed.scroll_to_reveal_item(self.visible.len() - 1);
            }
        }
    }
    pub fn set_dark_mode(&mut self, dark: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.pane_sizes.dark_mode = dark;
        gpui_component::Theme::change(
            if dark {
                gpui_component::ThemeMode::Dark
            } else {
                gpui_component::ThemeMode::Light
            },
            Some(window),
            cx,
        );
        ui::configure_controls(cx);
        self.save_pane_sizes(cx);
        cx.notify();
    }
    pub fn open_direct(&mut self, user: String, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(channel) = self
            .snapshot
            .channels
            .iter()
            .find(|ch| {
                (ch.is_direct && ch.peer_user_id.as_ref() == Some(&user))
                    || (ch.is_saved && self.snapshot.user_id.as_ref() == Some(&user))
            })
            .cloned()
        {
            self.select_channel(channel.id, window, cx);
            return;
        }
        if let Some(ch) = &self.snapshot.channel {
            self.drafts
                .insert(ch.clone(), self.composer.read(cx).value().to_string());
        }
        self.composer
            .update(cx, |i, cx| i.set_value("", window, cx));
        self.topic.update(cx, |i, cx| i.set_value("", window, cx));
        self.command(Command::OpenDirect(user), cx);
        self.modal = Modal::None;
        cx.notify();
    }
    pub fn toggle_reaction(
        &mut self,
        id: String,
        emoji: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.command(
            Command::Event(ClientEvent::ToggleReaction {
                message_id: id,
                emoji,
            }),
            cx,
        ) {
            self.modal = Modal::None;
            self.focus.focus(window);
            cx.notify();
        }
    }
    pub fn save_pane_sizes(&mut self, cx: &mut Context<Self>) {
        let result = (|| -> anyhow::Result<()> {
            let bytes = serde_json::to_vec(&self.pane_sizes)?;
            let tmp = self.layout_path.with_extension("tmp");
            std::fs::write(&tmp, bytes)?;
            std::fs::rename(tmp, &self.layout_path)?;
            Ok(())
        })();
        if let Err(e) = result {
            self.local_error = Some(format!("Could not save pane widths: {e}"));
            cx.notify();
        }
    }
    pub fn choose_file(&mut self, cx: &mut Context<Self>) {
        self.choose_upload(false, cx);
    }
    pub fn choose_avatar(&mut self, cx: &mut Context<Self>) {
        self.choose_upload(true, cx);
    }
    fn choose_upload(&mut self, avatar: bool, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(
                if avatar {
                    "Choose profile picture (PNG, JPEG, WebP, GIF)"
                } else {
                    "Attach file"
                }
                .into(),
            ),
        });
        cx.spawn(async move |entity, cx| match paths.await {
            Ok(Ok(Some(paths))) => {
                let _ = entity.update(cx, |this, cx| {
                    for path in paths {
                        this.command(
                            if avatar {
                                Command::UploadAvatar(path)
                            } else {
                                Command::Upload(path)
                            },
                            cx,
                        );
                    }
                });
            }
            Ok(Err(e)) => {
                let _ = entity.update(cx, |this, cx| {
                    this.local_error = Some(e.to_string());
                    cx.notify();
                });
            }
            _ => {}
        })
        .detach();
    }
    pub fn save_attachment(&mut self, id: String, filename: String, cx: &mut Context<Self>) {
        let directory = directories::UserDirs::new()
            .and_then(|u| u.download_dir().map(|p| p.to_owned()))
            .unwrap_or_else(std::env::temp_dir);
        let name = std::path::Path::new(&filename)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let result = cx.prompt_for_new_path(&directory, Some(&name));
        cx.spawn(async move |entity, cx| {
            if let Ok(Ok(Some(path))) = result.await {
                let _ = entity.update(cx, |this, cx| {
                    this.command(Command::Download { id, path }, cx)
                });
            }
        })
        .detach();
    }
    pub fn search_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Modal::Reaction(id) = self.modal.clone() {
            let count = ui::reactions::CHOICES.len();
            match event.keystroke.key.as_str() {
                "left" => self.reaction_selection = self.reaction_selection.saturating_sub(1),
                "right" => self.reaction_selection = (self.reaction_selection + 1).min(count - 1),
                "up" => self.reaction_selection = self.reaction_selection.saturating_sub(8),
                "down" => self.reaction_selection = (self.reaction_selection + 8).min(count - 1),
                "enter" => {
                    let emoji = ui::reactions::CHOICES[self.reaction_selection].0;
                    self.toggle_reaction(id, emoji.into(), window, cx);
                }
                _ => return,
            }
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if self.modal != Modal::Search {
            if matches!(
                self.modal,
                Modal::Direct
                    | Modal::Members
                    | Modal::Settings
                    | Modal::Manage
                    | Modal::Pins
                    | Modal::Profile(_)
                    | Modal::ConfirmDelete(_)
            ) {
                return;
            }
            if event.keystroke.key == "enter" && !matches!(self.modal, Modal::Edit(_)) {
                cx.stop_propagation();
                self.submit_modal(window, cx);
            }
            return;
        }
        let text = self.search.read(cx).value().to_string();
        let text = text.trim_start_matches('#').to_lowercase();
        let channels: Vec<_> = self
            .snapshot
            .channels
            .iter()
            .filter(|c| {
                ui::channel_label(c, &self.snapshot.users)
                    .to_lowercase()
                    .contains(&text)
            })
            .take(8)
            .cloned()
            .collect();
        let count = channels.len() + self.snapshot.search_results.len();
        match event.keystroke.key.as_str() {
            "down" => {
                self.search_selection = (self.search_selection + 1).min(count.saturating_sub(1));
            }
            "up" => {
                self.search_selection = self.search_selection.saturating_sub(1);
            }
            "enter" => {
                if let Some(ch) = channels.get(self.search_selection) {
                    self.select_channel(ch.id.clone(), window, cx);
                } else if let Some(m) = self
                    .snapshot
                    .search_results
                    .get(self.search_selection.saturating_sub(channels.len()))
                    .cloned()
                {
                    self.select_channel(m.channel_id.clone(), window, cx);
                    self.command(Command::SelectThread(Some(m.parent_id.unwrap_or(m.id))), cx);
                }
            }
            _ => return,
        }
        if self.search_selection >= channels.len() {
            self.search_scroll.scroll_to_item(
                self.search_selection - channels.len(),
                ScrollStrategy::Center,
            );
        }
        cx.stop_propagation();
        cx.notify();
    }
    fn quick_search(&mut self, _: &QuickSearch, window: &mut Window, cx: &mut Context<Self>) {
        if self.snapshot.user_id.is_some() {
            self.open_modal(Modal::Search, window, cx);
        }
    }
    fn close_overlay(&mut self, _: &CloseOverlay, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal != Modal::None {
            self.modal = Modal::None;
        } else {
            self.command(Command::SelectThread(None), cx);
        }
        self.focus.focus(window);
        cx.notify();
    }
}
impl Focusable for Messenger {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl Render for Messenger {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let authenticated = self.snapshot.user_id.is_some();
        let colors = ui::palette(cx);
        div()
            .key_context("Messenger")
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .relative()
            .bg(rgb(colors.bg))
            .text_color(rgb(colors.text))
            .text_sm()
            .font_family(".SystemUIFont")
            .on_action(cx.listener(Self::quick_search))
            .on_action(cx.listener(Self::close_overlay))
            .child(if authenticated && self.modal == Modal::Settings {
                ui::settings::render(self, window, cx).into_any_element()
            } else if authenticated
                && matches!(self.modal, Modal::Manage | Modal::Pins | Modal::Profile(_))
            {
                ui::management::render(self, window, cx).into_any_element()
            } else if authenticated {
                ui::shell(self, window, cx).into_any_element()
            } else {
                ui::login(self, window, cx).into_any_element()
            })
            .when(
                self.modal != Modal::None
                    && !matches!(
                        self.modal,
                        Modal::Settings | Modal::Manage | Modal::Pins | Modal::Profile(_)
                    ),
                |view| view.child(ui::search::overlay(self, window, cx)),
            )
    }
}
