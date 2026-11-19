use super::*;
use client_core::{CachedMessage, OutboxStatus};
use gpui_component::text::TextView;
use shared_proto::ClientEvent;
pub fn render(
    this: &mut Messenger,
    _window: &mut Window,
    cx: &mut Context<Messenger>,
) -> impl IntoElement + use<> {
    let entity = cx.entity();
    div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .when(this.snapshot.has_older, |view| {
            view.child(
                Button::new("older")
                    .label("Load earlier messages")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.command(Command::LoadOlder, cx);
                    })),
            )
        })
        .when(this.visible.is_empty(), |view| {
            view.child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .justify_center()
                    .items_center()
                    .gap_3()
                    .child(div().text_3xl().text_color(rgb(ACCENT)).child("#"))
                    .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child(
                        if this.snapshot.channel.is_some() {
                            "Every conversation starts somewhere."
                        } else {
                            "A space for your team."
                        },
                    ))
                    .child(muted(if this.snapshot.channel.is_some() {
                        "Send the first message below."
                    } else {
                        "Create a workspace and channel from the sidebar."
                    })),
            )
        })
        .when(!this.visible.is_empty(), |view| {
            view.child(
                list(this.feed.clone(), move |ix, window, cx| {
                    entity.update(cx, |this, cx| {
                        let Some(&index) = this.visible.get(ix) else {
                            return div().into_any_element();
                        };
                        let message = this.snapshot.messages[index].clone();
                        let grouped = ix > 0 && {
                            let previous = &this.snapshot.messages[this.visible[ix - 1]].message;
                            previous.user_id == message.message.user_id
                                && message.message.created_at - previous.created_at < 300_000
                        };
                        row(this, &message, grouped, false, window, cx)
                    })
                })
                .size_full(),
            )
        })
}
pub fn row(
    this: &mut Messenger,
    cached: &CachedMessage,
    grouped: bool,
    in_thread: bool,
    window: &mut Window,
    cx: &mut Context<Messenger>,
) -> AnyElement {
    let m = &cached.message;
    let id = m.id.clone();
    let mine = this.snapshot.user_id.as_ref() == Some(&m.user_id);
    let author = if mine {
        "You".to_owned()
    } else {
        this.snapshot
            .users
            .iter()
            .find(|u| u.id == m.user_id)
            .map(|u| u.display_name.clone())
            .unwrap_or_else(|| format!("Member {}", m.user_id.chars().take(6).collect::<String>()))
    };
    let deleted = m.deleted_at.is_some();
    let replies = *this.snapshot.reply_counts.get(&m.id).unwrap_or(&0);
    let time = chrono::DateTime::from_timestamp_millis(m.created_at)
        .map(|t| t.with_timezone(&chrono::Local).format("%H:%M").to_string())
        .unwrap_or_default();
    let parent = m.parent_id.clone().unwrap_or_else(|| id.clone());
    let parent_reply = parent.clone();
    let edit_id = id.clone();
    let edit_body = m.body_md.clone();
    let delete_id = id.clone();
    let react_id = id.clone();
    let retry_id = id.clone();
    div()
        .id(SharedString::from(format!(
            "{}-{}",
            if in_thread { "thread" } else { "feed" },
            id
        )))
        .px_6()
        .pt(if grouped { px(2.) } else { px(18.) })
        .pb_2()
        .flex()
        .gap_3()
        .hover(|v| v.bg(rgb(0x161e2a)))
        .child(
            div()
                .w(px(32.))
                .flex_shrink_0()
                .when(!grouped, |v| v.child(super::avatar(this, &m.user_id, 32.))),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_2()
                .when(!grouped, |v| {
                    v.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(div().font_weight(FontWeight::SEMIBOLD).child(author))
                            .child(muted(time))
                            .when(m.edited_at.is_some(), |v| v.child(muted("edited"))),
                    )
                })
                .when(m.topic_tag.is_some(), |v| {
                    v.child(
                        div()
                            .text_xs()
                            .text_color(rgb(ACCENT))
                            .child(format!("◈ {}", m.topic_tag.as_deref().unwrap_or_default())),
                    )
                })
                .child(if deleted {
                    muted("Message deleted").into_any_element()
                } else {
                    TextView::markdown(
                        SharedString::from(format!(
                            "markdown-{}-{id}",
                            if in_thread { "thread" } else { "feed" }
                        )),
                        m.body_md.clone(),
                        window,
                        cx,
                    )
                    .selectable(true)
                    .into_any_element()
                })
                .when(!deleted, |v| {
                    v.children(
                        this.snapshot
                            .message_attachments
                            .get(&id)
                            .cloned()
                            .unwrap_or_default()
                            .into_iter()
                            .map(|a| {
                                let aid = a.id.clone();
                                let name = a.filename.clone();
                                let download = Button::new(SharedString::from(format!(
                                    "download-{}-{aid}",
                                    if in_thread { "thread" } else { "feed" }
                                )))
                                .label(format!(
                                    "↓ {} · {} KB",
                                    a.filename,
                                    (a.size_bytes.max(0) as u64).div_ceil(1024)
                                ))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.save_attachment(aid.clone(), name.clone(), cx)
                                }));
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .when_some(
                                        this.snapshot.image_previews.get(&a.id).cloned(),
                                        |v, path| {
                                            v.child(
                                                img(path)
                                                    .w_full()
                                                    .max_w(px(440.))
                                                    .h(px(220.))
                                                    .object_fit(ObjectFit::Contain)
                                                    .rounded_lg(),
                                            )
                                        },
                                    )
                                    .child(download)
                            }),
                    )
                })
                .when(!cached.reactions.is_empty(), |v| {
                    v.child(div().flex().flex_wrap().gap_2().children(
                        cached.reactions.clone().into_iter().map(|r| {
                            let id = id.clone();
                            let emoji = r.emoji.clone();
                            Button::new(SharedString::from(format!("reaction-{id}-{}", r.emoji)))
                                .label(format!("{} {}", r.emoji, r.count))
                                .when(r.mine, |b| b.primary())
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.command(
                                        Command::Event(ClientEvent::ToggleReaction {
                                            message_id: id.clone(),
                                            emoji: emoji.clone(),
                                        }),
                                        cx,
                                    );
                                }))
                        }),
                    ))
                })
                .when(
                    !deleted && matches!(cached.status, OutboxStatus::Sent),
                    |v| {
                        v.child(
                            div()
                                .flex()
                                .gap_2()
                                .when(!in_thread, |v| {
                                    v.child(
                                        Button::new(SharedString::from(format!("reply-{id}")))
                                            .label(if replies > 0 {
                                                format!("{replies} replies →")
                                            } else {
                                                "Reply in thread".into()
                                            })
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.command(
                                                    Command::SelectThread(Some(
                                                        parent_reply.clone(),
                                                    )),
                                                    cx,
                                                );
                                                this.reply.update(cx, |i, cx| i.focus(window, cx));
                                            })),
                                    )
                                })
                                .child(
                                    Button::new(SharedString::from(format!("react-{id}")))
                                        .label("+ 👍")
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.command(
                                                Command::Event(ClientEvent::ToggleReaction {
                                                    message_id: react_id.clone(),
                                                    emoji: "👍".into(),
                                                }),
                                                cx,
                                            );
                                        })),
                                )
                                .when(mine, |v| {
                                    v.child(
                                        Button::new(SharedString::from(format!("edit-{id}")))
                                            .label("Edit")
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.open_modal(
                                                    Modal::Edit(edit_id.clone()),
                                                    window,
                                                    cx,
                                                );
                                                this.name.update(cx, |i, cx| {
                                                    i.set_value(edit_body.clone(), window, cx)
                                                });
                                            })),
                                    )
                                    .child(
                                        Button::new(SharedString::from(format!("delete-{id}")))
                                            .label("Delete")
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.command(
                                                    Command::Event(ClientEvent::DeleteMessage {
                                                        message_id: delete_id.clone(),
                                                    }),
                                                    cx,
                                                );
                                            })),
                                    )
                                }),
                        )
                    },
                )
                .when(matches!(cached.status, OutboxStatus::Pending), |v| {
                    v.child(muted("◌ Queued · will send when connected"))
                })
                .when(matches!(cached.status, OutboxStatus::Failed(_)), |v| {
                    v.child(
                        div()
                            .text_color(rgb(0xffb8bf))
                            .child(match &cached.status {
                                OutboxStatus::Failed(e) => format!("Not sent: {e}"),
                                _ => String::new(),
                            })
                            .child(
                                Button::new(SharedString::from(format!("retry-{id}")))
                                    .label("Retry")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.command(Command::Retry(retry_id.clone()), cx);
                                    })),
                            ),
                    )
                }),
        )
        .into_any_element()
}
