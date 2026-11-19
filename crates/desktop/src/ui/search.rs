use super::*;
use gpui_component::input::Input;
pub fn overlay(
    this: &mut Messenger,
    _window: &mut Window,
    cx: &mut Context<Messenger>,
) -> impl IntoElement + use<> {
    let search = this.modal == Modal::Search;
    let title = match &this.modal {
        Modal::Search => "Jump to a conversation",
        Modal::Workspace => "Create a workspace",
        Modal::Channel => "Create a channel",
        Modal::Invite => "Invite a teammate",
        Modal::Edit(_) => "Edit message",
        Modal::None => "",
    };
    let query = this.search.read(cx).value().to_string();
    let channel_query = query.trim_start_matches('#').to_lowercase();
    let channels: Vec<_> = this
        .snapshot
        .channels
        .iter()
        .filter(|c| c.name.to_lowercase().contains(&channel_query))
        .take(8)
        .cloned()
        .collect();
    let count = this.snapshot.search_results.len();
    let channel_count = channels.len();
    let results = uniform_list(
        "search-results",
        count,
        cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
            range
                .map(|ix| {
                    let m = this.snapshot.search_results[ix].clone();
                    let channel = m.channel_id.clone();
                    let parent = m.parent_id.clone().unwrap_or(m.id.clone());
                    let label = this
                        .snapshot
                        .channels
                        .iter()
                        .find(|c| c.id == channel)
                        .map(|c| c.name.clone())
                        .unwrap_or_default();
                    div()
                        .id(SharedString::from(format!("result-{}", m.id)))
                        .h(px(68.))
                        .py_2()
                        .px_3()
                        .rounded_md()
                        .cursor_pointer()
                        .hover(|v| v.bg(rgb(0x24323e)))
                        .when(this.search_selection == channel_count + ix, |v| {
                            v.bg(rgb(0x24323e))
                        })
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(muted(format!("# {label}")))
                        .child(
                            div()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(m.body_md.replace('\n', " ")),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.select_channel(channel.clone(), window, cx);
                            this.command(Command::SelectThread(Some(parent.clone())), cx);
                        }))
                })
                .collect::<Vec<_>>()
        }),
    )
    .track_scroll(this.search_scroll.clone())
    .h(px((count.min(4) * 68) as f32));
    let content = div().w(px(620.)).max_h(relative(0.9)).h_auto().p_5().rounded_xl().bg(rgb(PANEL)).border_1().border_color(rgb(BORDER))
        .flex().flex_col().gap_4().on_mouse_down(MouseButton::Left,|_,_,cx|cx.stop_propagation()).capture_key_down(cx.listener(|this,event,window,cx|this.search_key(event,window,cx)))
        .child(div().flex().justify_between()
            .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(title))
            .child(Button::new("close-modal").label("Esc ×").on_click(cx.listener(|this,_,window,cx| {
                this.modal=Modal::None;this.focus.focus(window);cx.notify();
            }))))
        .when(search,|view| {
            view.child(div().id("search-body").max_h(px(400.)).overflow_y_scroll().flex().flex_col().gap_2().child(Input::new(&this.search)).child(muted("CHANNELS"))
                .children(channels.into_iter().enumerate().map(|(ix,ch)| {
                    let id=ch.id;
                    div().id(SharedString::from(format!("jump-{id}"))).px_3().py_2().rounded_md().cursor_pointer()
                        .hover(|v|v.bg(rgb(0x24323e))).when(this.search_selection==ix,|v|v.bg(rgb(0x24323e))).child(format!("# {}",ch.name))
                        .on_click(cx.listener(move|this,_,window,cx|this.select_channel(id.clone(),window,cx)))
                }))
                .child(muted(format!("MESSAGES · {count} local results")))
                .when(count>0,|v|v.child(results))
                .when(count==0&&!query.trim().is_empty(),|v|v.child(muted("No matching messages in your offline history."))))
        })
        .when(!search,|view| {
            view.child(Input::new(&this.name))
                .when(this.modal==Modal::Workspace,|v|v.child(Input::new(&this.slug)))
                .when(this.modal==Modal::Invite,|v|v.child(muted("Enter the email of a registered user. Workspace owner/admin access is required.")))
                .child(Button::new("confirm-modal").primary()
                    .label(match this.modal {Modal::Workspace=>"Create workspace",Modal::Channel=>"Create channel",Modal::Invite=>"Add teammate",_=>"Save changes"})
                    .on_click(cx.listener(|this,_,window,cx|this.submit_modal(window,cx))))
        });
    div()
        .absolute()
        .inset_0()
        .bg(rgba(0x080c14dd))
        .flex()
        .justify_center()
        .items_center()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _, _, cx| {
                this.modal = Modal::None;
                cx.notify();
            }),
        )
        .child(content)
}
