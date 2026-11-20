use gpui::prelude::*;
mod autocomplete;
pub mod composer;
pub mod conversations;
pub mod feed;
pub mod management;
pub mod reactions;
pub mod references;
pub mod search;
pub mod settings;
pub mod sidebar;
pub mod thread;
use crate::state::{Messenger, Modal};
use client_core::Command;
use gpui::*;
use gpui_component::{
    Disableable,
    button::{Button, ButtonVariants},
    input::Input,
    resizable::{ResizableState, h_resizable, resizable_panel},
};
#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: u32,
    pub panel: u32,
    pub border: u32,
    pub text: u32,
    pub muted: u32,
    pub accent: u32,
    pub hover: u32,
    pub selected: u32,
    pub error_bg: u32,
    pub error_text: u32,
    pub notice: u32,
    pub badge: u32,
    pub avatar: u32,
    pub overlay: u32,
}
pub fn palette(cx: &App) -> Palette {
    if gpui_component::Theme::global(cx).is_dark() {
        Palette {
            bg: 0x17191f,
            panel: 0x111318,
            border: 0x30333d,
            text: 0xedf0f5,
            muted: 0xa0a7b6,
            accent: 0x62dbc3,
            hover: 0x22262f,
            selected: 0x203c37,
            error_bg: 0x402832,
            error_text: 0xffb8bf,
            notice: 0x1b2937,
            badge: 0x344557,
            avatar: 0x285654,
            overlay: 0x080c14dd,
        }
    } else {
        Palette {
            bg: 0xffffff,
            panel: 0xf4f6f8,
            border: 0xdde2e8,
            text: 0x172335,
            muted: 0x54657c,
            accent: 0x117562,
            hover: 0xe5eaf1,
            selected: 0xd9f0e9,
            error_bg: 0xffe7e9,
            error_text: 0x9d2236,
            notice: 0xe6f0fa,
            badge: 0xd7e0ed,
            avatar: 0xc2e8de,
            overlay: 0x17233566,
        }
    }
}
/// Common secondary controls have a visible outline on both page and sidebar surfaces.
pub fn secondary_button(id: impl Into<ElementId>) -> Button {
    Button::new(id).outline()
}
/// Keep component controls consistent with the app palette whenever appearance changes.
pub fn configure_controls(cx: &mut App) {
    let dark = gpui_component::Theme::global(cx).is_dark();
    let colors = palette(cx);
    let theme = gpui_component::Theme::global_mut(cx);
    theme.background = rgb(colors.bg).into();
    theme.foreground = rgb(colors.text).into();
    theme.border = rgb(if dark { 0x596a80 } else { 0x73859b }).into();
    theme.input = theme.border;
    theme.ring = rgb(colors.accent).into();
    theme.secondary = rgb(if dark { 0x263344 } else { 0xe1e7ef }).into();
    theme.secondary_foreground = rgb(colors.text).into();
    theme.secondary_hover = rgb(if dark { 0x35475d } else { 0xcbd7e5 }).into();
    theme.secondary_active = rgb(if dark { 0x405872 } else { 0xb8c9dd }).into();
    theme.primary = rgb(if dark { 0x62dbc3 } else { 0x146b59 }).into();
    theme.primary_foreground = rgb(if dark { 0x092e28 } else { 0xffffff }).into();
    theme.primary_hover = rgb(if dark { 0x88ebd8 } else { 0x0f5749 }).into();
    theme.primary_active = rgb(if dark { 0x46bea8 } else { 0x0a493d }).into();
}
#[derive(IntoElement)]
pub struct Muted(SharedString);
impl RenderOnce for Muted {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .text_xs()
            .text_color(rgb(palette(cx).muted))
            .child(self.0)
    }
}
pub fn muted(text: impl Into<SharedString>) -> Muted {
    Muted(text.into())
}
pub fn channel_label(channel: &shared_proto::Channel, users: &[shared_proto::User]) -> String {
    if channel.is_saved {
        "Saved messages".into()
    } else if channel.is_direct {
        users
            .iter()
            .find(|u| Some(&u.id) == channel.peer_user_id.as_ref())
            .map(|u| {
                u.handle
                    .as_ref()
                    .map(|h| format!("@{h}"))
                    .unwrap_or_else(|| u.display_name.clone())
            })
            .unwrap_or_else(|| "Direct message".into())
    } else {
        format!(
            "{} {}",
            if channel.is_private { "▣" } else { "#" },
            channel.name
        )
    }
}
pub fn shell(
    this: &mut Messenger,
    window: &mut Window,
    cx: &mut Context<Messenger>,
) -> impl IntoElement + use<> {
    let colors = palette(cx);
    let channel = this
        .snapshot
        .channels
        .iter()
        .find(|c| Some(&c.id) == this.snapshot.channel.as_ref());
    let heading = channel
        .map(|c| channel_label(c, &this.snapshot.users))
        .unwrap_or_else(|| "Your workspace".into());
    let topic = channel.and_then(|c| c.topic.clone()).unwrap_or_else(|| {
        if channel.is_some_and(|c| c.is_saved) {
            "Private notes and files · only you can read these".into()
        } else if channel.is_some_and(|c| c.is_direct) {
            "Only you and this teammate can read this conversation.".into()
        } else if channel.is_some_and(|c| c.is_private) {
            "Private channel · invited members only".into()
        } else {
            "A little more focus. A lot less noise.".into()
        }
    });
    let header = div()
        .h(px(68.))
        .flex_shrink_0()
        .px_6()
        .border_b_1()
        .border_color(rgb(colors.border))
        .flex()
        .items_center()
        .gap_3()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(heading),
                )
                .child(div().overflow_hidden().child(muted(topic))),
        )
        .when(
            channel.is_some_and(|c| c.is_private && !c.is_direct && !c.is_saved),
            |v| {
                v.child(
                    secondary_button("channel-members")
                        .label("Members")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_modal(Modal::Members, window, cx)
                        })),
                )
            },
        )
        .when(
            channel.is_some_and(|c| {
                !c.is_direct
                    && !c.is_saved
                    && (this.snapshot.can_moderate()
                        || Some(&c.created_by) == this.snapshot.user_id.as_ref())
            }),
            |v| {
                v.child(
                    secondary_button("edit-current-channel")
                        .label("Details")
                        .on_click(cx.listener(|this, _, window, cx| {
                            if let Some(ch) = this.snapshot.channel.clone() {
                                this.open_modal(Modal::ChannelEdit(ch), window, cx);
                            }
                        })),
                )
            },
        )
        .child(
            secondary_button("search-open")
                .label("Search   ⌘ K")
                .on_click(
                    cx.listener(|this, _, window, cx| this.open_modal(Modal::Search, window, cx)),
                ),
        );
    let sidebar = sidebar::render(this, cx);
    let center = div()
        .flex_1()
        .min_w_0()
        .h_full()
        .flex()
        .flex_col()
        .child(header)
        .when(
            this.local_error.is_some() || this.snapshot.error.is_some(),
            |view| {
                view.child(
                    div()
                        .px_6()
                        .py_2()
                        .bg(rgb(colors.error_bg))
                        .text_color(rgb(colors.error_text))
                        .child(
                            this.local_error
                                .clone()
                                .or(this.snapshot.error.clone())
                                .unwrap_or_default(),
                        )
                        .child(secondary_button("dismiss-error").label("Dismiss").on_click(
                            cx.listener(|this, _, _, cx| {
                                this.local_error = None;
                                this.command(Command::ClearError, cx);
                            }),
                        )),
                )
            },
        )
        .when(this.snapshot.notice.is_some(), |view| {
            view.child(
                div()
                    .px_6()
                    .py_2()
                    .bg(rgb(colors.notice))
                    .child(muted(this.snapshot.notice.clone().unwrap_or_default())),
            )
        })
        .child(feed::render(this, window, cx))
        .when(this.snapshot.channel.is_some(), |view| {
            view.child(composer::render(this, false, window, cx))
        });
    let conversation = if this.snapshot.thread.is_some() {
        let thread = thread::render(this, window, cx);
        h_resizable("message-thread-panes")
            .child(
                resizable_panel()
                    .size_range(px(300.)..Pixels::MAX)
                    .child(center),
            )
            .child(
                resizable_panel()
                    .size(px(this.pane_sizes.thread))
                    .size_range(px(260.)..px(600.))
                    .child(thread),
            )
            .on_resize(cx.listener(|this, state: &Entity<ResizableState>, _, cx| {
                if let Some(size) = state.read(cx).sizes().get(1) {
                    this.pane_sizes.thread = f32::from(*size);
                    this.save_pane_sizes(cx);
                }
            }))
            .into_any_element()
    } else {
        center.into_any_element()
    };
    h_resizable("sidebar-conversation-panes")
        .child(
            resizable_panel()
                .size(px(this.pane_sizes.sidebar))
                .size_range(px(180.)..px(420.))
                .child(sidebar),
        )
        .child(
            resizable_panel()
                .size_range(px(560.)..Pixels::MAX)
                .child(conversation),
        )
        .on_resize(cx.listener(|this, state: &Entity<ResizableState>, _, cx| {
            if let Some(size) = state.read(cx).sizes().first() {
                this.pane_sizes.sidebar = f32::from(*size);
                this.save_pane_sizes(cx);
            }
        }))
}
pub fn login(
    this: &mut Messenger,
    _window: &mut Window,
    cx: &mut Context<Messenger>,
) -> impl IntoElement + use<> {
    let colors = palette(cx);
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .w(px(420.))
                .p_8()
                .flex()
                .flex_col()
                .gap_4()
                .rounded_xl()
                .bg(rgb(colors.panel))
                .border_1()
                .border_color(rgb(colors.border))
                .child(
                    div()
                        .text_3xl()
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(colors.accent))
                        .child("h."),
                )
                .child(
                    div()
                        .text_2xl()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(if this.register {
                            "Make room for your team."
                        } else {
                            "Welcome back."
                        }),
                )
                .child(muted("Your conversations. Your server. Your space."))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(muted("SERVER"))
                        .child(Input::new(&this.server)),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(muted("EMAIL"))
                        .child(Input::new(&this.email)),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(muted(if this.register {
                            "PASSWORD · AT LEAST 12 CHARACTERS"
                        } else {
                            "PASSWORD"
                        }))
                        .child(Input::new(&this.password)),
                )
                .when(
                    this.local_error.is_some() || this.snapshot.error.is_some(),
                    |view| {
                        view.child(
                            div().text_color(rgb(colors.error_text)).child(
                                this.local_error
                                    .clone()
                                    .or(this.snapshot.error.clone())
                                    .unwrap_or_default(),
                            ),
                        )
                    },
                )
                .child(
                    Button::new("sign-in")
                        .primary()
                        .label(if this.snapshot.busy {
                            "Connecting…"
                        } else if this.register {
                            "Create account"
                        } else {
                            "Sign in"
                        })
                        .disabled(this.snapshot.busy)
                        .on_click(cx.listener(|this, _, window, cx| this.authenticate(window, cx))),
                )
                .child(
                    secondary_button("switch-auth")
                        .label(if this.register {
                            "Already have an account? Sign in"
                        } else {
                            "New here? Create an account"
                        })
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.register = !this.register;
                            cx.notify();
                        })),
                )
                .child(muted(
                    "Session and offline history are saved on this device.",
                )),
        )
}

pub fn avatar(this: &Messenger, user: &str, diameter: f32, cx: &App) -> AnyElement {
    let colors = palette(cx);
    if let Some(path) = this.snapshot.avatars.get(user) {
        img(path.clone())
            .size(px(diameter))
            .object_fit(ObjectFit::Cover)
            .rounded_full()
            .into_any_element()
    } else {
        let name = this
            .snapshot
            .users
            .iter()
            .chain(this.snapshot.me.iter())
            .find(|u| u.id == user)
            .map(|u| u.display_name.as_str())
            .unwrap_or("?");
        div()
            .size(px(diameter))
            .flex_shrink_0()
            .rounded_full()
            .bg(rgb(colors.avatar))
            .flex()
            .items_center()
            .justify_center()
            .font_weight(FontWeight::SEMIBOLD)
            .child(
                name.chars()
                    .next()
                    .unwrap_or('?')
                    .to_uppercase()
                    .to_string(),
            )
            .into_any_element()
    }
}
