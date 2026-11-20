use super::*;
use std::ops::Range;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Token {
    pub range: Range<usize>,
    pub marker: char,
    pub query: String,
}

fn word(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-')
}

/// Byte offsets stay aligned with GPUI's UTF-8 cursor, including emoji before a mention.
fn token(text: &str, cursor: usize) -> Option<Token> {
    let before = text.get(..cursor)?;
    let (start, marker) = before.char_indices().rev().find(|(_, c)| !word(*c))?;
    if !matches!(marker, '@' | '#') {
        return None;
    }
    if before[..start]
        .chars()
        .next_back()
        .is_some_and(|c| !c.is_whitespace() && !matches!(c, '(' | '[' | '{' | '>'))
    {
        return None;
    }
    let end = cursor
        + text[cursor..]
            .chars()
            .take_while(|c| word(*c))
            .map(char::len_utf8)
            .sum::<usize>();
    Some(Token {
        range: start..end,
        marker,
        query: before[start + 1..].to_lowercase(),
    })
}

#[derive(Clone)]
pub(super) struct Suggestion {
    pub text: String,
    detail: String,
    user_id: Option<String>,
}

pub(super) fn suggestions(
    this: &Messenger,
    thread: bool,
    window: &Window,
    cx: &App,
) -> Option<(Token, Vec<Suggestion>)> {
    let ix = usize::from(thread);
    if this.markdown_preview[ix] || this.completion_dismissed[ix] {
        return None;
    }
    let input = if thread { &this.reply } else { &this.composer };
    let input = input.read(cx);
    if !input.focus_handle(cx).is_focused(window) {
        return None;
    }
    let token = token(&input.value(), input.cursor())?;
    let matches = candidates(&this.snapshot, &token);
    (!matches.is_empty()).then_some((token, matches))
}

fn candidates(snapshot: &client_core::Snapshot, token: &Token) -> Vec<Suggestion> {
    let mut matches: Vec<_> = if token.marker == '@' {
        snapshot
            .users
            .iter()
            .filter_map(|u| {
                let handle = u.handle.as_ref()?;
                (handle.to_lowercase().contains(&token.query)
                    || u.display_name.to_lowercase().contains(&token.query))
                .then(|| Suggestion {
                    text: format!("@{handle}"),
                    detail: u.display_name.clone(),
                    user_id: Some(u.id.clone()),
                })
            })
            .collect()
    } else {
        snapshot
            .channels
            .iter()
            .filter(|c| !c.is_direct && !c.is_saved && c.name.to_lowercase().contains(&token.query))
            .map(|c| Suggestion {
                text: format!("#{}", c.name),
                detail: if c.is_private {
                    "Private channel".into()
                } else {
                    c.topic.clone().unwrap_or_else(|| "Channel".into())
                },
                user_id: None,
            })
            .collect()
    };
    matches.sort_by_key(|s| {
        (
            !s.text[1..].to_lowercase().starts_with(&token.query),
            s.text.to_lowercase(),
        )
    });
    matches.dedup_by(|a, b| a.text == b.text);
    matches.truncate(8);
    matches
}

pub(super) fn menu(
    this: &Messenger,
    thread: bool,
    window: &Window,
    cx: &mut Context<Messenger>,
) -> AnyElement {
    let Some((token, matches)) = suggestions(this, thread, window, cx) else {
        return div().into_any_element();
    };
    let colors = palette(cx);
    let selected = this.completion_selection[usize::from(thread)].min(matches.len() - 1);
    let count = matches.len();
    let list = uniform_list(
        SharedString::from(format!("completion-list-{thread}")),
        count,
        cx.processor(move |this, visible: Range<usize>, _, cx| {
            visible
                .map(|ix| {
                    let item = &matches[ix];
                    let text = item.text.clone();
                    let range = token.range.clone();
                    div()
                        .id(SharedString::from(format!("completion-{thread}-{ix}")))
                        .h(px(36.))
                        .px_2()
                        .rounded_md()
                        .flex()
                        .items_center()
                        .gap_2()
                        .cursor_pointer()
                        .when(ix == selected, |v| {
                            v.bg(rgb(colors.selected)).text_color(rgb(colors.accent))
                        })
                        .hover(|v| v.bg(rgb(colors.hover)))
                        .when_some(item.user_id.clone(), |v, id| {
                            v.child(avatar(this, &id, 24., cx))
                        })
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(item.text.clone()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .child(muted(item.detail.clone())),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            insert(this, thread, range.clone(), &text, window, cx)
                        }))
                })
                .collect::<Vec<_>>()
        }),
    )
    .track_scroll(this.completion_scroll[usize::from(thread)].clone())
    .h(px((count.min(4) * 36) as f32));
    div()
        .absolute()
        .bottom(relative(1.))
        .left_0()
        .w_full()
        .mb_2()
        .p_2()
        .rounded_lg()
        .bg(rgb(colors.bg))
        .border_1()
        .border_color(rgb(colors.border))
        .shadow_lg()
        .flex()
        .flex_col()
        .gap_1()
        .child(div().px_2().py_1().child(muted(
            "↑↓ to choose · Enter or Tab to insert · Esc to dismiss",
        )))
        .child(list)
        .into_any_element()
}

fn insert(
    this: &mut Messenger,
    thread: bool,
    range: Range<usize>,
    text: &str,
    window: &mut Window,
    cx: &mut Context<Messenger>,
) {
    let input = if thread {
        this.reply.clone()
    } else {
        this.composer.clone()
    };
    input.update(cx, |input, cx| {
        let value = input.value().to_string();
        if range.end > value.len()
            || !value.is_char_boundary(range.start)
            || !value.is_char_boundary(range.end)
        {
            return;
        }
        let (range_utf16, replacement) = completion_edit(&value, range, text);
        input.replace_text_in_range(Some(range_utf16), &replacement, window, cx);
        input.focus(window, cx);
    });
    this.completion_selection[usize::from(thread)] = 0;
    cx.notify();
}

fn completion_edit(value: &str, range: Range<usize>, text: &str) -> (Range<usize>, String) {
    let range_utf16 =
        value[..range.start].encode_utf16().count()..value[..range.end].encode_utf16().count();
    let separated = value[range.end..].chars().next().is_some_and(|c| {
        c.is_whitespace() || matches!(c, ',' | '.' | '!' | '?' | ';' | ':' | ')' | ']' | '}')
    });
    (
        range_utf16,
        format!("{text}{}", if separated { "" } else { " " }),
    )
}

pub(super) fn key(
    this: &mut Messenger,
    thread: bool,
    event: &KeyDownEvent,
    window: &mut Window,
    cx: &mut Context<Messenger>,
) -> bool {
    if event.keystroke.modifiers.platform
        || event.keystroke.modifiers.control
        || event.keystroke.modifiers.alt
        || event.keystroke.modifiers.shift
    {
        return false;
    }
    let Some((token, matches)) = suggestions(this, thread, window, cx) else {
        return false;
    };
    let ix = usize::from(thread);
    let selected = this.completion_selection[ix].min(matches.len() - 1);
    match event.keystroke.key.as_str() {
        "up" => this.completion_selection[ix] = (selected + matches.len() - 1) % matches.len(),
        "down" => this.completion_selection[ix] = (selected + 1) % matches.len(),
        "enter" | "tab" => {
            if !event.is_held {
                insert(
                    this,
                    thread,
                    token.range,
                    &matches[selected].text,
                    window,
                    cx,
                );
            }
        }
        "escape" => this.completion_dismissed[ix] = true,
        _ => return false,
    }
    this.completion_scroll[ix]
        .scroll_to_item(this.completion_selection[ix], ScrollStrategy::Center);
    cx.stop_propagation();
    cx.notify();
    true
}

#[cfg(test)]
mod tests {
    use super::{Token, token};
    #[test]
    fn finds_mentions_and_channels_without_matching_email_or_markdown_headings() {
        assert_eq!(
            token("hello @i", 8),
            Some(Token {
                range: 6..8,
                marker: '@',
                query: "i".into()
            })
        );
        assert_eq!(token("👋 (@ihor), hi", 9).unwrap().range, 6..11);
        assert_eq!(token("#gen", 4).unwrap().query, "gen");
        assert_eq!(token("@", 1).unwrap().query, "");
        for text in [
            "mail@ihor",
            "## Heading",
            "https://site/#general",
            "hello ",
            "@ihor ",
        ] {
            assert!(token(text, text.len()).is_none(), "{text}");
        }
    }
    #[test]
    fn completes_whole_token_when_cursor_is_in_middle() {
        assert_eq!(token("hi #general!", 6).unwrap().range, 3..11);
        assert!(token("👋 @i", 1).is_none());
    }
    #[test]
    fn replacements_use_utf16_and_preserve_punctuation_and_surrounding_text() {
        let value = "👋 @ihor, hello";
        let token = token(value, 7).unwrap();
        assert_eq!(
            super::completion_edit(value, token.range, "@ivan"),
            (3..8, "@ivan".into())
        );
        assert_eq!(
            super::completion_edit("@i", 0..2, "@ihor"),
            (0..2, "@ihor ".into())
        );
        assert_eq!(
            super::completion_edit("@i rest", 0..2, "@ihor"),
            (0..2, "@ihor".into())
        );
    }

    #[test]
    fn matches_handles_names_and_accessible_nonpersonal_channels() {
        let mut snapshot = client_core::Snapshot::default();
        for (id, handle, name) in [
            ("1", Some("ihor"), "Ihor"),
            ("2", Some("anna"), "Ihor's teammate"),
            ("3", None, "No handle"),
        ] {
            snapshot.users.push(shared_proto::User {
                id: id.into(),
                email: format!("{id}@test.local"),
                display_name: name.into(),
                handle: handle.map(str::to_owned),
                bio: String::new(),
                avatar_url: None,
                created_at: 0,
            });
        }
        let query = token("@I", 2).unwrap();
        let people = super::candidates(&snapshot, &query);
        assert_eq!(
            people.iter().map(|p| p.text.as_str()).collect::<Vec<_>>(),
            ["@ihor", "@anna"]
        );
        for (id, name, private, direct, saved) in [
            ("1", "general", false, false, false),
            ("2", "general-private", true, false, false),
            ("3", "general-dm", true, true, false),
            ("4", "general-saved", true, false, true),
        ] {
            snapshot.channels.push(shared_proto::Channel {
                id: id.into(),
                name: name.into(),
                topic: None,
                is_private: private,
                is_direct: direct,
                is_saved: saved,
                peer_user_id: None,
                created_by: "1".into(),
                created_at: 0,
            });
        }
        let channels = super::candidates(&snapshot, &token("#gen", 4).unwrap());
        assert_eq!(
            channels.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(),
            ["#general", "#general-private"]
        );
        assert!(super::candidates(&snapshot, &token("@zz", 3).unwrap()).is_empty());
    }
}
