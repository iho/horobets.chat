use super::*;

pub fn render(
    this: &mut Messenger,
    _window: &mut Window,
    cx: &mut Context<Messenger>,
) -> impl IntoElement + use<> {
    let colors = palette(cx);
    let user = this.snapshot.user_id.clone().unwrap_or_default();
    let card = || {
        div()
            .p_6()
            .rounded_xl()
            .bg(rgb(colors.bg))
            .border_1()
            .border_color(rgb(colors.border))
            .flex()
            .flex_col()
            .gap_4()
    };
    let notification_enabled = this.pane_sizes.notifications_enabled;
    let notifications=card()
        .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child("Notifications"))
        .child(muted(this.notification_status.clone()))
        .child(secondary_button("enable-notifications").label(if notification_enabled {"Check permission"}else{"Enable notifications"})
            .on_click(cx.listener(|this,_,_,cx|this.enable_notifications(cx))))
        .when(notification_enabled,|v|v.child(secondary_button("disable-notifications").label("Turn off notifications")
            .on_click(cx.listener(|this,_,_,cx|{this.pane_sizes.notifications_enabled=false;this.notification_status="Notifications are off".into();this.save_pane_sizes(cx);}))))
        .child(muted("Desktop alerts arrive while the app is running. Open the packaged app to use macOS Notification Center."));
    let profile = card()
        .child(
            div()
                .text_xl()
                .font_weight(FontWeight::SEMIBOLD)
                .child("Your profile"),
        )
        .child(muted("How you appear to people in your workspaces."))
        .child(
            div()
                .flex()
                .items_center()
                .gap_4()
                .child(avatar(this, &user, 64., cx))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            secondary_button("settings-photo")
                                .label("Change picture")
                                .on_click(cx.listener(|this, _, _, cx| this.choose_avatar(cx))),
                        )
                        .child(muted(
                            this.snapshot
                                .me
                                .as_ref()
                                .map(|u| u.email.clone())
                                .unwrap_or_default(),
                        )),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(muted("Display name"))
                .child(Input::new(&this.profile_name)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(muted("Handle"))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child("@")
                        .child(Input::new(&this.profile_handle)),
                )
                .child(muted(
                    "3–32 letters, numbers, or underscores. Start with a letter.",
                )),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(muted("About you"))
                .child(Input::new(&this.profile_bio)),
        )
        .child(
            div().flex().child(
                Button::new("save-profile")
                    .primary()
                    .label("Save changes")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.command(
                            Command::SaveProfile {
                                display_name: this.profile_name.read(cx).value().to_string(),
                                handle: this.profile_handle.read(cx).value().to_string(),
                                bio: Some(this.profile_bio.read(cx).value().to_string()),
                            },
                            cx,
                        );
                    })),
            ),
        );
    let appearance =
        card()
            .child(
                div()
                    .text_xl()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Appearance"),
            )
            .child(muted(
                "Make yourself at home. Your choice is saved on this device.",
            ))
            .child(
                div()
                    .flex()
                    .gap_3()
                    .child(
                        Button::new("theme-dark")
                            .label("◐  Dark")
                            .when(this.pane_sizes.dark_mode, |b| b.primary())
                            .when(!this.pane_sizes.dark_mode, |b| b.outline())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_dark_mode(true, window, cx)
                            })),
                    )
                    .child(
                        Button::new("theme-light")
                            .label("☀  Light")
                            .when(!this.pane_sizes.dark_mode, |b| b.primary())
                            .when(this.pane_sizes.dark_mode, |b| b.outline())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_dark_mode(false, window, cx)
                            })),
                    ),
            );
    div()
        .size_full()
        .flex()
        .flex_col()
        .bg(rgb(colors.panel))
        .child(
            div()
                .h(px(CHAT_HEADER_HEIGHT))
                .flex_shrink_0()
                .px_6()
                .flex()
                .items_center()
                .gap_4()
                .bg(rgb(colors.bg))
                .border_b_1()
                .border_color(rgb(colors.border))
                .child(
                    secondary_button("settings-back")
                        .label("← Conversations")
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
                        .child("Settings"),
                ),
        )
        .child(
            div()
                .id("settings-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .p_8()
                .flex()
                .flex_col()
                .items_center()
                .child(
                    div()
                        .w_full()
                        .max_w(px(680.))
                        .flex()
                        .flex_col()
                        .gap_5()
                        .when(
                            this.snapshot.error.is_some() || this.local_error.is_some(),
                            |v| {
                                v.child(
                                    div()
                                        .p_3()
                                        .rounded_lg()
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
                                    .p_3()
                                    .rounded_lg()
                                    .bg(rgb(colors.notice))
                                    .child(this.snapshot.notice.clone().unwrap_or_default()),
                            )
                        })
                        .child(notifications)
                        .child(profile)
                        .child(appearance),
                ),
        )
}
