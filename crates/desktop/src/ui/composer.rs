use super::*;
use gpui_component::input::Input;
pub fn render(
    this: &mut Messenger,
    thread: bool,
    _window: &mut Window,
    cx: &mut Context<Messenger>,
) -> impl IntoElement + use<> {
    let input = if thread {
        this.reply.clone()
    } else {
        this.composer.clone()
    };
    div()
        .flex_shrink_0()
        .p_4()
        .border_t_1()
        .border_color(rgb(BORDER))
        .flex()
        .flex_col()
        .gap_2()
        .when(!thread, |view| view.child(Input::new(&this.topic)))
        .child(
            div()
                .id(if thread {
                    "reply-input"
                } else {
                    "message-input"
                })
                .capture_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                    if event.keystroke.key == "enter"
                        && !event.keystroke.modifiers.shift
                        && !event.is_held
                    {
                        cx.stop_propagation();
                        this.submit_message(thread, window, cx);
                    }
                }))
                .child(Input::new(&input)),
        )
        .when(!this.snapshot.attachments.is_empty(), |view| {
            view.child(
                div().flex().gap_2().children(
                    this.snapshot
                        .attachments
                        .iter()
                        .filter_map(|a| this.snapshot.image_previews.get(&a.id))
                        .cloned()
                        .map(|path| {
                            img(path)
                                .size(px(64.))
                                .object_fit(ObjectFit::Cover)
                                .rounded_md()
                        }),
                ),
            )
        })
        .when(!this.snapshot.attachments.is_empty(), |view| {
            view.child(div().flex().flex_wrap().gap_2().children(
                this.snapshot.attachments.clone().into_iter().map(|a| {
                    let id = a.id.clone();
                    Button::new(SharedString::from(format!("attachment-{}", a.id)))
                        .label(format!("{}  ×", a.filename))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.command(Command::RemoveAttachment(id.clone()), cx);
                        }))
                }),
            ))
        })
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    Button::new(if thread {
                        "attach-reply"
                    } else {
                        "attach-message"
                    })
                    .label("Attach file")
                    .on_click(cx.listener(|this, _, _, cx| this.choose_file(cx))),
                )
                .child(
                    Button::new(if thread { "send-reply" } else { "send-message" })
                        .primary()
                        .label(if thread { "Reply" } else { "Send →" })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.submit_message(thread, window, cx)
                        })),
                ),
        )
        .child(muted("Enter ↵ · Shift+Enter for newline"))
        .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
            for path in paths.paths() {
                this.command(Command::Upload(path.clone()), cx);
            }
        }))
}
