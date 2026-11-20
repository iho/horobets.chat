use messenger_mobile::{MobileClient, SnapshotListener};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
struct Listener(async_channel::Sender<Value>);
impl SnapshotListener for Listener {
    fn on_snapshot(&self, json: String) {
        self.0
            .try_send(serde_json::from_str(&json).unwrap())
            .unwrap();
    }
}
async fn wait(rx: &async_channel::Receiver<Value>, predicate: impl Fn(&Value) -> bool) -> Value {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let state = rx.recv().await.unwrap();
            assert!(state["error"].is_null(), "{state}");
            if predicate(&state) {
                return state;
            }
        }
    })
    .await
    .unwrap()
}
fn command(client: &MobileClient, kind: &str, payload: Value) {
    client
        .send_command(json!({"type":kind,"payload":payload}).to_string())
        .unwrap();
}
fn subscribe(client: &MobileClient) -> async_channel::Receiver<Value> {
    let (tx, rx) = async_channel::unbounded();
    client.subscribe(Box::new(Listener(tx))).unwrap();
    rx
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mobile_json_contract_authenticates_sends_replies_and_restores_offline() {
    let temp = tempfile::tempdir().unwrap();
    let app = messenger_server::App::open(
        temp.path().join("server"),
        "a-test-secret-with-at-least-thirty-two-bytes",
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, messenger_server::api::router(app))
            .await
            .unwrap();
    });
    let directory = temp.path().join("mobile").to_str().unwrap().to_owned();
    let client = MobileClient::new(directory.clone()).unwrap();
    let rx = subscribe(&client);
    command(
        &client,
        "authenticate",
        json!({"server":format!("http://{address}"),"email":"mobile@test.local","password":"correct horse battery staple","register":true}),
    );
    wait(&rx, |s| s["user_id"].is_string() && s["busy"] == false).await;
    command(
        &client,
        "create_workspace",
        json!({"name":"Mobile workspace","slug":"mobile"}),
    );
    wait(&rx, |s| {
        s["workspace"].is_string() && s["connected"] == true
    })
    .await;
    command(&client, "create_channel", json!("general"));
    wait(&rx, |s| {
        s["channel"].is_string() && s["channels"].as_array().is_some_and(|a| !a.is_empty())
    })
    .await;
    let photo = temp.path().join("mobile-photo.png");
    image::RgbImage::from_pixel(32, 24, image::Rgb([35u8, 180, 150]))
        .save(&photo)
        .unwrap();
    command(&client, "upload_avatar", json!(photo.to_str().unwrap()));
    let avatar = wait(&rx, |s| {
        s["me"]["avatar_url"].is_string() && s["avatars"].as_object().is_some_and(|a| !a.is_empty())
    })
    .await;
    assert!(
        std::path::Path::new(
            avatar["avatars"]
                .as_object()
                .unwrap()
                .values()
                .next()
                .unwrap()
                .as_str()
                .unwrap()
        )
        .is_file()
    );
    command(&client, "upload", json!(photo.to_str().unwrap()));
    let attachment = wait(&rx, |s| {
        s["attachments"].as_array().is_some_and(|a| !a.is_empty())
            && s["image_previews"]
                .as_object()
                .is_some_and(|a| !a.is_empty())
    })
    .await;
    let attachment_id = attachment["attachments"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let preview = attachment["image_previews"][&attachment_id]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(std::path::Path::new(&preview).is_file());
    command(&client, "send", json!({"body":"Hello from **mobile** 👋"}));
    let sent = wait(&rx, |s| s["messages"][0]["status"] == "Sent").await;
    let parent = sent["messages"][0]["message"]["id"].as_str().unwrap();
    if sent["message_attachments"][parent]
        .as_array()
        .is_none_or(|a| a.is_empty())
    {
        wait(&rx, |s| {
            s["message_attachments"][parent]
                .as_array()
                .is_some_and(|a| a.iter().any(|a| a["id"] == attachment_id))
        })
        .await;
    }
    let download = temp.path().join("download.png");
    command(
        &client,
        "download",
        json!({"id":attachment_id,"path":download.to_str().unwrap()}),
    );
    wait(&rx, |s| {
        s["notice"]
            .as_str()
            .is_some_and(|n| n.contains("Saved") || n.contains("saved"))
    })
    .await;
    assert_eq!(
        std::fs::read(download).unwrap(),
        std::fs::read(&photo).unwrap()
    );
    command(&client, "select_thread", json!(parent));
    command(
        &client,
        "send",
        json!({"body":"Thread reply","parent_id":parent}),
    );
    wait(&rx, |s| {
        s["thread_messages"].as_array().is_some_and(|a| {
            a.iter()
                .any(|m| m["message"]["body_md"] == "Thread reply" && m["status"] == "Sent")
        })
    })
    .await;
    command(&client, "select_thread", Value::Null);
    wait(&rx, |s| s["thread"].is_null()).await;
    client.shutdown();
    drop(client);
    server.abort();
    let restored: Arc<MobileClient> = MobileClient::new(directory).unwrap();
    let rx = subscribe(&restored);
    wait(&rx, |s| {
        s["user_id"].is_string() && s["messages"].as_array().is_some_and(|a| !a.is_empty())
    })
    .await;
    assert!(std::path::Path::new(&preview).is_file());
    command(&restored, "search", json!("mobile"));
    wait(&rx, |s| {
        s["search_results"]
            .as_array()
            .is_some_and(|a| !a.is_empty())
    })
    .await;
    restored.shutdown();
}
