use super::*;
use client_core::{CachedMessage, OutboxStatus};
use shared_proto::ClientEvent;
pub fn render(
    this: &mut Messenger,
    _window: &mut Window,
    cx: &mut Context<Messenger>,
) -> impl IntoElement + use<> {
    let colors = palette(cx);
    let entity = cx.entity();
    div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .when(this.snapshot.has_older, |view| {
            view.child(
                secondary_button("older")
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
                    .gap(px(CHAT_GAP))
                    .child(
                        div()
                            .size(px(72.))
                            .rounded_xl()
                            .bg(rgb(colors.selected))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_3xl()
                            .text_color(rgb(colors.accent))
                            .child("#"),
                    )
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
                        let day =
                            chrono::DateTime::from_timestamp_millis(message.message.created_at)
                                .map(|t| t.with_timezone(&chrono::Local).date_naive());
                        let previous = ix
                            .checked_sub(1)
                            .map(|i| &this.snapshot.messages[this.visible[i]].message);
                        let same_day = previous.is_some_and(|p| {
                            chrono::DateTime::from_timestamp_millis(p.created_at)
                                .map(|t| t.with_timezone(&chrono::Local).date_naive())
                                == day
                        });
                        let grouped = same_day
                            && previous.is_some_and(|p| {
                                p.user_id == message.message.user_id
                                    && message.message.created_at - p.created_at < 300_000
                            });
                        let message_row = row(this, &message, grouped, false, window, cx);
                        div()
                            .when(!same_day, |v| {
                                v.child(
                                    div()
                                        .px(px(CHAT_GUTTER))
                                        .pt_3()
                                        .pb_1()
                                        .flex()
                                        .items_center()
                                        .gap(px(CHAT_GAP))
                                        .child(div().flex_1().h(px(1.)).bg(rgb(colors.border)))
                                        .child(muted(
                                            day.map(|d| {
                                                let today = chrono::Local::now().date_naive();
                                                if d == today {
                                                    "Today".to_owned()
                                                } else if Some(d) == today.pred_opt() {
                                                    "Yesterday".to_owned()
                                                } else {
                                                    d.format("%B %-d, %Y").to_string()
                                                }
                                            })
                                            .unwrap_or_default(),
                                        ))
                                        .child(div().flex_1().h(px(1.)).bg(rgb(colors.border))),
                                )
                            })
                            .child(message_row)
                            .into_any_element()
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
    let colors = palette(cx);
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
    let profile_avatar = m.user_id.clone();
    let profile_name = m.user_id.clone();
    let personal = this
        .snapshot
        .channels
        .iter()
        .find(|c| c.id == m.channel_id)
        .is_some_and(|c| c.is_direct || c.is_saved);
    let moderate = this.snapshot.can_moderate() && !personal;
    let can_pin = personal || moderate;
    let pinned = this.snapshot.pins.iter().any(|pin| pin.message.id == id);
    let pin_id = id.clone();
    let moderated_delete = id.clone();
    div()
        .relative()
        .min_h(px(32.))
        .group("message-row")
        .id(SharedString::from(format!(
            "{}-{}",
            if in_thread { "thread" } else { "feed" },
            id
        )))
        .px(px(CHAT_GUTTER))
        .pt(if grouped { px(2.) } else { px(8.) })
        .pb_1()
        .flex()
        .gap(px(CHAT_GAP))
        .hover(|v| v.bg(rgb(colors.hover)))
        .child(
            div()
                .w(px(CHAT_AVATAR))
                .flex_shrink_0()
                .when(!grouped, |v| {
                    v.child(
                        div()
                            .id(SharedString::from(format!("profile-avatar-{id}")))
                            .cursor_pointer()
                            .child(super::avatar(this, &m.user_id, CHAT_AVATAR, cx))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_modal(Modal::Profile(profile_avatar.clone()), window, cx)
                            })),
                    )
                }),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_1()
                .when(!grouped, |v| {
                    v.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(CHAT_GAP))
                            .child(
                                div()
                                    .id(SharedString::from(format!("profile-name-{id}")))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .cursor_pointer()
                                    .child(author)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.open_modal(
                                            Modal::Profile(profile_name.clone()),
                                            window,
                                            cx,
                                        )
                                    })),
                            )
                            .when(pinned, |v| v.child(muted("📌 Pinned")))
                            .when_some(
                                this.snapshot
                                    .users
                                    .iter()
                                    .find(|u| u.id == m.user_id)
                                    .and_then(|u| u.handle.clone()),
                                |v, h| v.child(muted(format!("@{h}"))),
                            )
                            .child(muted(time))
                            .when(m.edited_at.is_some(), |v| v.child(muted("edited"))),
                    )
                })
                .when(m.topic_tag.is_some(), |v| {
                    v.child(
                        div()
                            .text_xs()
                            .text_color(rgb(colors.accent))
                            .child(format!("◈ {}", m.topic_tag.as_deref().unwrap_or_default())),
                    )
                })
                .child(if deleted {
                    muted("Message deleted").into_any_element()
                } else {
                    super::references::render(
                        this,
                        SharedString::from(format!(
                            "markdown-{}-{id}",
                            if in_thread { "thread" } else { "feed" }
                        )),
                        &m.body_md,
                        window,
                        cx,
                    )
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
                                let download = secondary_button(SharedString::from(format!(
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
                                .when(!r.mine, |b| b.outline())
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
                .when(!deleted && !in_thread && replies > 0, |v| {
                    let parent = parent.clone();
                    v.child(
                        div().child(
                            secondary_button(SharedString::from(format!("replies-{id}")))
                                .small()
                                .label(format!("{replies} replies"))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.command(Command::SelectThread(Some(parent.clone())), cx);
                                    this.reply.update(cx, |input, cx| input.focus(window, cx));
                                })),
                        ),
                    )
                })
                .when(
                    !deleted && matches!(cached.status, OutboxStatus::Sent),
                    |v| {
                        v.child(
                            div()
                                .absolute()
                                .top_0()
                                .right_4()
                                .mt_1()
                                .flex()
                                .items_center()
                                .gap_1()
                                .p_1()
                                .rounded_md()
                                .bg(rgb(colors.bg))
                                .border_1()
                                .border_color(rgb(colors.border))
                                .invisible()
                                .group_hover("message-row", |v| v.visible())
                                .when(!in_thread, |v| {
                                    v.child(
                                        secondary_button(SharedString::from(format!("reply-{id}")))
                                            .small()
                                            .label(if replies > 0 {
                                                format!("{replies} replies →")
                                            } else {
                                                "Reply".into()
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
                                    secondary_button(SharedString::from(format!("react-{id}")))
                                        .small()
                                        .label("React")
                                        .tooltip("Choose an emoji reaction")
                                        .disabled(!this.snapshot.connected)
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.open_modal(
                                                Modal::Reaction(react_id.clone()),
                                                window,
                                                cx,
                                            );
                                        })),
                                )
                                .when(can_pin, |v| {
                                    v.child(
                                        secondary_button(SharedString::from(format!("pin-{id}")))
                                            .small()
                                            .label(if pinned { "Unpin" } else { "Pin" })
                                            .disabled(!this.snapshot.connected)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.command(
                                                    Command::Event(ClientEvent::PinMessage {
                                                        message_id: pin_id.clone(),
                                                        pinned: !pinned,
                                                    }),
                                                    cx,
                                                );
                                            })),
                                    )
                                })
                                .when(!mine && moderate, |v| {
                                    v.child(
                                        secondary_button(SharedString::from(format!(
                                            "moderate-delete-{id}"
                                        )))
                                        .small()
                                        .label("Delete")
                                        .on_click(
                                            cx.listener(move |this, _, window, cx| {
                                                this.open_modal(
                                                    Modal::ConfirmDelete(moderated_delete.clone()),
                                                    window,
                                                    cx,
                                                )
                                            }),
                                        ),
                                    )
                                })
                                .when(mine, |v| {
                                    v.child(
                                        secondary_button(SharedString::from(format!("edit-{id}")))
                                            .small()
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
                                        secondary_button(SharedString::from(format!(
                                            "delete-{id}"
                                        )))
                                        .small()
                                        .label("Delete")
                                        .on_click(
                                            cx.listener(move |this, _, window, cx| {
                                                this.open_modal(
                                                    Modal::ConfirmDelete(delete_id.clone()),
                                                    window,
                                                    cx,
                                                )
                                            }),
                                        ),
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
                            .text_color(rgb(colors.error_text))
                            .child(match &cached.status {
                                OutboxStatus::Failed(e) => format!("Not sent: {e}"),
                                _ => String::new(),
                            })
                            .child(
                                secondary_button(SharedString::from(format!("retry-{id}")))
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
