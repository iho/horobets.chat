use client_core::{ClientEngine, Command, OutboxStatus, Snapshot};
use shared_proto::ClientEvent;
use std::time::Duration;
async fn wait(engine: &ClientEngine, predicate: impl Fn(&Snapshot) -> bool) -> Snapshot {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let snapshot = engine.updates.recv().await.unwrap();
            assert!(
                snapshot.error.is_none(),
                "client error: {:?}",
                snapshot.error
            );
            if predicate(&snapshot) {
                return snapshot;
            }
        }
    })
    .await
    .expect("timed out waiting for client state")
}
fn auth(engine: &ClientEngine, server: &str, email: &str) {
    engine
        .send(Command::Authenticate {
            server: server.into(),
            email: email.into(),
            password: "correct horse battery staple".into(),
            register: true,
        })
        .unwrap();
}
fn send(engine: &ClientEngine, body: &str, parent_id: Option<String>) {
    engine
        .send(Command::Send {
            body: body.into(),
            parent_id,
            topic_tag: None,
        })
        .unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_engine_delivers_threads_reactions_and_offline_outbox_after_restart() {
    let temp = tempfile::tempdir().unwrap();
    let app = messenger_server::App::open(
        temp.path().join("server"),
        "a-test-secret-with-at-least-thirty-two-bytes",
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = format!("http://{addr}");
    let server_app = app.clone();
    let server_task = tokio::spawn(async move {
        axum::serve(listener, messenger_server::api::router(server_app))
            .await
            .unwrap()
    });
    let alice_dir = temp.path().join("alice");
    let alice = ClientEngine::start(alice_dir.clone()).unwrap();
    let bob = ClientEngine::start(temp.path().join("bob")).unwrap();
    auth(&alice, &server, "alice@example.com");
    auth(&bob, &server, "bob@example.com");
    wait(&alice, |s| s.user_id.is_some() && !s.busy).await;
    wait(&bob, |s| s.user_id.is_some() && !s.busy).await;
    let photo = temp.path().join("profile.png");
    let mut encoded = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
        16,
        16,
        image::Rgb([40, 150, 120]),
    ))
    .write_to(&mut encoded, image::ImageFormat::Png)
    .unwrap();
    std::fs::write(&photo, encoded.into_inner()).unwrap();
    alice.send(Command::UploadAvatar(photo.clone())).unwrap();
    let profile = wait(&alice, |s| {
        s.me.as_ref().is_some_and(|u| u.avatar_url.is_some())
            && s.user_id
                .as_ref()
                .is_some_and(|id| s.avatars.contains_key(id))
    })
    .await;
    let alice_id = profile.user_id.unwrap();
    alice
        .send(Command::CreateWorkspace {
            name: "Engineering".into(),
            slug: "engineering".into(),
        })
        .unwrap();
    wait(&alice, |s| s.workspace.is_some()).await;
    alice
        .send(Command::Invite("bob@example.com".into()))
        .unwrap();
    wait(&alice, |s| {
        s.notice.as_deref() == Some("Member added to workspace")
    })
    .await;
    alice
        .send(Command::CreateChannel("general".into()))
        .unwrap();
    wait(&alice, |s| s.connected && s.channel.is_some()).await;
    bob.send(Command::Refresh).unwrap();
    wait(&bob, |s| {
        s.connected && s.channel.is_some() && s.avatars.contains_key(&alice_id)
    })
    .await;
    send(&alice, "SQLite from Alice", None);
    let received = wait(&bob, |s| {
        s.messages
            .iter()
            .any(|m| m.message.body_md == "SQLite from Alice")
    })
    .await;
    let id = received.messages[0].message.id.clone();
    wait(&alice, |s| {
        s.messages
            .iter()
            .any(|m| m.message.id == id && m.status == OutboxStatus::Sent)
    })
    .await;
    bob.send(Command::SelectThread(Some(id.clone()))).unwrap();
    send(&bob, "thread reply", Some(id.clone()));
    wait(&bob, |s| {
        s.thread_messages
            .iter()
            .any(|m| m.message.body_md == "thread reply" && m.status == OutboxStatus::Sent)
    })
    .await;
    alice.send(Command::SelectThread(Some(id.clone()))).unwrap();
    wait(&alice, |s| s.thread_messages.len() == 2).await;
    bob.send(Command::Event(ClientEvent::ToggleReaction {
        message_id: id.clone(),
        emoji: "👍".into(),
    }))
    .unwrap();
    wait(&alice, |s| {
        s.messages.iter().any(|m| !m.reactions.is_empty())
    })
    .await;
    let upload_path = temp.path().join("notes.txt");
    std::fs::write(&upload_path, b"shared attachment bytes").unwrap();
    alice.send(Command::Upload(upload_path)).unwrap();
    wait(&alice, |s| s.attachments.len() == 1).await;
    send(&alice, "attached note", None);
    let files = wait(&bob, |s| {
        s.message_attachments.values().any(|v| !v.is_empty())
    })
    .await;
    let attachment = files.message_attachments.values().flatten().next().unwrap();
    let saved = temp.path().join("download.txt");
    bob.send(Command::Download {
        id: attachment.id.clone(),
        path: saved.clone(),
    })
    .unwrap();
    wait(&bob, |s| s.notice.as_deref() == Some("File saved")).await;
    assert_eq!(std::fs::read(saved).unwrap(), b"shared attachment bytes");
    alice.send(Command::Upload(photo)).unwrap();
    wait(&alice, |s| {
        s.attachments.len() == 1 && !s.image_previews.is_empty()
    })
    .await;
    send(&alice, "", None);
    wait(&bob, |s| {
        !s.image_previews.is_empty()
            && s.messages
                .iter()
                .any(|m| m.message.body_md.is_empty() && m.status == OutboxStatus::Sent)
    })
    .await;
    alice.send(Command::Search("SQLite".into())).unwrap();
    wait(&alice, |s| s.search_results.len() == 1).await;
    // Closing both engines closes upgraded sockets. Then stop the listener entirely.
    drop(alice);
    drop(bob);
    server_task.abort();
    let _ = server_task.await;
    let offline = ClientEngine::start(alice_dir.clone()).unwrap();
    let cached = wait(&offline, |s| s.user_id.is_some() && !s.messages.is_empty()).await;
    assert!(cached.avatars.contains_key(&alice_id));
    assert!(!cached.image_previews.is_empty());
    send(&offline, "queued while offline", None);
    wait(&offline, |s| {
        s.messages.iter().any(|m| {
            m.message.body_md == "queued while offline" && m.status == OutboxStatus::Pending
        })
    })
    .await;
    drop(offline);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    let server_task = tokio::spawn(async move {
        axum::serve(listener, messenger_server::api::router(app))
            .await
            .unwrap()
    });
    let restored = ClientEngine::start(alice_dir).unwrap();
    let state = wait(&restored, |s| {
        s.connected
            && s.messages.iter().any(|m| {
                m.message.body_md == "queued while offline" && m.status == OutboxStatus::Sent
            })
    })
    .await;
    assert_eq!(
        state
            .messages
            .iter()
            .filter(|m| m.message.body_md == "queued while offline")
            .count(),
        1
    );
    assert_eq!(state.users.len(), 2);
    drop(restored);
    server_task.abort();
}
