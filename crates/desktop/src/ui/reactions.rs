use super::*;
pub const CHOICES: [(&str, &str); 32] = [
    ("👍", "Thumbs up"),
    ("👎", "Thumbs down"),
    ("❤️", "Love"),
    ("😂", "Laugh"),
    ("😮", "Surprised"),
    ("😢", "Sad"),
    ("😡", "Angry"),
    ("🎉", "Celebrate"),
    ("🚀", "Rocket"),
    ("👀", "Eyes"),
    ("✅", "Done"),
    ("🙏", "Thanks"),
    ("🔥", "Fire"),
    ("💯", "Hundred"),
    ("🤔", "Thinking"),
    ("👏", "Applause"),
    ("💪", "Strong"),
    ("🙌", "Hooray"),
    ("🤝", "Handshake"),
    ("🫡", "Salute"),
    ("😄", "Happy"),
    ("😍", "Heart eyes"),
    ("😎", "Cool"),
    ("🤯", "Mind blown"),
    ("🧠", "Brain"),
    ("💡", "Idea"),
    ("☕", "Coffee"),
    ("🐛", "Bug"),
    ("🛠️", "Tools"),
    ("⚠️", "Warning"),
    ("❌", "No"),
    ("✨", "Sparkles"),
];
pub fn render(this: &mut Messenger, cx: &mut Context<Messenger>) -> impl IntoElement + use<> {
    let colors = palette(cx);
    let id = match &this.modal {
        Modal::Reaction(id) => id.clone(),
        _ => String::new(),
    };
    let reactions = this
        .snapshot
        .messages
        .iter()
        .chain(this.snapshot.thread_messages.iter())
        .find(|m| m.message.id == id)
        .map(|m| m.reactions.clone())
        .unwrap_or_default();
    div()
        .flex()
        .flex_col()
        .gap_4()
        .child(muted("Choose a reaction. Select it again to remove yours."))
        .when(!this.snapshot.connected, |v| {
            v.child(muted("Reconnect to add a reaction."))
        })
        .child(div().w(px(528.)).flex().flex_wrap().gap_2().children(
            CHOICES.into_iter().enumerate().map(|(ix, (emoji, name))| {
                let id = id.clone();
                let mine = reactions.iter().any(|r| r.emoji == emoji && r.mine);
                Button::new(SharedString::from(format!("choose-reaction-{ix}")))
                    .label(emoji)
                    .tooltip(name)
                    .w(px(59.))
                    .h(px(48.))
                    .text_xl()
                    .when(mine, |b| b.primary())
                    .when(!mine, |b| b.outline())
                    .when(this.reaction_selection == ix, |b| {
                        b.border_2().border_color(rgb(colors.accent))
                    })
                    .disabled(!this.snapshot.connected)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.toggle_reaction(id.clone(), emoji.into(), window, cx)
                    }))
            }),
        ))
        .child(muted("← ↑ ↓ → to choose · Enter to react · Esc to close"))
}
