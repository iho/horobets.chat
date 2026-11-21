use super::*;
use gpui_component::input::Input;
pub fn render(
    this: &mut Messenger,
    thread: bool,
    window: &mut Window,
    cx: &mut Context<Messenger>,
) -> impl IntoElement + use<> {
    let colors = palette(cx);
    let input = if thread {
        this.reply.clone()
    } else {
        this.composer.clone()
    };
    let preview = this.markdown_preview[usize::from(thread)];
    let draft = input.read(cx).value().to_string();
    div()
        .relative()
        .flex_shrink_0()
        .mx(px(CHAT_GUTTER))
        .mt_2()
        .mb_3()
        .p_2()
        .rounded_lg()
        .bg(rgb(colors.panel))
        .border_1()
        .border_color(rgb(colors.border))
        .flex()
        .flex_col()
        .gap_2()
        .when(!thread && this.show_topic, |view| {
            view.child(Input::new(&this.topic))
        })
        .when(this.formatting_expanded[usize::from(thread)], |view| {
            view.child(div().flex().flex_wrap().items_center().gap_1().children(
                FORMATS.iter().copied().enumerate().map(|(ix, format)| {
                    secondary_button(SharedString::from(format!("markdown-{thread}-{ix}")))
                        .small()
                        .label(format.label)
                        .tooltip(format.hint)
                        .disabled(preview)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            apply_format(this, thread, format, window, cx);
                        }))
                }),
            ))
        })
        .when(preview, |v| {
            v.child(
                div()
                    .id(if thread {
                        "reply-markdown-preview"
                    } else {
                        "message-markdown-preview"
                    })
                    .min_h(px(80.))
                    .max_h(px(240.))
                    .overflow_y_scroll()
                    .p_3()
                    .rounded_lg()
                    .bg(rgb(colors.bg))
                    .child(if draft.trim().is_empty() {
                        muted("Nothing to preview yet. Write a message first.").into_any_element()
                    } else {
                        super::references::render(
                            this,
                            if thread {
                                "reply-preview-body"
                            } else {
                                "message-preview-body"
                            },
                            &draft,
                            window,
                            cx,
                        )
                    }),
            )
        })
        .child(super::autocomplete::menu(this, thread, window, cx))
        .when(!preview, |v| {
            v.child(
                div()
                    .id(if thread {
                        "reply-input"
                    } else {
                        "message-input"
                    })
                    .capture_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                        if super::autocomplete::key(this, thread, event, window, cx) {
                            return;
                        }
                        if (event.keystroke.modifiers.platform || event.keystroke.modifiers.control)
                            && !event.keystroke.modifiers.alt
                            && let Some(format) = match event.keystroke.key.as_str() {
                                "b" => Some(FORMATS[0]),
                                "i" => Some(FORMATS[1]),
                                "e" => Some(FORMATS[2]),
                                _ => None,
                            }
                        {
                            cx.stop_propagation();
                            apply_format(this, thread, format, window, cx);
                            return;
                        }
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
        })
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
                    secondary_button(SharedString::from(format!("attachment-{}", a.id)))
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
                    div()
                        .flex()
                        .flex_wrap()
                        .flex_1()
                        .min_w_0()
                        .items_center()
                        .gap_1()
                        .child(
                            secondary_button(if thread {
                                "attach-reply"
                            } else {
                                "attach-message"
                            })
                            .small()
                            .label("+ Attach")
                            .on_click(cx.listener(|this, _, _, cx| this.choose_file(cx))),
                        )
                        .child(
                            secondary_button(if thread {
                                "reply-format"
                            } else {
                                "message-format"
                            })
                            .small()
                            .label("Format")
                            .tooltip("Show Markdown formatting tools")
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.formatting_expanded[usize::from(thread)] =
                                        !this.formatting_expanded[usize::from(thread)];
                                    cx.notify();
                                },
                            )),
                        )
                        .when(!thread, |v| {
                            v.child(
                                secondary_button("message-topic")
                                    .small()
                                    .label(if this.topic.read(cx).value().is_empty() {
                                        "Topic"
                                    } else {
                                        "Topic •"
                                    })
                                    .tooltip("Show or hide the optional topic")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.show_topic = !this.show_topic;
                                        cx.notify();
                                    })),
                            )
                        })
                        .child(
                            secondary_button(if thread {
                                "reply-preview"
                            } else {
                                "message-preview"
                            })
                            .small()
                            .label(if preview { "Write" } else { "Preview" })
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.markdown_preview[usize::from(thread)] =
                                        !this.markdown_preview[usize::from(thread)];
                                    if !this.markdown_preview[usize::from(thread)] {
                                        let input = if thread {
                                            this.reply.clone()
                                        } else {
                                            this.composer.clone()
                                        };
                                        input.update(cx, |input, cx| input.focus(window, cx));
                                    }
                                    cx.notify();
                                },
                            )),
                        ),
                )
                .child(
                    Button::new(if thread { "send-reply" } else { "send-message" })
                        .primary()
                        .small()
                        .label(if thread { "Reply" } else { "Send" })
                        .tooltip("Enter to send · Shift+Enter for a new line")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.submit_message(thread, window, cx)
                        })),
                ),
        )
        .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
            for path in paths.paths() {
                this.command(Command::Upload(path.clone()), cx);
            }
        }))
}

#[derive(Clone, Copy)]
struct Format {
    label: &'static str,
    hint: &'static str,
    before: &'static str,
    after: &'static str,
    placeholder: &'static str,
    block: bool,
}
const FORMATS: [Format; 8] = [
    Format {
        label: "B",
        hint: "Bold · ⌘B / Ctrl+B",
        before: "**",
        after: "**",
        placeholder: "bold text",
        block: false,
    },
    Format {
        label: "I",
        hint: "Italic · ⌘I / Ctrl+I",
        before: "*",
        after: "*",
        placeholder: "italic text",
        block: false,
    },
    Format {
        label: "</>",
        hint: "Inline code · ⌘E / Ctrl+E",
        before: "`",
        after: "`",
        placeholder: "code",
        block: false,
    },
    Format {
        label: "Link",
        hint: "Insert a link",
        before: "[",
        after: "](https://example.com)",
        placeholder: "link text",
        block: false,
    },
    Format {
        label: "H",
        hint: "Heading",
        before: "## ",
        after: "\n",
        placeholder: "Heading",
        block: true,
    },
    Format {
        label: "Quote",
        hint: "Quote selected lines",
        before: "> ",
        after: "\n",
        placeholder: "Quote",
        block: true,
    },
    Format {
        label: "List",
        hint: "Bulleted list",
        before: "- ",
        after: "\n",
        placeholder: "List item",
        block: true,
    },
    Format {
        label: "Code",
        hint: "Fenced code block",
        before: "```\n",
        after: "\n```\n",
        placeholder: "code",
        block: true,
    },
];

fn apply_format(
    this: &mut Messenger,
    thread: bool,
    format: Format,
    window: &mut Window,
    cx: &mut Context<Messenger>,
) {
    let input = if thread {
        this.reply.clone()
    } else {
        this.composer.clone()
    };
    input.update(cx, |input, cx| {
        let selection = input
            .selected_text_range(false, window, cx)
            .map(|s| s.range)
            .unwrap_or(0..0);
        let value = input.value().to_string();
        let utf16: Vec<_> = value.encode_utf16().collect();
        let selected = String::from_utf16_lossy(&utf16[selection.clone()]);
        let preceding = String::from_utf16_lossy(&utf16[..selection.start]);
        input.replace(formatted(format, &selected, &preceding), window, cx);
        input.focus(window, cx);
    });
    cx.notify();
}

fn formatted(format: Format, selected: &str, preceding: &str) -> String {
    let body = if selected.is_empty() {
        format.placeholder
    } else {
        selected
    };
    let body = if matches!(format.before, "> " | "- ") {
        body.replace('\n', &format!("\n{}", format.before))
    } else {
        body.to_owned()
    };
    let newline = if format.block && !preceding.is_empty() && !preceding.ends_with('\n') {
        "\n"
    } else {
        ""
    };
    format!("{newline}{}{body}{}", format.before, format.after)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn markdown_formats_preserve_unicode_and_block_boundaries() {
        assert_eq!(formatted(FORMATS[0], "привіт 👋", ""), "**привіт 👋**");
        assert_eq!(formatted(FORMATS[1], "", ""), "*italic text*");
        assert_eq!(
            formatted(FORMATS[5], "one\ntwo", "intro"),
            "\n> one\n> two\n"
        );
        assert_eq!(
            formatted(FORMATS[6], "one\ntwo", "intro\n"),
            "- one\n- two\n"
        );
        assert_eq!(
            formatted(FORMATS[7], "let x = 1;", "intro"),
            "\n```\nlet x = 1;\n```\n"
        );
    }
}
