use super::*;
use shared_proto::ClientEvent;
fn date(value: i64) -> String {
    chrono::DateTime::from_timestamp_millis(value)
        .map(|t| t.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}
fn person(this: &Messenger, id: &str) -> String {
    this.snapshot
        .users
        .iter()
        .find(|u| u.id == id)
        .map(|u| u.display_name.clone())
        .unwrap_or_else(|| id.chars().take(8).collect())
}
pub fn render(
    this: &mut Messenger,
    window: &mut Window,
    cx: &mut Context<Messenger>,
) -> impl IntoElement + use<> {
    let colors = palette(cx);
    let title = match this.modal {
        Modal::Manage => "Workspace & moderation",
        Modal::Pins => "Pinned messages",
        _ => "User profile",
    };
    let body = match this.modal.clone() {
        Modal::Manage => workspace(this, cx).into_any_element(),
        Modal::Pins => pins(this, window, cx).into_any_element(),
        Modal::Profile(id) => profile(this, &id, cx).into_any_element(),
        _ => div().into_any_element(),
    };
    let header = div()
        .h(px(68.))
        .flex_shrink_0()
        .px_6()
        .flex()
        .items_center()
        .gap_4()
        .border_b_1()
        .border_color(rgb(colors.border))
        .child(
            secondary_button("management-back")
                .label("← Back to conversations")
                .on_click(cx.listener(|this, _, window, cx| {
                    this.modal = Modal::None;
                    this.focus.focus(window);
                    cx.notify();
                })),
        )
        .child(
            div()
                .text_lg()
                .font_weight(FontWeight::SEMIBOLD)
                .child(title),
        );
    div()
        .size_full()
        .flex()
        .flex_col()
        .bg(rgb(colors.bg))
        .child(header)
        .when(
            this.snapshot.error.is_some() || this.local_error.is_some(),
            |v| {
                v.child(
                    div()
                        .px_6()
                        .py_3()
                        .bg(rgb(colors.error_bg))
                        .text_color(rgb(colors.error_text))
                        .child(
                            this.local_error
                                .clone()
                                .or(this.snapshot.error.clone())
                                .unwrap_or_default(),
                        ),
                )
            },
        )
        .when(this.snapshot.notice.is_some(), |v| {
            v.child(
                div()
                    .px_6()
                    .py_2()
                    .child(muted(this.snapshot.notice.clone().unwrap_or_default())),
            )
        })
        .child(
            div()
                .id("management-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .p_6()
                .child(div().max_w(px(900.)).child(body)),
        )
}
fn workspace(this: &mut Messenger, cx: &mut Context<Messenger>) -> impl IntoElement + use<> {
    let colors = palette(cx);
    let can_manage = this.snapshot.can_manage_workspace();
    let form = div()
        .p_5()
        .rounded_lg()
        .bg(rgb(colors.panel))
        .border_1()
        .border_color(rgb(colors.border))
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div()
                .text_xl()
                .font_weight(FontWeight::SEMIBOLD)
                .child("Workspace details"),
        )
        .child(muted(format!("Your role: {}", this.snapshot.role())))
        .child(muted("NAME"))
        .child(Input::new(&this.manage_name).disabled(!can_manage))
        .child(muted("SLUG"))
        .child(Input::new(&this.manage_slug).disabled(!can_manage))
        .child(
            Button::new("save-workspace")
                .primary()
                .label("Save workspace")
                .disabled(!can_manage)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.command(
                        Command::EditWorkspace {
                            name: this.manage_name.read(cx).value().to_string(),
                            slug: this.manage_slug.read(cx).value().to_string(),
                        },
                        cx,
                    );
                })),
        )
        .when(!can_manage, |v| {
            v.child(muted(
                "Workspace owners and administrators can edit these details.",
            ))
        });
    let channels = div()
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div()
                .text_xl()
                .font_weight(FontWeight::SEMIBOLD)
                .child("Channels"),
        )
        .children(
            this.snapshot
                .channels
                .clone()
                .into_iter()
                .filter(|ch| !ch.is_direct && !ch.is_saved)
                .map(|ch| {
                    let id = ch.id.clone();
                    let can_edit = this.snapshot.can_moderate()
                        || Some(&ch.created_by) == this.snapshot.user_id.as_ref();
                    div()
                        .p_3()
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(colors.border))
                        .flex()
                        .items_center()
                        .gap_4()
                        .child(
                            div()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(channel_label(&ch, &this.snapshot.users))
                                .child(muted(ch.topic.clone().unwrap_or_default())),
                        )
                        .child(
                            secondary_button(SharedString::from(format!(
                                "manage-channel-{}",
                                ch.id
                            )))
                            .label("Edit name / topic")
                            .disabled(!can_edit)
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.open_modal(Modal::ChannelEdit(id.clone()), window, cx)
                                },
                            )),
                        )
                }),
        );
    let people = div()
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div()
                .text_xl()
                .font_weight(FontWeight::SEMIBOLD)
                .child("People & roles"),
        )
        .child(muted(
            "Owners and administrators can assign or remove the moderator role.",
        ))
        .children(this.snapshot.people.clone().into_iter().map(|member| {
            let id = member.user.id.clone();
            let target = id.clone();
            let moderator = member.role == "moderator";
            let can_change = can_manage && matches!(member.role.as_str(), "member" | "moderator");
            div()
                .p_3()
                .rounded_md()
                .border_1()
                .border_color(rgb(colors.border))
                .flex()
                .items_center()
                .gap_3()
                .child(avatar(this, &member.user.id, 36., cx))
                .child(
                    div()
                        .flex_1()
                        .child(member.user.display_name)
                        .child(muted(member.role)),
                )
                .child(
                    secondary_button(SharedString::from(format!("view-profile-{id}")))
                        .label("Profile")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_modal(Modal::Profile(id.clone()), window, cx)
                        })),
                )
                .child(
                    secondary_button(SharedString::from(format!("role-{target}")))
                        .label(if moderator {
                            "Remove moderator"
                        } else {
                            "Make moderator"
                        })
                        .disabled(!can_change)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.command(
                                Command::SetRole {
                                    user_id: target.clone(),
                                    role: if moderator { "member" } else { "moderator" }.into(),
                                },
                                cx,
                            );
                        })),
                )
        }));
    let activity = div()
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div()
                .text_xl()
                .font_weight(FontWeight::SEMIBOLD)
                .child("Moderation activity"),
        )
        .child(muted(
            "Recent channel pins, unpins, and deletions. Private conversations are excluded.",
        ))
        .when(this.snapshot.moderation.is_empty(), |v| {
            v.child(muted("No moderation activity yet."))
        })
        .children(this.snapshot.moderation.iter().map(|action| {
            let channel = this
                .snapshot
                .channels
                .iter()
                .find(|c| c.id == action.channel_id)
                .map(|c| channel_label(c, &this.snapshot.users))
                .unwrap_or_default();
            div()
                .py_2()
                .border_b_1()
                .border_color(rgb(colors.border))
                .child(format!(
                    "{} · {} · {} · {}",
                    date(action.created_at),
                    person(this, &action.actor_id),
                    action.action.replace('_', " "),
                    channel
                ))
        }));
    div()
        .flex()
        .flex_col()
        .gap_6()
        .child(form)
        .child(channels)
        .child(people)
        .when(this.snapshot.can_moderate(), |v| v.child(activity))
}
fn pins(
    this: &mut Messenger,
    window: &mut Window,
    cx: &mut Context<Messenger>,
) -> impl IntoElement + use<> {
    let colors = palette(cx);
    div()
        .flex()
        .flex_col()
        .gap_4()
        .child(muted("Pinned messages from conversations you can access."))
        .when(this.snapshot.pins.is_empty(), |v| {
            v.child(div().py_8().child("No pinned messages yet."))
        })
        .children(this.snapshot.pins.clone().into_iter().map(|pin| {
            let id = pin.message.id.clone();
            let remove = id.clone();
            let open = pin.message.clone();
            let channel = this
                .snapshot
                .channels
                .iter()
                .find(|c| c.id == pin.message.channel_id);
            let personal = channel.is_some_and(|c| c.is_direct || c.is_saved);
            let can_pin = personal || this.snapshot.can_moderate();
            let can_delete = Some(&pin.message.user_id) == this.snapshot.user_id.as_ref()
                || (!personal && this.snapshot.can_moderate());
            div()
                .p_5()
                .rounded_lg()
                .bg(rgb(colors.panel))
                .border_1()
                .border_color(rgb(colors.border))
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div().font_weight(FontWeight::SEMIBOLD).child(
                        channel
                            .map(|c| channel_label(c, &this.snapshot.users))
                            .unwrap_or_default(),
                    ),
                )
                .child(muted(format!(
                    "Pinned by {} · {}",
                    person(this, &pin.pinned_by),
                    date(pin.created_at)
                )))
                .child(super::references::render(
                    this,
                    SharedString::from(format!("pin-body-{id}")),
                    &pin.message.body_md,
                    window,
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            secondary_button(SharedString::from(format!("open-pin-{id}")))
                                .label("Open message / files")
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.select_channel(open.channel_id.clone(), window, cx);
                                    this.command(
                                        Command::SelectThread(Some(
                                            open.parent_id
                                                .clone()
                                                .unwrap_or_else(|| open.id.clone()),
                                        )),
                                        cx,
                                    );
                                })),
                        )
                        .when(can_pin, |v| {
                            v.child(
                                secondary_button(SharedString::from(format!("unpin-{id}")))
                                    .label("Unpin")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.command(
                                            Command::Event(ClientEvent::PinMessage {
                                                message_id: id.clone(),
                                                pinned: false,
                                            }),
                                            cx,
                                        );
                                    })),
                            )
                        })
                        .when(can_delete, |v| {
                            v.child(
                                secondary_button(SharedString::from(format!(
                                    "delete-pin-{remove}"
                                )))
                                .label("Delete")
                                .on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        this.open_modal(
                                            Modal::ConfirmDelete(remove.clone()),
                                            window,
                                            cx,
                                        )
                                    },
                                )),
                            )
                        }),
                )
        }))
}
fn profile(
    this: &mut Messenger,
    id: &str,
    cx: &mut Context<Messenger>,
) -> impl IntoElement + use<> {
    let member = this
        .snapshot
        .people
        .iter()
        .find(|m| m.user.id == id)
        .cloned();
    let Some(member) = member else {
        return div()
            .child("This profile is unavailable in the current workspace.")
            .into_any_element();
    };
    let own = this.snapshot.user_id.as_deref() == Some(id);
    let target = id.to_owned();
    let colors = palette(cx);
    div()
        .p_6()
        .rounded_lg()
        .border_1()
        .border_color(rgb(colors.border))
        .bg(rgb(colors.panel))
        .flex()
        .flex_col()
        .gap_4()
        .child(avatar(this, id, 96., cx))
        .child(
            div()
                .text_2xl()
                .font_weight(FontWeight::SEMIBOLD)
                .child(member.user.display_name),
        )
        .when_some(member.user.handle, |v, h| v.child(muted(format!("@{h}"))))
        .child(member.user.bio)
        .child(muted(member.user.email))
        .child(muted(format!("Workspace role: {}", member.role)))
        .child(muted(format!(
            "Joined workspace: {} · Account created: {}",
            date(member.joined_at),
            date(member.user.created_at)
        )))
        .child(
            Button::new("profile-send-message")
                .primary()
                .label(if own {
                    "Open saved messages"
                } else {
                    "Send private message"
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_direct(target.clone(), window, cx)
                })),
        )
        .when(own, |v| {
            v.child(
                secondary_button("edit-own-profile")
                    .label("Edit your profile")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_modal(Modal::Settings, window, cx)
                    })),
            )
        })
        .into_any_element()
}
