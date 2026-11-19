use super::*;
pub fn render(this: &mut Messenger, cx: &mut Context<Messenger>) -> impl IntoElement + use<> {
    let current = this.snapshot.workspace.clone();
    div()
        .w_full()
        .flex_shrink_0()
        .h_full()
        .flex()
        .flex_col()
        .bg(rgb(PANEL))
        .border_r_1()
        .border_color(rgb(BORDER))
        .child(
            div()
                .h(px(68.))
                .px_5()
                .flex()
                .items_center()
                .gap_3()
                .border_b_1()
                .border_color(rgb(BORDER))
                .child(
                    div()
                        .text_2xl()
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(ACCENT))
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
                            Button::new("new-workspace")
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
                        .when(current.as_ref() == Some(&w.id), |v| v.bg(rgb(0x24323e)))
                        .hover(|v| v.bg(rgb(0x24323e)))
                        .child(w.name)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.composer
                                .update(cx, |i, cx| i.set_value("", window, cx));
                            this.command(Command::SelectWorkspace(id.clone()), cx);
                        }))
                }))
                .child(
                    div()
                        .px_2()
                        .pt_6()
                        .pb_2()
                        .flex()
                        .justify_between()
                        .child(muted("CHANNELS"))
                        .child(
                            Button::new("new-channel")
                                .label("+")
                                .disabled(this.snapshot.workspace.is_none())
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.open_modal(Modal::Channel, window, cx)
                                })),
                        ),
                )
                .children(this.snapshot.channels.clone().into_iter().map(|ch| {
                    let id = ch.id.clone();
                    let count = *this.snapshot.unread.get(&ch.id).unwrap_or(&0);
                    div()
                        .id(SharedString::from(format!("channel-{}", ch.id)))
                        .px_3()
                        .py_2()
                        .rounded_md()
                        .cursor_pointer()
                        .flex()
                        .justify_between()
                        .when(this.snapshot.channel.as_ref() == Some(&ch.id), |v| {
                            v.bg(rgb(0x213a3c)).text_color(rgb(ACCENT))
                        })
                        .hover(|v| v.bg(rgb(0x24323e)))
                        .child(format!(
                            "{} {}",
                            if ch.is_private { "▣" } else { "#" },
                            ch.name
                        ))
                        .when(count > 0, |v| {
                            v.child(
                                div()
                                    .px_2()
                                    .rounded_md()
                                    .bg(rgb(0x344557))
                                    .text_xs()
                                    .child(count.to_string()),
                            )
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.select_channel(id.clone(), window, cx)
                        }))
                }))
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
                .border_t_1()
                .border_color(rgb(BORDER))
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
                            ))
                            .child(
                                div().flex_1().min_w_0().child(
                                    this.snapshot
                                        .me
                                        .as_ref()
                                        .map(|u| u.display_name.clone())
                                        .unwrap_or_else(|| "Your profile".into()),
                                ),
                            )
                            .child(
                                Button::new("change-profile-photo")
                                    .label("Photo")
                                    .on_click(cx.listener(|this, _, _, cx| this.choose_avatar(cx))),
                            ),
                    )
                })
                .child(
                    Button::new("invite")
                        .label("Invite a teammate")
                        .disabled(this.snapshot.workspace.is_none())
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_modal(Modal::Invite, window, cx)
                        })),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new("refresh")
                                .label("Refresh")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.command(Command::Refresh, cx);
                                })),
                        )
                        .child(
                            Button::new("logout")
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
                .child(muted(this.snapshot.server.clone())),
        )
}
