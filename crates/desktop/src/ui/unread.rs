use super::*;
pub fn render(this: &mut Messenger, cx: &mut Context<Messenger>) -> impl IntoElement + use<> {
    let colors = palette(cx);
    div()
        .id("unread-list")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .p_4()
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(muted("Unread messages in this workspace"))
                .child(
                    secondary_button("unread-mark-all")
                        .label("Mark all as read")
                        .disabled(this.snapshot.unread.is_empty())
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.command(Command::MarkAllRead, cx);
                        })),
                ),
        )
        .when(this.snapshot.unread_messages.is_empty(), |v| {
            v.child(
                div()
                    .py_8()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("You're all caught up"),
                    )
                    .child(muted("New messages will appear here.")),
            )
        })
        .children(this.snapshot.unread_messages.clone().into_iter().map(|m| {
            let channel = this
                .snapshot
                .channels
                .iter()
                .find(|c| c.id == m.channel_id)
                .map(|c| channel_label(c, &this.snapshot.users))
                .unwrap_or_default();
            let name = this
                .snapshot
                .users
                .iter()
                .find(|u| u.id == m.user_id)
                .map(|u| u.display_name.clone())
                .unwrap_or_else(|| "Member".into());
            let open_channel = m.channel_id.clone();
            let read_channel = m.channel_id.clone();
            let thread = m.parent_id.clone().unwrap_or_else(|| m.id.clone());
            let time = chrono::DateTime::from_timestamp_millis(m.created_at)
                .map(|t| {
                    t.with_timezone(&chrono::Local)
                        .format("%b %-d · %H:%M")
                        .to_string()
                })
                .unwrap_or_default();
            div()
                .id(SharedString::from(format!("unread-{}", m.id)))
                .border_b_1()
                .border_color(rgb(colors.border))
                .pb_3()
                .flex()
                .gap_2()
                .child(avatar(this, &m.user_id, CHAT_AVATAR, cx))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .child(div().font_weight(FontWeight::SEMIBOLD).child(name))
                                .child(muted(channel))
                                .child(muted(time)),
                        )
                        .child(div().child(if m.body_md.is_empty() {
                            "Attachment".into()
                        } else {
                            m.body_md.chars().take(280).collect::<String>()
                        }))
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .child(
                                    secondary_button(SharedString::from(format!(
                                        "unread-open-{}",
                                        m.id
                                    )))
                                    .small()
                                    .label(if m.parent_id.is_some() {
                                        "Open thread"
                                    } else {
                                        "Open message"
                                    })
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.modal = Modal::None;
                                        this.select_channel(open_channel.clone(), window, cx);
                                        this.command(
                                            Command::SelectThread(Some(thread.clone())),
                                            cx,
                                        );
                                    })),
                                )
                                .child(
                                    secondary_button(SharedString::from(format!(
                                        "unread-read-{}",
                                        m.id
                                    )))
                                    .small()
                                    .label("Mark conversation as read")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.command(
                                            Command::MarkChannelRead(read_channel.clone()),
                                            cx,
                                        );
                                    })),
                                ),
                        ),
                )
        }))
        .when(this.snapshot.unread_messages.len() >= 200, |v| {
            v.child(muted(
                "Showing the latest 200 unread messages. Open a conversation for its full history.",
            ))
        })
}
