use futures_util::{SinkExt, StreamExt};
use reqwest::Client;
use serde_json::{Value, json};
use shared_proto::{ClientEvent, ServerEvent};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest, http::HeaderValue},
};
async fn receive<S>(ws: &mut tokio_tungstenite::WebSocketStream<S>) -> ServerEvent
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let Message::Text(t) = ws.next().await.unwrap().unwrap() {
                return serde_json::from_str(&t).unwrap();
            }
        }
    })
    .await
    .unwrap()
}
async fn register(client: &Client, base: &str, email: &str) -> Value {
    client
        .post(format!("{base}/api/v1/auth/register"))
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
#[tokio::test]
async fn two_users_messages_search_sync_and_isolation() {
    let temp = tempfile::tempdir().unwrap();
    let app = messenger_server::App::open(
        temp.path().to_owned(),
        "test-secret-with-at-least-thirty-two-bytes",
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        axum::serve(listener, messenger_server::api::router(app))
            .await
            .unwrap()
    });
    let base = format!("http://{addr}");
    let client = Client::new();
    let a = register(&client, &base, "alice@example.com").await;
    let b = register(&client, &base, "bob@example.com").await;
    let outsider = register(&client, &base, "outsider@example.com").await;
    let at = a["token"].as_str().unwrap();
    let bt = b["token"].as_str().unwrap();
    let login: Value = client
        .post(format!("{base}/api/v1/auth/login"))
        .json(&json!({"email":"alice@example.com","password":"correct horse battery staple"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(login["token"].is_string());
    let w: Value = client
        .post(format!("{base}/api/v1/workspaces"))
        .bearer_auth(at)
        .json(&json!({"slug":"engineering","name":"Engineering"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let root = format!("{base}/api/v1/workspaces/{}", w["id"].as_str().unwrap());
    assert_eq!(
        client
            .get(format!("{root}/channels"))
            .bearer_auth(outsider["token"].as_str().unwrap())
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    client
        .post(format!("{root}/members"))
        .bearer_auth(at)
        .json(&json!({"email":"bob@example.com"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let ch: Value = client
        .post(format!("{root}/channels"))
        .bearer_auth(at)
        .json(&json!({"name":"general"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let channel = ch["id"].as_str().unwrap().to_owned();
    let private: Value = client
        .post(format!("{root}/channels"))
        .bearer_auth(at)
        .json(&json!({"name":"secret","is_private":true}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(
        client
            .get(format!(
                "{root}/channels/{}/messages",
                private["id"].as_str().unwrap()
            ))
            .bearer_auth(bt)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    let url = format!(
        "ws://{addr}/api/v1/workspaces/{}/ws",
        w["id"].as_str().unwrap()
    );
    let mut ar = url.clone().into_client_request().unwrap();
    ar.headers_mut().insert(
        "authorization",
        HeaderValue::from_str(&format!("Bearer {at}")).unwrap(),
    );
    let mut br = url.into_client_request().unwrap();
    br.headers_mut().insert(
        "authorization",
        HeaderValue::from_str(&format!("Bearer {bt}")).unwrap(),
    );
    let (mut aws, _) = connect_async(ar).await.unwrap();
    let (mut bws, _) = connect_async(br).await.unwrap();
    assert!(matches!(receive(&mut aws).await, ServerEvent::Hello { .. }));
    assert!(matches!(receive(&mut bws).await, ServerEvent::Hello { .. }));
    let msg_id = uuid::Uuid::now_v7().to_string();
    let event = ClientEvent::SendMessage {
        client_msg_id: msg_id.clone(),
        channel_id: channel.clone(),
        parent_id: None,
        topic_tag: Some("backend".into()),
        body_md: "SQLite concurrency works".into(),
        attachment_ids: vec![],
    };
    aws.send(Message::Text(serde_json::to_string(&event).unwrap().into()))
        .await
        .unwrap();
    let received = receive(&mut bws).await;
    assert!(
        matches!(&received,ServerEvent::MessageCreated{message,..} if message.body_md=="SQLite concurrency works")
    );
    assert_eq!(receive(&mut aws).await, received);
    // Retry acknowledges the same canonical row without a duplicate broadcast or insert.
    aws.send(Message::Text(serde_json::to_string(&event).unwrap().into()))
        .await
        .unwrap();
    assert_eq!(receive(&mut aws).await, received);
    let found: Vec<Value> = client
        .get(format!("{root}/search"))
        .query(&[("q", "concurrency")])
        .bearer_auth(bt)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(found.len(), 1);
    let history: Vec<Value> = client
        .get(format!("{root}/channels/{channel}/messages"))
        .bearer_auth(bt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(history.len(), 1);
    let sync: Value = client
        .get(format!("{root}/sync?since_seq=0"))
        .bearer_auth(bt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let cursor = sync["next_seq"].as_i64().unwrap();
    assert_eq!(sync["events"].as_array().unwrap().len(), 1);
    let edit = ClientEvent::EditMessage {
        message_id: msg_id.clone(),
        body_md: "edited durable event".into(),
    };
    aws.send(Message::Text(serde_json::to_string(&edit).unwrap().into()))
        .await
        .unwrap();
    assert!(matches!(
        receive(&mut bws).await,
        ServerEvent::MessageUpdated { .. }
    ));
    receive(&mut aws).await;
    let delta: Value = client
        .get(format!("{root}/sync?since_seq={cursor}"))
        .bearer_auth(bt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(delta["events"][0]["event"]["type"], "message_updated");
    // Bob replies in the same thread and toggles a reaction.
    let reply = ClientEvent::SendMessage {
        client_msg_id: uuid::Uuid::now_v7().to_string(),
        channel_id: channel.clone(),
        parent_id: Some(msg_id.clone()),
        topic_tag: None,
        body_md: "reply from Bob".into(),
        attachment_ids: vec![],
    };
    bws.send(Message::Text(serde_json::to_string(&reply).unwrap().into()))
        .await
        .unwrap();
    assert!(
        matches!(receive(&mut aws).await, ServerEvent::MessageCreated{message,..} if message.parent_id.as_deref()==Some(&msg_id))
    );
    receive(&mut bws).await;
    for added in [true, false] {
        let reaction = ClientEvent::ToggleReaction {
            message_id: msg_id.clone(),
            emoji: "👍".into(),
        };
        bws.send(Message::Text(
            serde_json::to_string(&reaction).unwrap().into(),
        ))
        .await
        .unwrap();
        assert!(
            matches!(receive(&mut aws).await,ServerEvent::ReactionUpdated{added:actual,..} if actual==added)
        );
        receive(&mut bws).await;
    }
    // Private-channel events are invisible both live and through durable sync.
    let secret = ClientEvent::SendMessage {
        client_msg_id: uuid::Uuid::now_v7().to_string(),
        channel_id: private["id"].as_str().unwrap().into(),
        parent_id: None,
        topic_tag: None,
        body_md: "classified sentinel".into(),
        attachment_ids: vec![],
    };
    aws.send(Message::Text(
        serde_json::to_string(&secret).unwrap().into(),
    ))
    .await
    .unwrap();
    receive(&mut aws).await;
    bws.send(Message::Text(
        serde_json::to_string(&ClientEvent::Ping { ts: 42 })
            .unwrap()
            .into(),
    ))
    .await
    .unwrap();
    assert_eq!(receive(&mut bws).await, ServerEvent::Pong { ts: 42 });
    let private_sync: Value = client
        .get(format!("{root}/sync?since_seq=0"))
        .bearer_auth(bt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(!private_sync.to_string().contains("classified sentinel"));
    let private_search: Vec<Value> = client
        .get(format!("{root}/search?q=classified"))
        .bearer_auth(bt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(private_search.is_empty());
    let bad = ClientEvent::EditMessage {
        message_id: msg_id.clone(),
        body_md: "hijacked".into(),
    };
    bws.send(Message::Text(serde_json::to_string(&bad).unwrap().into()))
        .await
        .unwrap();
    assert!(matches!(receive(&mut bws).await, ServerEvent::Error { .. }));
    let rejected = ClientEvent::SendMessage {
        client_msg_id: "rejected-send".into(),
        channel_id: private["id"].as_str().unwrap().into(),
        parent_id: None,
        topic_tag: None,
        body_md: "unauthorized".into(),
        attachment_ids: vec![],
    };
    bws.send(Message::Text(
        serde_json::to_string(&rejected).unwrap().into(),
    ))
    .await
    .unwrap();
    assert!(
        matches!(receive(&mut bws).await,ServerEvent::Error{client_msg_id:Some(id),..} if id=="rejected-send")
    );
    let delete = ClientEvent::DeleteMessage { message_id: msg_id };
    aws.send(Message::Text(
        serde_json::to_string(&delete).unwrap().into(),
    ))
    .await
    .unwrap();
    assert!(matches!(
        receive(&mut bws).await,
        ServerEvent::MessageDeleted { .. }
    ));
    receive(&mut aws).await;
    let found: Vec<Value> = client
        .get(format!("{root}/search?q=durable"))
        .bearer_auth(bt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(found.is_empty());
    let form = reqwest::multipart::Form::new().part(
        "file",
        reqwest::multipart::Part::bytes(b"hello attachment".to_vec()).file_name("test.txt"),
    );
    let attachment: Value = client
        .post(format!("{root}/attachments"))
        .bearer_auth(at)
        .multipart(form)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let path = format!("{root}/attachments/{}", attachment["id"].as_str().unwrap());
    assert_eq!(
        client
            .get(&path)
            .bearer_auth(bt)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        client
            .get(path)
            .bearer_auth(at)
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .text()
            .await
            .unwrap(),
        "hello attachment"
    );
    aws.close(None).await.unwrap();
    bws.close(None).await.unwrap();
    task.abort();
}
