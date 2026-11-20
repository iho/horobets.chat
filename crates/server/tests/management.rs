use futures_util::{SinkExt, StreamExt};
use reqwest::Client;
use serde_json::{Value, json};
use shared_proto::{ClientEvent, ServerEvent};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};
type Socket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;
async fn account(c: &Client, base: &str, email: &str) -> Value {
    c.post(format!("{base}/api/v1/auth/register"))
        .json(&json!({"email":email,"password":"correct horse battery staple"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap()
}
async fn post(c: &Client, url: &str, t: &str, body: Value) -> Value {
    c.post(url)
        .bearer_auth(t)
        .json(&body)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap()
}
async fn event(ws: &mut Socket, e: ClientEvent) {
    ws.send(Message::Text(serde_json::to_string(&e).unwrap().into()))
        .await
        .unwrap();
}
async fn receive(ws: &mut Socket, predicate: impl Fn(&ServerEvent) -> bool) -> ServerEvent {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let Message::Text(t) = ws.next().await.unwrap().unwrap() {
                let e: ServerEvent = serde_json::from_str(&t).unwrap();
                if predicate(&e) {
                    return e;
                }
            }
        }
    })
    .await
    .unwrap()
}
async fn socket(url: &str, t: &str) -> Socket {
    let mut r = url.into_client_request().unwrap();
    r.headers_mut()
        .insert("authorization", format!("Bearer {t}").parse().unwrap());
    let (mut ws, _) = connect_async(r).await.unwrap();
    receive(&mut ws, |e| matches!(e, ServerEvent::Hello { .. })).await;
    ws
}
async fn send(ws: &mut Socket, id: &str, ch: &str, body: &str) {
    event(
        ws,
        ClientEvent::SendMessage {
            client_msg_id: id.into(),
            channel_id: ch.into(),
            parent_id: None,
            topic_tag: None,
            body_md: body.into(),
            attachment_ids: vec![],
        },
    )
    .await;
    receive(
        ws,
        |e| matches!(e,ServerEvent::MessageCreated{message,..} if message.id==id),
    )
    .await;
}
#[tokio::test]
async fn role_changes_are_enforced_on_live_sockets_and_personal_messages_stay_private() {
    let temp = tempfile::tempdir().unwrap();
    let app = messenger_server::App::open(
        temp.path().to_owned(),
        "a-test-secret-with-at-least-thirty-two-bytes",
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base = format!("http://{addr}");
    let task = tokio::spawn(async move {
        axum::serve(listener, messenger_server::api::router(app))
            .await
            .unwrap()
    });
    let c = Client::new();
    let a = account(&c, &base, "alice@example.com").await;
    let b = account(&c, &base, "bob@example.com").await;
    let outsider = account(&c, &base, "outside@example.com").await;
    let at = a["token"].as_str().unwrap();
    let bt = b["token"].as_str().unwrap();
    let ot = outsider["token"].as_str().unwrap();
    let aid = a["user_id"].as_str().unwrap();
    let bid = b["user_id"].as_str().unwrap();
    let workspace = post(
        &c,
        &format!("{base}/api/v1/workspaces"),
        at,
        json!({"slug":"original","name":"Original"}),
    )
    .await;
    let wid = workspace["id"].as_str().unwrap();
    let root = format!("{base}/api/v1/workspaces/{wid}");
    post(
        &c,
        &format!("{root}/members"),
        at,
        json!({"email":"bob@example.com"}),
    )
    .await;
    let ch = post(
        &c,
        &format!("{root}/channels"),
        at,
        json!({"name":"general"}),
    )
    .await;
    let cid = ch["id"].as_str().unwrap();
    let url = format!("ws://{addr}/api/v1/workspaces/{wid}/ws");
    let mut aws = socket(&url, at).await;
    let mut bws = socket(&url, bt).await;
    send(&mut aws, "owner-message", cid, "original body").await;
    for denied in [
        ClientEvent::PinMessage {
            message_id: "owner-message".into(),
            pinned: true,
        },
        ClientEvent::DeleteMessage {
            message_id: "owner-message".into(),
        },
        ClientEvent::EditMessage {
            message_id: "owner-message".into(),
            body_md: "hijacked".into(),
        },
    ] {
        event(&mut bws, denied).await;
        receive(&mut bws, |e| matches!(e, ServerEvent::Error { .. })).await;
    }
    let role_url = format!("{root}/people/{bid}/role");
    c.patch(&role_url)
        .bearer_auth(at)
        .json(&json!({"role":"moderator"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    event(
        &mut bws,
        ClientEvent::PinMessage {
            message_id: "owner-message".into(),
            pinned: true,
        },
    )
    .await;
    receive(&mut bws, |e| {
        matches!(e, ServerEvent::PinUpdated { pinned: true, .. })
    })
    .await;
    let pins: Vec<Value> = c
        .get(format!("{root}/pins"))
        .bearer_auth(bt)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(pins.len(), 1);
    c.patch(&role_url)
        .bearer_auth(at)
        .json(&json!({"role":"member"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    for denied in [
        ClientEvent::PinMessage {
            message_id: "owner-message".into(),
            pinned: false,
        },
        ClientEvent::DeleteMessage {
            message_id: "owner-message".into(),
        },
    ] {
        event(&mut bws, denied).await;
        receive(&mut bws, |e| matches!(e, ServerEvent::Error { .. })).await;
    }
    assert_eq!(
        c.get(format!("{root}/moderation"))
            .bearer_auth(bt)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    c.patch(&role_url)
        .bearer_auth(at)
        .json(&json!({"role":"moderator"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    event(
        &mut bws,
        ClientEvent::DeleteMessage {
            message_id: "owner-message".into(),
        },
    )
    .await;
    receive(
        &mut bws,
        |e| matches!(e,ServerEvent::MessageDeleted{message_id,..} if message_id=="owner-message"),
    )
    .await;
    let pins: Vec<Value> = c
        .get(format!("{root}/pins"))
        .bearer_auth(at)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(pins.is_empty());
    let private = post(
        &c,
        &format!("{root}/channels"),
        at,
        json!({"name":"restricted","is_private":true}),
    )
    .await;
    let private_id = private["id"].as_str().unwrap();
    send(&mut aws, "restricted-message", private_id, "secret").await;
    event(
        &mut bws,
        ClientEvent::PinMessage {
            message_id: "restricted-message".into(),
            pinned: true,
        },
    )
    .await;
    receive(&mut bws, |e| matches!(e, ServerEvent::Error { .. })).await;
    let dm = post(&c, &format!("{root}/directs"), at, json!({"user_id":bid})).await;
    let dm_id = dm["id"].as_str().unwrap();
    send(&mut bws, "bob-direct", dm_id, "bob's private message").await;
    event(
        &mut aws,
        ClientEvent::DeleteMessage {
            message_id: "bob-direct".into(),
        },
    )
    .await;
    receive(&mut aws, |e| matches!(e, ServerEvent::Error { .. })).await;
    let saved = post(&c, &format!("{root}/directs"), bt, json!({"user_id":bid})).await;
    let saved_id = saved["id"].as_str().unwrap();
    assert_eq!(saved["is_saved"], true);
    assert_eq!(saved["is_private"], true);
    assert_eq!(
        post(&c, &format!("{root}/directs"), bt, json!({"user_id":bid})).await["id"],
        saved["id"]
    );
    send(&mut bws, "saved-note", saved_id, "note to myself").await;
    event(
        &mut bws,
        ClientEvent::EditMessage {
            message_id: "saved-note".into(),
            body_md: "edited personal note".into(),
        },
    )
    .await;
    receive(
        &mut bws,
        |e| matches!(e,ServerEvent::MessageUpdated{message} if message.id=="saved-note"),
    )
    .await;
    event(
        &mut bws,
        ClientEvent::PinMessage {
            message_id: "saved-note".into(),
            pinned: true,
        },
    )
    .await;
    receive(
        &mut bws,
        |e| matches!(e,ServerEvent::PinUpdated{message_id,..} if message_id=="saved-note"),
    )
    .await;
    assert_eq!(
        c.get(format!("{root}/channels/{saved_id}/messages"))
            .bearer_auth(at)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    let audit: Vec<Value> = c
        .get(format!("{root}/moderation"))
        .bearer_auth(at)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        audit
            .iter()
            .any(|a| a["action"] == "message_deleted" && a["actor_id"] == bid)
    );
    assert!(
        !audit
            .iter()
            .any(|a| a["channel_id"] == saved_id || a["channel_id"] == dm_id)
    );
    let patch = json!({"name":"Renamed","slug":"renamed"});
    assert_eq!(
        c.patch(&root)
            .bearer_auth(bt)
            .json(&patch)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    c.patch(&root)
        .bearer_auth(at)
        .json(&patch)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    c.patch(format!("{root}/channels/{cid}"))
        .bearer_auth(bt)
        .json(&json!({"name":"renamed-channel","topic":"Updated topic"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    assert_eq!(
        c.patch(format!("{root}/channels/{saved_id}"))
            .bearer_auth(bt)
            .json(&json!({"name":"public-note"}))
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    assert_eq!(
        c.post(format!("{root}/channels/{saved_id}/members"))
            .bearer_auth(bt)
            .json(&json!({"user_id":aid}))
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    c.patch(format!("{base}/api/v1/users/me"))
        .bearer_auth(at)
        .json(&json!({"display_name":"Ihor","handle":"ihor","bio":"Building useful things"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let profile: Value = c
        .get(format!("{root}/people/{aid}"))
        .bearer_auth(bt)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(profile["user"]["bio"], "Building useful things");
    assert_eq!(profile["role"], "owner");
    assert!(profile["joined_at"].is_i64());
    assert_eq!(
        c.get(format!("{root}/people/{aid}"))
            .bearer_auth(ot)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        c.patch(format!("{root}/people/{aid}/role"))
            .bearer_auth(at)
            .json(&json!({"role":"member"}))
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    aws.close(None).await.unwrap();
    bws.close(None).await.unwrap();
    task.abort();
}
