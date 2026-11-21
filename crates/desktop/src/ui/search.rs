use super::*;
use gpui_component::input::Input;
pub fn overlay(
    this: &mut Messenger,
    _window: &mut Window,
    cx: &mut Context<Messenger>,
) -> impl IntoElement + use<> {
    let colors = palette(cx);
    let search = this.modal == Modal::Search;
    let title = match &this.modal {
        Modal::Search => "Jump to a conversation",
        Modal::Workspace => "Create a workspace",
        Modal::Channel => "Create a channel",
        Modal::Invite => "Invite a teammate",
        Modal::Edit(_) => "Edit message",
        Modal::Direct => "New direct message",
        Modal::Members => "Private channel members",
        Modal::Settings => "Settings",
        Modal::Reaction(_) => "Add a reaction",
        Modal::Manage => "Workspace & moderation",
        Modal::Pins => "Pinned messages",
        Modal::Unread => "Unread messages",
        Modal::Profile(_) => "Profile",
        Modal::ChannelEdit(_) => "Edit channel",
        Modal::ConfirmDelete(_) => "Delete message?",
        Modal::None => "",
    };
    let query = this.search.read(cx).value().to_string();
    let channel_query = query.trim_start_matches('#').to_lowercase();
    let channels: Vec<_> = this
        .snapshot
        .channels
        .iter()
        .filter(|c| {
            channel_label(c, &this.snapshot.users)
                .to_lowercase()
                .contains(&channel_query)
        })
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
                        .map(|c| channel_label(c, &this.snapshot.users))
                        .unwrap_or_default();
                    div()
                        .id(SharedString::from(format!("result-{}", m.id)))
                        .h(px(68.))
                        .py_2()
                        .px_3()
                        .rounded_md()
                        .cursor_pointer()
                        .hover(|v| v.bg(rgb(colors.hover)))
                        .when(this.search_selection == channel_count + ix, |v| {
                            v.bg(rgb(colors.hover))
                        })
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(muted(label))
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
    let content = div().w(px(620.)).max_h(relative(0.9)).h_auto().p_5().rounded_xl().bg(rgb(colors.panel)).border_1().border_color(rgb(colors.border))
        .flex().flex_col().gap_4().when(matches!(this.modal,Modal::Reaction(_)),|v|v.track_focus(&this.reaction_focus)).on_mouse_down(MouseButton::Left,|_,_,cx|cx.stop_propagation()).capture_key_down(cx.listener(|this,event,window,cx|this.search_key(event,window,cx)))
        .child(div().flex().justify_between()
            .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(title))
            .child(secondary_button("close-modal").label("Esc ×").on_click(cx.listener(|this,_,window,cx| {
                this.modal=Modal::None;this.focus.focus(window);cx.notify();
            }))))
        .when(search,|view| {
            view.child(div().id("search-body").max_h(px(400.)).overflow_y_scroll().flex().flex_col().gap_2().child(Input::new(&this.search)).child(muted("CHANNELS"))
                .children(channels.into_iter().enumerate().map(|(ix,ch)| {
                    let id=ch.id.clone();
                    div().id(SharedString::from(format!("jump-{id}"))).px_3().py_2().rounded_md().cursor_pointer()
                        .hover(|v|v.bg(rgb(colors.hover))).when(this.search_selection==ix,|v|v.bg(rgb(colors.hover))).child(channel_label(&ch,&this.snapshot.users))
                        .on_click(cx.listener(move|this,_,window,cx|this.select_channel(id.clone(),window,cx)))
                }))
                .child(muted(format!("MESSAGES · {count} local results")))
                .when(count>0,|v|v.child(results))
                .when(count==0&&!query.trim().is_empty(),|v|v.child(muted("No matching messages in your offline history."))))
        })
        .when(matches!(this.modal,Modal::ConfirmDelete(_)),|v|v.child(div().flex().flex_col().gap_4().child("This deletes the message text and removes its pin for everyone in this conversation.")
            .child(Button::new("confirm-delete-message").danger().label("Delete message").on_click(cx.listener(|this,_,window,cx|{if let Modal::ConfirmDelete(id)=this.modal.clone()&&this.command(Command::Event(shared_proto::ClientEvent::DeleteMessage{message_id:id}),cx){this.modal=Modal::None;this.focus.focus(window);cx.notify();}})))))
        .when(matches!(this.modal,Modal::Reaction(_)),|v|v.child(super::reactions::render(this,cx)))
        .when(matches!(this.modal,Modal::Direct|Modal::Members),|v|v.child(super::conversations::people(this,cx,this.modal==Modal::Members)))
        .when(!search&&!matches!(this.modal,Modal::Direct|Modal::Members|Modal::Reaction(_)|Modal::ConfirmDelete(_)),|view| {
            view.child(Input::new(&this.name))
                .when(this.modal==Modal::Channel,|v|v.child(div().flex().gap_2()
                    .child(Button::new("channel-public").label("Public").when(!this.private_channel,|b|b.primary()).when(this.private_channel, |b| b.outline()).on_click(cx.listener(|this,_,_,cx|{this.private_channel=false;cx.notify();})))
                    .child(Button::new("channel-private").label("Private · invite only").when(this.private_channel,|b|b.primary()).when(!this.private_channel, |b| b.outline()).on_click(cx.listener(|this,_,_,cx|{this.private_channel=true;cx.notify();})))))
                .when(matches!(this.modal,Modal::ChannelEdit(_)),|v|v.child(Input::new(&this.edit_topic)))
                .when(this.modal==Modal::Workspace,|v|v.child(Input::new(&this.slug)))
                .when(this.modal==Modal::Invite,|v|v.child(muted("Enter the email of a registered user. Workspace owner/admin access is required.")))
                .child(Button::new("confirm-modal").primary()
                    .label(match this.modal {Modal::Workspace=>"Create workspace",Modal::Channel=>"Create channel",Modal::Invite=>"Add teammate",_=>"Save changes"})
                    .on_click(cx.listener(|this,_,window,cx|this.submit_modal(window,cx))))
        });
    div()
        .absolute()
        .inset_0()
        .bg(rgba(colors.overlay))
        .flex()
        .justify_center()
        .items_center()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _, window, cx| {
                this.modal = Modal::None;
                this.focus.focus(window);
                cx.notify();
            }),
        )
        .child(content)
}
