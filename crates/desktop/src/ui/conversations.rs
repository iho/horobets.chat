use super::*;
pub fn people(
    this: &mut Messenger,
    cx: &mut Context<Messenger>,
    members: bool,
) -> impl IntoElement + use<> {
    let colors = palette(cx);
    let query = this.name.read(cx).value().to_lowercase();
    let channel = this
        .snapshot
        .channels
        .iter()
        .find(|ch| Some(&ch.id) == this.snapshot.channel.as_ref());
    let creator = channel.map(|ch| ch.created_by.clone()).unwrap_or_default();
    let can_manage =
        this.snapshot.user_id.as_ref() == Some(&creator) || this.snapshot.can_moderate();
    let users: Vec<_> = this
        .snapshot
        .users
        .iter()
        .filter(|u| {
            (members || Some(&u.id) != this.snapshot.user_id.as_ref())
                && (u.display_name.to_lowercase().contains(&query)
                    || u.handle
                        .as_ref()
                        .is_some_and(|h| format!("@{h}").contains(&query)))
        })
        .cloned()
        .collect();
    let empty = users.is_empty();
    div()
        .flex()
        .flex_col()
        .gap_3()
        .child(Input::new(&this.name))
        .when(empty,|v|v.child(muted("No matching teammates. Invite registered accounts to this workspace using Invite a teammate.")))
        .when(members, |v| {
            v.child(muted(if can_manage {
                "Only invited members can see this channel. Add or remove teammates below."
            } else {
                "These are the invited channel members. The creator manages access."
            }))
        })
        .child(
            div()
                .id("conversation-people")
                .max_h(px(360.))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap_2()
                .children(
                    users
                        .into_iter()
                        .filter(|u| {
                            !members || can_manage || this.snapshot.channel_members.contains(&u.id)
                        })
                        .map(|u| {
                            let id = u.id.clone();
                            let joined = this.snapshot.channel_members.contains(&u.id);
                            let label = u
                                .handle
                                .as_ref()
                                .map(|h| format!("{} · @{h}", u.display_name))
                                .unwrap_or_else(|| u.display_name.clone());
                            div()
                                .flex()
                                .items_center()
                                .gap_3()
                                .p_2()
                                .rounded_md()
                                .bg(rgb(colors.bg))
                                .child(avatar(this, &u.id, 32., cx))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .child(label),
                                )
                                .child(if members {
                                    secondary_button(SharedString::from(format!("member-{}", u.id)))
                                        .label(if joined { "Remove" } else { "Add" })
                                        .disabled(!can_manage || u.id == creator)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.command(
                                                Command::ChannelMember {
                                                    user_id: id.clone(),
                                                    add: !joined,
                                                },
                                                cx,
                                            );
                                        }))
                                        .into_any_element()
                                } else {
                                    secondary_button(SharedString::from(format!("dm-{}", u.id)))
                                        .label("Message")
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.open_direct(id.clone(), window, cx)
                                        }))
                                        .into_any_element()
                                })
                        }),
                ),
        )
}
