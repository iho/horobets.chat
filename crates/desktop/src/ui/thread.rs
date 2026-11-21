use super::*;
pub fn render(
    this: &mut Messenger,
    window: &mut Window,
    cx: &mut Context<Messenger>,
) -> impl IntoElement + use<> {
    let colors = palette(cx);
    let entity = cx.entity();
    div()
        .w_full()
        .flex_shrink_0()
        .h_full()
        .flex()
        .flex_col()
        .border_l_1()
        .border_color(rgb(colors.border))
        .bg(rgb(colors.bg))
        .child(
            div()
                .h(px(CHAT_HEADER_HEIGHT))
                .flex_shrink_0()
                .px_5()
                .flex()
                .items_center()
                .justify_between()
                .border_b_1()
                .border_color(rgb(colors.border))
                .child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Thread"),
                )
                .child(
                    secondary_button("close-thread")
                        .label("×")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.command(Command::SelectThread(None), cx);
                        })),
                ),
        )
        .child(
            list(this.thread_list.clone(), move |ix, window, cx| {
                entity.update(cx, |this, cx| {
                    let Some(message) = this.snapshot.thread_messages.get(ix).cloned() else {
                        return div().into_any_element();
                    };
                    feed::row(this, &message, false, true, window, cx)
                })
            })
            .flex_1()
            .min_h_0(),
        )
        .child(composer::render(this, true, window, cx))
}
