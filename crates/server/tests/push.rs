use messenger_server::{App, push::Payload};
use serde_json::{Value, json};
#[test]
fn provider_payloads_carry_account_scoped_routes_and_deduplication_tags() {
    let p = Payload {
        workspace: "w".into(),
        channel: "c".into(),
        message: "m".into(),
        recipient: "bob".into(),
        title: "Alice".into(),
        body: "Hello".into(),
    };
    let apns = p.apns();
    assert_eq!(apns["aps"]["alert"]["body"], "Hello");
    assert_eq!(apns["aps"]["thread-id"], "w:c");
    assert_eq!(apns["user_id"], "bob");
    let fcm = p.fcm("test-token");
    assert_eq!(fcm["message"]["data"]["workspace_id"], "w");
    assert_eq!(fcm["message"]["data"]["message_id"], "m");
    assert!(fcm["message"]["notification"].is_null());
    assert_eq!(fcm["message"]["android"]["priority"], "high");
}
#[tokio::test]
async fn push_device_registration_requires_auth_and_logout_deletes_delivery_jobs() {
    let dir = tempfile::tempdir().unwrap();
    let app = App::open(
        dir.path().into(),
        "a-secret-with-more-than-thirty-two-bytes",
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let served = app.clone();
    let task = tokio::spawn(async move {
        axum::serve(listener, messenger_server::api::router(served))
            .await
            .unwrap()
    });
    let http = reqwest::Client::new();
    let a: Value = http
        .post(format!("{base}/api/v1/auth/register"))
        .json(&json!({"email":"a@example.test","password":"correct horse battery staple"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let b: Value = http
        .post(format!("{base}/api/v1/auth/register"))
        .json(&json!({"email":"b@example.test","password":"correct horse battery staple"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = uuid::Uuid::now_v7().to_string();
    let url = format!("{base}/api/v1/users/me/devices/{id}");
    let body = json!({"platform":"ios","token":"a".repeat(64),"environment":"development"});
    assert_eq!(
        http.put(&url).json(&body).send().await.unwrap().status(),
        401
    );
    let registered: Value = http
        .put(&url)
        .bearer_auth(a["token"].as_str().unwrap())
        .json(&body)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(registered["registered"], true);
    assert!(registered["delivery_configured"].is_boolean());
    let device = id.clone();
    app.db.global.write(move|c|{c.execute("INSERT INTO notification_jobs(workspace_id,channel_id,message_id,user_id,device_id,created_at,not_before) SELECT 'w','c','m',user_id,id,0,9223372036854775807 FROM push_tokens WHERE id=?",[device])?;Ok(())}).await.unwrap();
    http.delete(&url)
        .bearer_auth(b["token"].as_str().unwrap())
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    assert_eq!(
        app.db
            .global
            .read(
                |c| Ok(c.query_row("SELECT COUNT(*) FROM push_tokens", [], |r| r
                    .get::<_, i64>(0))?)
            )
            .await
            .unwrap(),
        1
    );
    http.delete(&url)
        .bearer_auth(a["token"].as_str().unwrap())
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let counts=app.db.global.read(|c|Ok(c.query_row("SELECT (SELECT COUNT(*) FROM push_tokens),(SELECT COUNT(*) FROM notification_jobs),(SELECT COUNT(*) FROM push_environments)",[],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,r.get::<_,i64>(2)?)))?)).await.unwrap();
    assert_eq!(counts, (0, 0, 0));
    task.abort();
}

#[tokio::test]
async fn account_wide_alert_stream_rechecks_workspace_and_private_channel_access() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest};
    type Socket = tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >;
    async fn connect(url: String, token: &str) -> Socket {
        let mut request = url.into_client_request().unwrap();
        request
            .headers_mut()
            .insert("authorization", format!("Bearer {token}").parse().unwrap());
        tokio_tungstenite::connect_async(request).await.unwrap().0
    }
    async fn alert(socket: &mut Socket) -> shared_proto::ServerEvent {
        loop {
            match socket.next().await.unwrap().unwrap() {
                Message::Ping(data) => socket.send(Message::Pong(data)).await.unwrap(),
                Message::Text(raw) => return serde_json::from_str(&raw).unwrap(),
                _ => {}
            }
        }
    }
    async fn post(http: &reqwest::Client, url: String, token: &str, body: Value) -> Value {
        http.post(url)
            .bearer_auth(token)
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
    async fn message(socket: &mut Socket, ch: &str, id: &str) {
        socket
            .send(Message::Text(
                serde_json::to_string(&shared_proto::ClientEvent::SendMessage {
                    client_msg_id: id.into(),
                    channel_id: ch.into(),
                    parent_id: None,
                    topic_tag: None,
                    body_md: id.into(),
                    attachment_ids: vec![],
                })
                .unwrap()
                .into(),
            ))
            .await
            .unwrap();
    }
    let dir = tempfile::tempdir().unwrap();
    let app = App::open(
        dir.path().into(),
        "a-secret-with-more-than-thirty-two-bytes",
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let base = format!("http://{address}/api/v1");
    let websocket = format!("ws://{address}/api/v1");
    let served = app.clone();
    let task = tokio::spawn(async move {
        axum::serve(listener, messenger_server::api::router(served))
            .await
            .unwrap()
    });
    let http = reqwest::Client::new();
    let mut accounts = vec![];
    for email in [
        "alice@example.test",
        "bob@example.test",
        "outsider@example.test",
    ] {
        accounts.push(
            http.post(format!("{base}/auth/register"))
                .json(&json!({"email":email,"password":"correct horse battery staple"}))
                .send()
                .await
                .unwrap()
                .json::<Value>()
                .await
                .unwrap(),
        );
    }
    let at = accounts[0]["token"].as_str().unwrap();
    let bt = accounts[1]["token"].as_str().unwrap();
    let et = accounts[2]["token"].as_str().unwrap();
    let w = post(
        &http,
        format!("{base}/workspaces"),
        at,
        json!({"name":"Alert ACL","slug":"alert-acl"}),
    )
    .await;
    let wid = w["id"].as_str().unwrap();
    post(
        &http,
        format!("{base}/workspaces/{wid}/members"),
        at,
        json!({"email":"bob@example.test"}),
    )
    .await;
    let public = post(
        &http,
        format!("{base}/workspaces/{wid}/channels"),
        at,
        json!({"name":"general"}),
    )
    .await;
    let private = post(
        &http,
        format!("{base}/workspaces/{wid}/channels"),
        at,
        json!({"name":"private","is_private":true}),
    )
    .await;
    let mut bob = connect(format!("{websocket}/users/me/notifications/ws"), bt).await;
    let mut outsider = connect(format!("{websocket}/users/me/notifications/ws"), et).await;
    let mut alice = connect(format!("{websocket}/workspaces/{wid}/ws"), at).await;
    // Waiting for Hello gives the alert subscribers time to complete their upgrades.
    let _ = alert(&mut alice).await;
    message(&mut alice, public["id"].as_str().unwrap(), "public-message").await;
    let e = tokio::time::timeout(std::time::Duration::from_secs(3), alert(&mut bob))
        .await
        .unwrap();
    assert!(
        matches!(e,shared_proto::ServerEvent::Notification{workspace_id,message,..} if workspace_id==wid&&message.id=="public-message")
    );
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(250), alert(&mut outsider))
            .await
            .is_err()
    );
    message(&mut alice, public["id"].as_str().unwrap(), "public-message").await;
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(250), alert(&mut bob))
            .await
            .is_err()
    );
    message(
        &mut alice,
        private["id"].as_str().unwrap(),
        "private-message",
    )
    .await;
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(250), alert(&mut bob))
            .await
            .is_err()
    );
    let w = wid.to_owned();
    let user = accounts[1]["user_id"].as_str().unwrap().to_owned();
    app.db
        .global
        .write(move |c| {
            c.execute(
                "DELETE FROM workspace_members WHERE workspace_id=? AND user_id=?",
                rusqlite::params![w, user],
            )?;
            Ok(())
        })
        .await
        .unwrap();
    message(
        &mut alice,
        public["id"].as_str().unwrap(),
        "after-revocation",
    )
    .await;
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(250), alert(&mut bob))
            .await
            .is_err()
    );
    task.abort();
}
