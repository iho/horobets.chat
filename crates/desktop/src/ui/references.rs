use super::*;
use markdown::mdast::Node;
use std::{ops::Range, rc::Rc};

const PREFIX: &str = "horobets://workspace/";

#[derive(Clone, Debug)]
struct Reference {
    label: String,
    url: String,
}

fn catalog(snapshot: &client_core::Snapshot) -> Vec<Reference> {
    let Some(workspace) = &snapshot.workspace else {
        return vec![];
    };
    let mut references: Vec<_> = snapshot
        .users
        .iter()
        .filter_map(|u| {
            u.handle.as_ref().map(|h| Reference {
                label: format!("@{h}"),
                url: format!("{PREFIX}{workspace}/user/{}", u.id),
            })
        })
        .chain(
            snapshot
                .channels
                .iter()
                .filter(|c| !c.is_direct && !c.is_saved)
                .map(|c| Reference {
                    label: format!("#{}", c.name),
                    url: format!("{PREFIX}{workspace}/channel/{}", c.id),
                }),
        )
        .collect();
    references.sort_by_key(|r| std::cmp::Reverse(r.label.len()));
    references
}

fn word(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-')
}

fn scan(
    source: &str,
    original: &str,
    offset: usize,
    references: &[Reference],
    edits: &mut Vec<(Range<usize>, String)>,
) {
    let mut consumed = 0;
    for (start, marker) in source.char_indices() {
        if start < consumed || !matches!(marker, '@' | '#') {
            continue;
        }
        let preceding = &source[..start];
        if original[..offset + start]
            .chars()
            .rev()
            .take_while(|c| *c == '\\')
            .count()
            % 2
            != 0
        {
            continue;
        }
        if preceding
            .chars()
            .next_back()
            .is_some_and(|c| !c.is_whitespace() && !matches!(c, '(' | '[' | '{' | '>'))
        {
            continue;
        }
        for reference in references {
            let end = start + reference.label.len();
            if source
                .get(start..end)
                .is_none_or(|s| !s.eq_ignore_ascii_case(&reference.label))
            {
                continue;
            }
            if source[end..].chars().next().is_some_and(word) {
                continue;
            }
            let label = source[start..end]
                .replace('\\', "\\\\")
                .replace('[', "\\[")
                .replace(']', "\\]");
            edits.push((
                offset + start..offset + end,
                format!("[{label}]({})", reference.url),
            ));
            consumed = end;
            break;
        }
    }
}

fn collect(
    node: &Node,
    source: &str,
    references: &[Reference],
    edits: &mut Vec<(Range<usize>, String)>,
) {
    match node {
        // Existing Markdown links, images and code must keep their original meaning.
        Node::Link(_)
        | Node::LinkReference(_)
        | Node::Image(_)
        | Node::ImageReference(_)
        | Node::Code(_)
        | Node::InlineCode(_)
        | Node::Html(_) => return,
        Node::Text(_) => {
            if let Some(position) = node.position() {
                let range = position.start.offset..position.end.offset;
                if let Some(text) = source.get(range.clone()) {
                    scan(text, source, range.start, references, edits);
                }
            }
        }
        _ => {}
    }
    if let Some(children) = node.children() {
        for child in children {
            collect(child, source, references, edits);
        }
    }
}

fn linked_markdown(source: &str, references: &[Reference]) -> String {
    if references.is_empty() || !source.contains(['@', '#']) {
        return source.to_owned();
    }
    let Ok(root) = markdown::to_mdast(source, &markdown::ParseOptions::gfm()) else {
        return source.to_owned();
    };
    let mut edits = vec![];
    collect(&root, source, references, &mut edits);
    let mut result = source.to_owned();
    for (range, replacement) in edits.into_iter().rev() {
        result.replace_range(range, &replacement);
    }
    result
}

pub fn render(
    this: &Messenger,
    id: impl Into<ElementId>,
    body: &str,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    gpui_component::text::TextView::markdown(
        id,
        linked_markdown(body, &catalog(&this.snapshot)),
        window,
        cx,
    )
    .selectable(true)
    .into_any_element()
}

pub fn install(view: &Entity<Messenger>, cx: &mut App) {
    let view = view.downgrade();
    cx.set_global(gpui_component::text::LinkHandler(Rc::new(
        move |url, window, cx| {
            let Some(path) = url.strip_prefix(PREFIX) else {
                // Unknown internal destinations must never launch an external application.
                return url.starts_with("horobets:");
            };
            let parts: Vec<_> = path.split('/').collect();
            let [workspace, kind, id] = parts.as_slice() else {
                return true;
            };
            let _ = view.update(cx, |this, cx| {
                if this.snapshot.workspace.as_deref() != Some(*workspace) {
                    this.local_error = Some(
                        "This reference belongs to another workspace. Open that workspace first."
                            .into(),
                    );
                } else if *kind == "user" && this.snapshot.users.iter().any(|u| u.id == *id) {
                    this.open_modal(Modal::Profile((*id).into()), window, cx);
                } else if *kind == "channel"
                    && this
                        .snapshot
                        .channels
                        .iter()
                        .any(|c| c.id == *id && !c.is_direct && !c.is_saved)
                {
                    this.select_channel((*id).into(), window, cx);
                } else {
                    this.local_error = Some(
                        "This user or channel is no longer available in your workspace.".into(),
                    );
                }
                cx.notify();
            });
            true
        },
    )));
}

#[cfg(test)]
mod tests {
    use super::*;
    fn refs() -> Vec<Reference> {
        vec![
            Reference {
                label: "@ihor".into(),
                url: format!("{PREFIX}w/user/u"),
            },
            Reference {
                label: "#general".into(),
                url: format!("{PREFIX}w/channel/c"),
            },
        ]
    }
    #[::core::prelude::v1::test]
    fn links_known_references_in_formatted_unicode_text() {
        let result = linked_markdown("👋 **@Ihor**, see #general!", &refs());
        assert_eq!(
            result,
            format!("👋 **[@Ihor]({PREFIX}w/user/u)**, see [#general]({PREFIX}w/channel/c)!")
        );
    }
    #[::core::prelude::v1::test]
    fn leaves_code_existing_links_email_escapes_and_unknown_names_unchanged() {
        for text in [
            "`@ihor #general`",
            "```rust\n@ihor #general\n```",
            "[@ihor](https://example.com/#general)",
            "[profile]: https://example.com/@ihor\n\n[@ihor][profile]",
            "name@ihor.com",
            "https://example.com/#general",
            "\\@ihor",
            "@ihor_other #general-other",
            "@unknown #unknown",
            "## Heading",
        ] {
            assert_eq!(linked_markdown(text, &refs()), text, "{text}");
        }
    }
    #[::core::prelude::v1::test]
    fn links_nested_lists_quotes_and_channel_names_with_spaces() {
        let mut refs = refs();
        refs.insert(
            0,
            Reference {
                label: "#general chat".into(),
                url: format!("{PREFIX}w/channel/long"),
            },
        );
        let text = linked_markdown("> @ihor\n\n- #general chat\n- #general", &refs);
        assert!(text.contains(&format!("> [@ihor]({PREFIX}w/user/u)")));
        assert!(text.contains(&format!("- [#general chat]({PREFIX}w/channel/long)")));
        assert!(text.contains(&format!("- [#general]({PREFIX}w/channel/c)")));
    }
}
