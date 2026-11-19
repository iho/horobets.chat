use gpui::prelude::*;
pub mod composer;
pub mod feed;
pub mod search;
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
pub const BG: u32 = 0x10141c;
pub const PANEL: u32 = 0x151b25;
pub const BORDER: u32 = 0x273142;
pub const TEXT: u32 = 0xe7edf6;
pub const MUTED: u32 = 0x8998ae;
pub const ACCENT: u32 = 0x62dbc3;
pub fn muted(text: impl Into<SharedString>) -> Div {
    div().text_xs().text_color(rgb(MUTED)).child(text.into())
}
pub fn shell(
    this: &mut Messenger,
    window: &mut Window,
    cx: &mut Context<Messenger>,
) -> impl IntoElement + use<> {
    let channel = this
        .snapshot
        .channels
        .iter()
        .find(|c| Some(&c.id) == this.snapshot.channel.as_ref());
    let heading = channel
        .map(|c| format!("# {}", c.name))
        .unwrap_or_else(|| "Your workspace".into());
    let topic = channel
        .and_then(|c| c.topic.clone())
        .unwrap_or_else(|| "A little more focus. A lot less noise.".into());
    let header = div()
        .h(px(68.))
        .flex_shrink_0()
        .px_6()
        .border_b_1()
        .border_color(rgb(BORDER))
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(heading),
                )
                .child(muted(topic)),
        )
        .child(Button::new("search-open").label("Search   ⌘ K").on_click(
            cx.listener(|this, _, window, cx| this.open_modal(Modal::Search, window, cx)),
        ));
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
                        .bg(rgb(0x402832))
                        .text_color(rgb(0xffb8bf))
                        .child(
                            this.local_error
                                .clone()
                                .or(this.snapshot.error.clone())
                                .unwrap_or_default(),
                        )
                        .child(Button::new("dismiss-error").label("Dismiss").on_click(
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
                    .bg(rgb(0x1b2937))
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
                .bg(rgb(PANEL))
                .border_1()
                .border_color(rgb(BORDER))
                .child(
                    div()
                        .text_3xl()
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(ACCENT))
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
                            div().text_color(rgb(0xffb8bf)).child(
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
                    Button::new("switch-auth")
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

pub fn avatar(this: &Messenger, user: &str, diameter: f32) -> AnyElement {
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
            .bg(rgb(0x285654))
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
