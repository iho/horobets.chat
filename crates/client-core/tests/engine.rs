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
    // Profile edits must reach other users AND the editor's other devices via
    // the live catalog event, without Refresh or waiting for the 30-second poll.
    let alice_other = ClientEngine::start(temp.path().join("alice-other-device")).unwrap();
    alice_other
        .send(Command::Authenticate {
            server: server.clone(),
            email: "alice@example.com".into(),
            password: "correct horse battery staple".into(),
            register: false,
        })
        .unwrap();
    wait(&alice_other, |s| s.connected && s.channel.is_some()).await;
    alice
        .send(Command::SaveProfile {
            display_name: "Alice Updated".into(),
            handle: "alice_updated".into(),
            bio: Some("Updated on another device".into()),
        })
        .unwrap();
    for client in [&bob, &alice_other] {
        let state = tokio::time::timeout(
            Duration::from_secs(5),
            wait(client, |s| {
                s.users.iter().any(|u| {
                    u.id == alice_id
                        && u.display_name == "Alice Updated"
                        && u.handle.as_deref() == Some("alice_updated")
                })
            }),
        )
        .await
        .expect("profile update was not broadcast promptly");
        assert!(
            state
                .people
                .iter()
                .any(|m| m.user.id == alice_id && m.user.bio == "Updated on another device")
        );
    }
    let old_avatar = profile.me.unwrap().avatar_url;
    // A replacement generates a different immutable URL and must be fetched
    // into every client's authenticated image cache before publishing its view.
    alice.send(Command::UploadAvatar(photo.clone())).unwrap();
    for client in [&bob, &alice_other] {
        let state = tokio::time::timeout(
            Duration::from_secs(5),
            wait(client, |s| {
                s.users.iter().any(|u| {
                    u.id == alice_id && u.avatar_url.is_some() && u.avatar_url != old_avatar
                }) && s.avatars.get(&alice_id).is_some_and(|path| path.is_file())
            }),
        )
        .await
        .expect("avatar replacement was not broadcast promptly");
        if state.user_id.as_deref() == Some(&alice_id) {
            assert_eq!(state.me.as_ref().unwrap().display_name, "Alice Updated");
            assert_ne!(state.me.as_ref().unwrap().avatar_url, old_avatar);
        }
    }
    drop(alice_other);
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
    // Multi-codepoint emoji coexist independently and toggling one preserves the others.
    for emoji in ["❤️", "😂", "🎉", "👀", "✅"] {
        bob.send(Command::Event(ClientEvent::ToggleReaction {
            message_id: id.clone(),
            emoji: emoji.into(),
        }))
        .unwrap();
    }
    wait(&alice, |s| {
        s.messages
            .iter()
            .any(|m| m.message.id == id && m.reactions.len() == 6)
    })
    .await;
    bob.send(Command::Event(ClientEvent::ToggleReaction {
        message_id: id.clone(),
        emoji: "😂".into(),
    }))
    .unwrap();
    let reactions = wait(&alice, |s| {
        s.messages.iter().any(|m| {
            m.message.id == id
                && m.reactions.len() == 5
                && !m.reactions.iter().any(|r| r.emoji == "😂")
        })
    })
    .await;
    assert!(
        reactions
            .messages
            .iter()
            .find(|m| m.message.id == id)
            .unwrap()
            .reactions
            .iter()
            .any(|r| r.emoji == "❤️")
    );
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
    assert_eq!(
        state
            .messages
            .iter()
            .find(|m| m.message.id == id)
            .unwrap()
            .reactions
            .len(),
        5
    );
    assert_eq!(state.users.len(), 2);
    drop(restored);
    server_task.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn invitations_replay_history_revocation_purges_search_and_dms_stay_private() {
    let temp = tempfile::tempdir().unwrap();
    let app = messenger_server::App::open(
        temp.path().join("server"),
        "a-secret-with-at-least-thirty-two-characters",
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, messenger_server::api::router(app))
            .await
            .unwrap()
    });
    let alice = ClientEngine::start(temp.path().join("alice")).unwrap();
    let bob = ClientEngine::start(temp.path().join("bob")).unwrap();
    let carol = ClientEngine::start(temp.path().join("carol")).unwrap();
    auth(&alice, &base, "alice@example.com");
    auth(&bob, &base, "bob@example.com");
    auth(&carol, &base, "carol@example.com");
    let a = wait(&alice, |s| s.user_id.is_some() && !s.busy).await;
    let b = wait(&bob, |s| s.user_id.is_some() && !s.busy).await;
    wait(&carol, |s| s.user_id.is_some() && !s.busy).await;
    let aid = a.user_id.unwrap();
    let bid = b.user_id.unwrap();
    alice
        .send(Command::SaveProfile {
            display_name: "Ihor".into(),
            handle: "ihor".into(),
            bio: None,
        })
        .unwrap();
    wait(&alice, |s| {
        s.me.as_ref().and_then(|u| u.handle.as_deref()) == Some("ihor")
    })
    .await;
    alice
        .send(Command::CreateWorkspace {
            name: "Team".into(),
            slug: "team".into(),
        })
        .unwrap();
    wait(&alice, |s| s.workspace.is_some()).await;
    for email in ["bob@example.com", "carol@example.com"] {
        alice.send(Command::Invite(email.into())).unwrap();
        wait(&alice, |s| {
            s.notice.as_deref() == Some("Member added to workspace")
        })
        .await;
    }
    alice
        .send(Command::CreateChannel("general".into()))
        .unwrap();
    let general = wait(&alice, |s| {
        s.channels.iter().any(|ch| ch.name == "general") && s.connected
    })
    .await
    .channels
    .into_iter()
    .find(|c| c.name == "general")
    .unwrap()
    .id;
    alice
        .send(Command::CreatePrivateChannel("staff".into()))
        .unwrap();
    let staff = wait(&alice, |s| {
        s.channels.iter().any(|ch| ch.name == "staff") && s.channel.as_ref() != Some(&general)
    })
    .await
    .channel
    .unwrap();
    send(&alice, "secret-before-invitation", None);
    wait(&alice, |s| {
        s.messages.iter().any(|m| {
            m.message.body_md == "secret-before-invitation" && m.status == OutboxStatus::Sent
        })
    })
    .await;
    bob.send(Command::Refresh).unwrap();
    let view = wait(&bob, |s| {
        s.connected && s.channels.iter().any(|ch| ch.name == "general")
    })
    .await;
    assert!(!view.channels.iter().any(|ch| ch.id == staff));
    carol.send(Command::Refresh).unwrap();
    wait(&carol, |s| s.connected && s.channel.is_some()).await;
    alice
        .send(Command::ChannelMember {
            user_id: bid.clone(),
            add: true,
        })
        .unwrap();
    wait(&bob, |s| s.channels.iter().any(|ch| ch.id == staff)).await;
    bob.send(Command::SelectChannel(staff.clone())).unwrap();
    wait(&bob, |s| {
        s.channel.as_ref() == Some(&staff)
            && s.messages
                .iter()
                .any(|m| m.message.body_md == "secret-before-invitation")
    })
    .await;
    bob.send(Command::Search("secret-before-invitation".into()))
        .unwrap();
    wait(&bob, |s| s.search_results.len() == 1).await;
    alice
        .send(Command::ChannelMember {
            user_id: bid.clone(),
            add: false,
        })
        .unwrap();
    wait(&bob, |s| {
        !s.channels.iter().any(|ch| ch.id == staff) && s.search_results.is_empty()
    })
    .await;
    alice.send(Command::OpenDirect(bid.clone())).unwrap();
    let dm = wait(&alice, |s| {
        s.channels.iter().any(|ch| ch.is_direct) && s.channel.as_ref() != Some(&staff)
    })
    .await
    .channel
    .unwrap();
    wait(&bob, |s| s.channels.iter().any(|ch| ch.id == dm)).await;
    bob.send(Command::OpenDirect(aid.clone())).unwrap();
    wait(&bob, |s| s.channel.as_ref() == Some(&dm)).await;
    send(&alice, "pairwise-private-message", None);
    wait(&bob, |s| {
        s.messages
            .iter()
            .any(|m| m.message.body_md == "pairwise-private-message")
    })
    .await;
    carol.send(Command::Refresh).unwrap();
    let view = wait(&carol, |s| s.connected && s.channel.is_some()).await;
    assert!(!view.channels.iter().any(|ch| ch.id == dm || ch.id == staff));
    carol
        .send(Command::Search("pairwise-private-message".into()))
        .unwrap();
    let view = wait(&carol, |s| s.search_query == "pairwise-private-message").await;
    assert!(view.search_results.is_empty());
    drop(alice);
    drop(bob);
    drop(carol);
    task.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn management_pins_profiles_and_saved_messages_sync_and_survive_offline() {
    let temp = tempfile::tempdir().unwrap();
    let app = messenger_server::App::open(
        temp.path().join("server"),
        "a-secret-with-at-least-thirty-two-characters",
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, messenger_server::api::router(app))
            .await
            .unwrap()
    });
    let alice = ClientEngine::start(temp.path().join("alice")).unwrap();
    let bob_dir = temp.path().join("bob");
    let bob = ClientEngine::start(bob_dir.clone()).unwrap();
    auth(&alice, &base, "alice@example.com");
    auth(&bob, &base, "bob@example.com");
    let a = wait(&alice, |s| s.user_id.is_some() && !s.busy).await;
    let b = wait(&bob, |s| s.user_id.is_some() && !s.busy).await;
    let aid = a.user_id.unwrap();
    let bid = b.user_id.unwrap();
    alice
        .send(Command::CreateWorkspace {
            name: "Original".into(),
            slug: "original".into(),
        })
        .unwrap();
    wait(&alice, |s| s.workspace.is_some() && s.role() == "owner").await;
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
    let general = wait(&alice, |s| s.connected && s.channel.is_some())
        .await
        .channel
        .unwrap();
    bob.send(Command::Refresh).unwrap();
    wait(&bob, |s| s.connected && s.channel.is_some()).await;
    alice
        .send(Command::SaveProfile {
            display_name: "Ihor".into(),
            handle: "ihor".into(),
            bio: Some("Builder of useful things".into()),
        })
        .unwrap();
    wait(&alice, |s| {
        s.me.as_ref()
            .is_some_and(|u| u.bio == "Builder of useful things")
    })
    .await;
    alice
        .send(Command::EditWorkspace {
            name: "Renamed workspace".into(),
            slug: "renamed-workspace".into(),
        })
        .unwrap();
    wait(&bob, |s| {
        s.workspaces.iter().any(|w| w.name == "Renamed workspace")
    })
    .await;
    alice
        .send(Command::SetRole {
            user_id: bid.clone(),
            role: "moderator".into(),
        })
        .unwrap();
    wait(&bob, |s| s.role() == "moderator").await;
    bob.send(Command::EditChannel {
        id: general.clone(),
        name: "renamed-channel".into(),
        topic: "Updated topic".into(),
    })
    .unwrap();
    wait(&alice, |s| {
        s.channels.iter().any(|c| {
            c.id == general
                && c.name == "renamed-channel"
                && c.topic.as_deref() == Some("Updated topic")
        })
    })
    .await;
    send(&alice, "Channel message for pinning", None);
    let state = wait(&bob, |s| {
        s.messages
            .iter()
            .any(|m| m.message.body_md == "Channel message for pinning")
    })
    .await;
    let id = state
        .messages
        .iter()
        .find(|m| m.message.body_md == "Channel message for pinning")
        .unwrap()
        .message
        .id
        .clone();
    bob.send(Command::Event(ClientEvent::PinMessage {
        message_id: id.clone(),
        pinned: true,
    }))
    .unwrap();
    wait(&alice, |s| {
        s.pins.iter().any(|p| p.message.id == id)
            && s.moderation.iter().any(|a| a.action == "message_pinned")
    })
    .await;
    bob.send(Command::Event(ClientEvent::DeleteMessage {
        message_id: id.clone(),
    }))
    .unwrap();
    wait(&alice, |s| {
        !s.pins.iter().any(|p| p.message.id == id)
            && s.messages
                .iter()
                .any(|m| m.message.id == id && m.message.deleted_at.is_some())
    })
    .await;
    bob.send(Command::OpenDirect(bid.clone())).unwrap();
    let saved = wait(&bob, |s| {
        s.channels
            .iter()
            .any(|c| c.is_saved && Some(&c.id) == s.channel.as_ref())
    })
    .await
    .channel
    .unwrap();
    send(&bob, "Only I can read this note", None);
    let state = wait(&bob, |s| {
        s.messages.iter().any(|m| {
            m.message.body_md == "Only I can read this note" && m.status == OutboxStatus::Sent
        })
    })
    .await;
    let note = state
        .messages
        .iter()
        .find(|m| m.message.body_md == "Only I can read this note")
        .unwrap()
        .message
        .id
        .clone();
    bob.send(Command::Event(ClientEvent::PinMessage {
        message_id: note.clone(),
        pinned: true,
    }))
    .unwrap();
    wait(&bob, |s| s.pins.iter().any(|p| p.message.id == note)).await;
    alice.send(Command::Refresh).unwrap();
    let state = wait(&alice, |s| s.connected && !s.busy).await;
    assert!(!state.channels.iter().any(|c| c.id == saved));
    assert!(!state.pins.iter().any(|p| p.message.id == note));
    bob.send(Command::Refresh).unwrap();
    let state = wait(&bob, |s| {
        s.people
            .iter()
            .any(|m| m.user.id == aid && m.user.bio == "Builder of useful things")
    })
    .await;
    assert_eq!(
        state.people.iter().find(|m| m.user.id == aid).unwrap().role,
        "owner"
    );
    drop(alice);
    drop(bob);
    task.abort();
    let _ = task.await;
    let offline = ClientEngine::start(bob_dir).unwrap();
    let state = wait(&offline, |s| s.pins.iter().any(|p| p.message.id == note)).await;
    assert!(state.channels.iter().any(|c| c.is_saved && c.id == saved));
    assert!(
        state
            .people
            .iter()
            .any(|m| m.user.id == aid && m.user.bio == "Builder of useful things")
    );
    drop(offline);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unread_and_alerts_follow_visibility_and_read_marks_sync_across_devices() {
    let temp = tempfile::tempdir().unwrap();
    let app = messenger_server::App::open(
        temp.path().join("server"),
        "a-secret-with-at-least-thirty-two-bytes",
    )
    .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, messenger_server::api::router(app))
            .await
            .unwrap()
    });
    let alice = ClientEngine::start(temp.path().join("alice")).unwrap();
    let bob = ClientEngine::start(temp.path().join("bob")).unwrap();
    auth(&alice, &server, "alice@example.com");
    auth(&bob, &server, "bob@example.com");
    wait(&alice, |s| s.user_id.is_some() && !s.busy).await;
    wait(&bob, |s| s.user_id.is_some() && !s.busy).await;
    alice
        .send(Command::CreateWorkspace {
            name: "Unread QA".into(),
            slug: "unread-qa".into(),
        })
        .unwrap();
    wait(&alice, |s| s.workspace.is_some()).await;
    alice
        .send(Command::Invite("bob@example.com".into()))
        .unwrap();
    wait(&alice, |s| s.people.len() == 2).await;
    alice
        .send(Command::CreateChannel("general".into()))
        .unwrap();
    wait(&alice, |s| s.connected && s.channel.is_some()).await;
    bob.send(Command::Refresh).unwrap();
    wait(&bob, |s| s.connected && s.channel.is_some()).await;
    send(&alice, "Notify while backgrounded", None);
    let unread = wait(&bob, |s| {
        s.unread_messages
            .iter()
            .any(|m| m.body_md == "Notify while backgrounded")
            && !s.notifications.is_empty()
    })
    .await;
    let id = unread.unread_messages[0].id.clone();
    assert_eq!(
        unread
            .notifications
            .iter()
            .filter(|n| n.message.id == id)
            .count(),
        1
    );
    bob.send(Command::Refresh).unwrap();
    wait(&bob, |s| !s.busy && s.connected && !s.unread.is_empty()).await;
    bob.send(Command::SetActivity {
        active: true,
        reading: true,
    })
    .unwrap();
    wait(&bob, |s| s.unread.is_empty()).await;
    let other = ClientEngine::start(temp.path().join("bob-other")).unwrap();
    other
        .send(Command::Authenticate {
            server: server.clone(),
            email: "bob@example.com".into(),
            password: "correct horse battery staple".into(),
            register: false,
        })
        .unwrap();
    wait(&other, |s| {
        s.connected && s.channel.is_some() && s.unread.is_empty()
    })
    .await;
    send(&alice, "Reading this conversation", None);
    let state = wait(&bob, |s| {
        s.messages
            .iter()
            .any(|m| m.message.body_md == "Reading this conversation")
            && s.unread.is_empty()
    })
    .await;
    assert!(
        !state
            .notifications
            .iter()
            .any(|n| n.message.body_md == "Reading this conversation")
    );
    bob.send(Command::SetActivity {
        active: false,
        reading: false,
    })
    .unwrap();
    wait(&bob, |s| {
        s.messages
            .iter()
            .any(|m| m.message.body_md == "Reading this conversation")
    })
    .await;
    send(&alice, "Back in the inbox", None);
    wait(&bob, |s| {
        s.unread_messages
            .iter()
            .any(|m| m.body_md == "Back in the inbox")
    })
    .await;
    bob.send(Command::MarkAllRead).unwrap();
    wait(&bob, |s| s.unread.is_empty()).await;
    wait(&other, |s| {
        s.unread.is_empty()
            && s.messages
                .iter()
                .any(|m| m.message.body_md == "Back in the inbox")
    })
    .await;
    // Alerts also arrive for another joined workspace, without switching the UI.
    let first_workspace = unread.workspace.clone().unwrap();
    alice
        .send(Command::CreateWorkspace {
            name: "Another workspace".into(),
            slug: "another-workspace".into(),
        })
        .unwrap();
    wait(&alice, |s| {
        s.workspace.as_ref() != Some(&first_workspace) && s.workspace.is_some()
    })
    .await;
    alice
        .send(Command::Invite("bob@example.com".into()))
        .unwrap();
    wait(&alice, |s| s.people.len() == 2).await;
    alice
        .send(Command::CreateChannel("other-channel".into()))
        .unwrap();
    let target = wait(&alice, |s| s.connected && s.channel.is_some()).await;
    send(&alice, "Alert from another workspace", None);
    let alert = wait(&bob, |s| {
        s.notifications
            .iter()
            .any(|n| n.message.body_md == "Alert from another workspace")
    })
    .await;
    assert_eq!(alert.workspace.as_ref(), Some(&first_workspace));
    let notification = alert
        .notifications
        .iter()
        .find(|n| n.message.body_md == "Alert from another workspace")
        .unwrap();
    bob.send(Command::OpenMessage {
        workspace_id: notification.workspace_id.clone(),
        channel_id: notification.message.channel_id.clone(),
        message_id: notification.message.id.clone(),
    })
    .unwrap();
    wait(&bob, |s| {
        s.workspace == target.workspace
            && s.thread_messages
                .iter()
                .any(|m| m.message.body_md == "Alert from another workspace")
    })
    .await;
    drop(other);
    drop(bob);
    drop(alice);
    task.abort();
}
