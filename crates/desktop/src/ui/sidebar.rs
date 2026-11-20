use super::*;
pub fn render(this: &mut Messenger, cx: &mut Context<Messenger>) -> impl IntoElement + use<> {
    let colors = palette(cx);
    let current = this.snapshot.workspace.clone();
    div()
        .w_full()
        .flex_shrink_0()
        .h_full()
        .flex()
        .flex_col()
        .bg(rgb(colors.panel))
        .border_r_1()
        .border_color(rgb(colors.border))
        .child(
            div()
                .h(px(68.))
                .px_5()
                .flex()
                .items_center()
                .gap_3()
                .border_b_1()
                .border_color(rgb(colors.border))
                .child(
                    div()
                        .size(px(36.))
                        .rounded_lg()
                        .bg(rgb(colors.selected))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_2xl()
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(colors.accent))
                        .child("h."),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("horobets.chat"),
                        )
                        .child(muted(if this.snapshot.connected {
                            "● Connected"
                        } else {
                            "○ Offline"
                        })),
                ),
        )
        .child(
            div()
                .id("sidebar-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .p_3()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .px_2()
                        .pt_3()
                        .pb_2()
                        .flex()
                        .justify_between()
                        .child(muted("WORKSPACES"))
                        .child(
                            secondary_button("new-workspace")
                                .label("+")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.open_modal(Modal::Workspace, window, cx)
                                })),
                        ),
                )
                .children(this.snapshot.workspaces.clone().into_iter().map(|w| {
                    let id = w.id.clone();
                    div()
                        .id(SharedString::from(format!("workspace-{}", w.id)))
                        .px_3()
                        .py_2()
                        .rounded_md()
                        .cursor_pointer()
                        .when(current.as_ref() == Some(&w.id), |v| {
                            v.bg(rgb(colors.selected))
                                .text_color(rgb(colors.accent))
                                .font_weight(FontWeight::SEMIBOLD)
                        })
                        .hover(|v| v.bg(rgb(colors.hover)))
                        .child(w.name)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.composer
                                .update(cx, |i, cx| i.set_value("", window, cx));
                            this.command(Command::SelectWorkspace(id.clone()), cx);
                        }))
                }))
                .child(
                    secondary_button("saved-messages")
                        .label("Saved messages")
                        .tooltip("Private notes and files · only you")
                        .disabled(this.snapshot.workspace.is_none())
                        .on_click(cx.listener(|this, _, window, cx| {
                            if let Some(user) = this.snapshot.user_id.clone() {
                                this.open_direct(user, window, cx);
                            }
                        })),
                )
                .child(
                    div()
                        .px_2()
                        .pt_6()
                        .pb_2()
                        .flex()
                        .justify_between()
                        .child(muted("CHANNELS"))
                        .child(
                            secondary_button("new-channel")
                                .label("+")
                                .disabled(this.snapshot.workspace.is_none())
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.open_modal(Modal::Channel, window, cx)
                                })),
                        ),
                )
                .children(
                    this.snapshot
                        .channels
                        .clone()
                        .into_iter()
                        .filter(|ch| !ch.is_direct && !ch.is_saved)
                        .map(|ch| {
                            let id = ch.id.clone();
                            let count = *this.snapshot.unread.get(&ch.id).unwrap_or(&0);
                            div()
                                .id(SharedString::from(format!("channel-{}", ch.id)))
                                .px_3()
                                .py_2()
                                .rounded_md()
                                .cursor_pointer()
                                .flex()
                                .items_center()
                                .gap_2()
                                .justify_between()
                                .when(this.snapshot.channel.as_ref() == Some(&ch.id), |v| {
                                    v.bg(rgb(colors.selected)).text_color(rgb(colors.accent))
                                })
                                .hover(|v| v.bg(rgb(colors.hover)))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .child(channel_label(&ch, &this.snapshot.users)),
                                )
                                .when(count > 0, |v| {
                                    v.child(
                                        div()
                                            .px_2()
                                            .rounded_full()
                                            .bg(rgb(colors.badge))
                                            .text_xs()
                                            .child(count.to_string()),
                                    )
                                })
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.select_channel(id.clone(), window, cx)
                                }))
                        }),
                )
                .child(
                    div()
                        .px_2()
                        .pt_6()
                        .pb_2()
                        .flex()
                        .justify_between()
                        .items_center()
                        .child(muted("DIRECT MESSAGES"))
                        .child(
                            secondary_button("new-direct-message")
                                .label("+")
                                .disabled(this.snapshot.workspace.is_none())
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.open_modal(Modal::Direct, window, cx)
                                })),
                        ),
                )
                .children(
                    this.snapshot
                        .channels
                        .clone()
                        .into_iter()
                        .filter(|ch| ch.is_direct)
                        .map(|ch| {
                            let id = ch.id.clone();
                            let count = *this.snapshot.unread.get(&id).unwrap_or(&0);
                            div()
                                .id(SharedString::from(format!("direct-{}", ch.id)))
                                .px_3()
                                .py_2()
                                .rounded_md()
                                .cursor_pointer()
                                .flex()
                                .items_center()
                                .gap_2()
                                .justify_between()
                                .when(this.snapshot.channel.as_ref() == Some(&ch.id), |v| {
                                    v.bg(rgb(colors.selected)).text_color(rgb(colors.accent))
                                })
                                .hover(|v| v.bg(rgb(colors.hover)))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .child(channel_label(&ch, &this.snapshot.users)),
                                )
                                .when(count > 0, |v| {
                                    v.child(div().text_xs().child(count.to_string()))
                                })
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.select_channel(id.clone(), window, cx)
                                }))
                        }),
                )
                .when(this.snapshot.channels.is_empty(), |v| {
                    v.child(
                        div()
                            .px_3()
                            .py_2()
                            .child(muted("Create a channel to start talking.")),
                    )
                }),
        )
        .child(
            div()
                .p_3()
                .max_h(px(260.))
                .id("sidebar-account-scroll")
                .overflow_y_scroll()
                .border_t_1()
                .border_color(rgb(colors.border))
                .flex()
                .flex_col()
                .gap_2()
                .when(this.snapshot.user_id.is_some(), |v| {
                    v.child(
                        div()
                            .flex()
                            .gap_2()
                            .items_center()
                            .child(super::avatar(
                                this,
                                this.snapshot.user_id.as_deref().unwrap_or_default(),
                                36.,
                                cx,
                            ))
                            .child(
                                div().flex_1().min_w_0().child(
                                    this.snapshot
                                        .me
                                        .as_ref()
                                        .map(|u| {
                                            u.handle
                                                .as_ref()
                                                .map(|h| format!("@{h}"))
                                                .unwrap_or_else(|| u.display_name.clone())
                                        })
                                        .unwrap_or_else(|| "Your profile".into()),
                                ),
                            )
                            .child(
                                secondary_button("change-profile-photo")
                                    .label("Photo")
                                    .on_click(cx.listener(|this, _, _, cx| this.choose_avatar(cx))),
                            ),
                    )
                })
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .child(
                            secondary_button("settings-open")
                                .label("Settings")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.open_modal(Modal::Settings, window, cx)
                                })),
                        )
                        .child(
                            secondary_button("invite")
                                .label("Invite a teammate")
                                .disabled(this.snapshot.workspace.is_none())
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.open_modal(Modal::Invite, window, cx)
                                })),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            secondary_button("refresh")
                                .label("Refresh")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.command(Command::Refresh, cx);
                                })),
                        )
                        .child(
                            secondary_button("logout")
                                .label("Sign out")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.drafts.clear();
                                    this.composer
                                        .update(cx, |i, cx| i.set_value("", window, cx));
                                    this.reply.update(cx, |i, cx| i.set_value("", window, cx));
                                    this.command(Command::Logout, cx);
                                })),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .child(
                            secondary_button("workspace-management")
                                .label("Workspace")
                                .tooltip("Workspace settings, people, and moderation")
                                .disabled(this.snapshot.workspace.is_none())
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.open_modal(Modal::Manage, window, cx)
                                })),
                        )
                        .child(
                            secondary_button("pinned-messages")
                                .label("Pins")
                                .disabled(this.snapshot.workspace.is_none())
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.open_modal(Modal::Pins, window, cx)
                                })),
                        ),
                )
                .child(muted(this.snapshot.server.clone())),
        )
}
