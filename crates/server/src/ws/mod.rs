use crate::{
    App, Result,
    db::{manager::Database, queries},
};
use axum::{
    extract::{
        Path, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::HeaderMap,
    response::Response,
};
use shared_proto::{ClientEvent, ServerEvent};
pub async fn upgrade(
    State(app): State<App>,
    Path(w): Path<String>,
    h: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response> {
    let (u, db) = crate::api::context(&app, &h, &w).await?;
    Ok(ws
        .max_message_size(128 * 1024)
        .on_upgrade(move |socket| run(socket, db, u)))
}
async fn send(socket: &mut WebSocket, e: &ServerEvent) -> bool {
    match serde_json::to_string(e) {
        Ok(s) => socket.send(Message::Text(s.into())).await.is_ok(),
        Err(_) => false,
    }
}
async fn run(mut socket: WebSocket, db: Database, user: String) {
    let mut rx = db.events.subscribe();
    let latest = db
        .read(|c| {
            Ok(
                c.query_row("SELECT COALESCE(MAX(seq),0) FROM events", [], |r| {
                    r.get::<_, i64>(0)
                })?,
            )
        })
        .await;
    let Ok(latest_seq) = latest else { return };
    if !send(
        &mut socket,
        &ServerEvent::Hello {
            user_id: user.clone(),
            latest_seq,
        },
    )
    .await
    {
        return;
    }
    loop {
        tokio::select! {
        input=socket.recv()=>{let event=match input{Some(Ok(Message::Text(raw)))=>serde_json::from_str::<ClientEvent>(&raw),Some(Ok(Message::Close(_)))|None|Some(Err(_))=>break,_=>continue};let client_msg_id=event.as_ref().ok().and_then(|e|if let ClientEvent::SendMessage{client_msg_id,..}=e{Some(client_msg_id.clone())}else{None});let result=match event{Ok(e)=>{let u=user.clone();let hub=db.events.clone();db.write(move|c|{let(e,broadcast)=queries::apply(c,&u,e)?;if broadcast{let _=hub.send(e.clone());}Ok((e,broadcast))}).await},Err(_)=>Err(crate::Error::Bad("invalid websocket event".into()))};match result{Ok((e,false))=>{if !send(&mut socket,&e).await{break}},Ok((_,true))=>{},Err(e)=>{let message=match e {crate::Error::Internal(ref error)=>{tracing::error!(?error,"websocket operation failed");"internal error".into()},_=>e.to_string()};if !send(&mut socket,&ServerEvent::Error{code:"request_failed".into(),message,client_msg_id}).await{break}}}},
        event=rx.recv()=>{match event {Ok(e)=>{let visible=if let Some(ch)=queries::event_channel(&e){let ch=ch.to_owned();let u=user.clone();db.read(move|c|queries::access(c,&ch,&u)).await.is_ok()}else{true};if visible&&!send(&mut socket,&e).await{break}},Err(tokio::sync::broadcast::error::RecvError::Lagged(_))=>{let _=send(&mut socket,&ServerEvent::Error{client_msg_id:None,code:"resync_required".into(),message:"event buffer exceeded; reconnect and sync".into()}).await;break},Err(_)=>break}}
        }
    }
}
