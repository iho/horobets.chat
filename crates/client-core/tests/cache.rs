use client_core::{OutboxStatus, cache::Cache};
use shared_proto::{ClientEvent, MessageDto, ServerEvent};
#[test]
fn atomic_sync_reconciles_outbox_and_keeps_workspace_search_isolated() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cache.db");
    let mut cache = Cache::open(&path).unwrap();
    let event = ClientEvent::SendMessage {
        client_msg_id: "message".into(),
        channel_id: "general".into(),
        parent_id: None,
        topic_tag: None,
        body_md: "optimistic café 🌍".into(),
        attachment_ids: vec![],
    };
    cache.enqueue("one", "alice", &event).unwrap();
    assert!(matches!(
        cache.messages("one", "general", 200, "alice").unwrap()[0].status,
        OutboxStatus::Pending
    ));
    let mut m = MessageDto {
        seq: 1,
        id: "message".into(),
        channel_id: "general".into(),
        user_id: "alice".into(),
        parent_id: None,
        topic_tag: None,
        body_md: "canonical SQLite message".into(),
        edited_at: None,
        deleted_at: None,
        created_at: 1,
    };
    cache
        .apply_page(
            "one",
            &[ServerEvent::MessageCreated {
                client_msg_id: Some("message".into()),
                message: m.clone(),
            }],
            7,
        )
        .unwrap();
    assert!(cache.next_pending("one").unwrap().is_none());
    assert_eq!(cache.cursor("one").unwrap(), 7);
    assert_eq!(
        cache
            .messages("one", "general", 200, "alice")
            .unwrap()
            .len(),
        1
    );
    assert_eq!(cache.search("one", "SQLite").unwrap().len(), 1);
    assert!(cache.search("two", "SQLite").unwrap().is_empty());
    m.body_md = "edited content".into();
    m.edited_at = Some(2);
    cache
        .apply_page("one", &[ServerEvent::MessageUpdated { message: m }], 8)
        .unwrap();
    assert!(cache.search("one", "SQLite").unwrap().is_empty());
    let reaction = ServerEvent::ReactionUpdated {
        channel_id: "general".into(),
        message_id: "message".into(),
        emoji: "👍".into(),
        user_id: "alice".into(),
        added: true,
    };
    cache
        .apply_page("one", &[reaction.clone(), reaction], 9)
        .unwrap();
    assert_eq!(
        cache.messages("one", "general", 200, "alice").unwrap()[0].reactions[0].count,
        1
    );
    cache
        .apply_page(
            "one",
            &[ServerEvent::MessageDeleted {
                channel_id: "general".into(),
                message_id: "message".into(),
                seq: 1,
            }],
            10,
        )
        .unwrap();
    assert!(cache.search("one", "edited").unwrap().is_empty());
    drop(cache);
    let cache = Cache::open(&path).unwrap();
    assert_eq!(cache.cursor("one").unwrap(), 10);
    assert!(
        cache.messages("one", "general", 200, "alice").unwrap()[0]
            .message
            .deleted_at
            .is_some()
    );
}
#[test]
fn server_urls_reject_credentials_and_non_http_schemes() {
    for url in [
        "file:///etc/passwd",
        "https://user:pass@example.com",
        "https://example.com/?token=secret",
    ] {
        assert!(client_core::sync::normalize_server(url).is_err());
    }
    assert_eq!(
        client_core::sync::normalize_server(" http://localhost:3000/ ").unwrap(),
        "http://localhost:3000"
    );
}

#[test]
fn long_threads_do_not_displace_roots_or_drop_thread_reactions() {
    let dir = tempfile::tempdir().unwrap();
    let mut cache = Cache::open(&dir.path().join("cache.db")).unwrap();
    let mut events = vec![];
    for n in 0..250 {
        let message = MessageDto {
            seq: n + 1,
            id: format!("m{n}"),
            channel_id: "general".into(),
            user_id: "alice".into(),
            parent_id: (n > 0).then(|| "m0".into()),
            topic_tag: None,
            body_md: format!("body {n}"),
            edited_at: None,
            deleted_at: None,
            created_at: n,
        };
        events.push(ServerEvent::MessageCreated {
            client_msg_id: None,
            message,
        });
    }
    events.push(ServerEvent::ReactionUpdated {
        channel_id: "general".into(),
        message_id: "m249".into(),
        emoji: "👍".into(),
        user_id: "alice".into(),
        added: true,
    });
    cache.apply_page("w", &events, 251).unwrap();
    assert_eq!(
        cache.messages("w", "general", 200, "alice").unwrap().len(),
        1
    );
    let thread = cache.thread("w", "m0", "alice").unwrap();
    assert_eq!(thread.len(), 250);
    assert_eq!(thread[0].message.id, "m0");
    assert_eq!(thread[249].reactions[0].count, 1);
}

#[test]
fn unread_receipts_are_monotonic_private_and_survive_offline_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("reads.db");
    let mut cache = Cache::open(&path).unwrap();
    cache.set_metadata("user_id", &"bob").unwrap();
    let channel = shared_proto::Channel {
        id: "general".into(),
        name: "general".into(),
        topic: None,
        is_private: false,
        is_direct: false,
        is_saved: false,
        peer_user_id: None,
        created_by: "alice".into(),
        created_at: 0,
    };
    cache.set_channels("w", &[channel]).unwrap();
    let m = MessageDto {
        seq: 5,
        id: "m".into(),
        channel_id: "general".into(),
        user_id: "alice".into(),
        parent_id: None,
        topic_tag: None,
        body_md: "Unread".into(),
        created_at: 1,
        edited_at: None,
        deleted_at: None,
    };
    cache
        .apply_page(
            "w",
            &[ServerEvent::MessageCreated {
                client_msg_id: None,
                message: m.clone(),
            }],
            5,
        )
        .unwrap();
    cache
        .apply_page(
            "w",
            &[ServerEvent::ReadStateUpdated {
                channel_id: "general".into(),
                user_id: "alice".into(),
                last_read_seq: 5,
            }],
            6,
        )
        .unwrap();
    assert_eq!(cache.unread_messages("w", "bob").unwrap(), vec![m.clone()]);
    assert!(cache.claim_notification("w", &m).unwrap());
    assert!(!cache.claim_notification("w", &m).unwrap());
    cache.mark_read("w", "general").unwrap();
    assert!(cache.unread_messages("w", "bob").unwrap().is_empty());
    drop(cache);
    let mut cache = Cache::open(&path).unwrap();
    assert_eq!(
        cache.pending_reads("w").unwrap(),
        vec![("general".into(), 5)]
    );
    assert!(!cache.claim_notification("w", &m).unwrap());
    cache
        .apply_page(
            "w",
            &[ServerEvent::ReadStateUpdated {
                channel_id: "general".into(),
                user_id: "bob".into(),
                last_read_seq: 3,
            }],
            7,
        )
        .unwrap();
    assert!(cache.unread_messages("w", "bob").unwrap().is_empty());
    assert!(!cache.pending_reads("w").unwrap().is_empty());
    cache
        .apply_page(
            "w",
            &[ServerEvent::ReadStateUpdated {
                channel_id: "general".into(),
                user_id: "bob".into(),
                last_read_seq: 5,
            }],
            8,
        )
        .unwrap();
    assert!(cache.pending_reads("w").unwrap().is_empty());
    cache.mark_read("w", "general").unwrap();
    cache.set_channels("w", &[]).unwrap();
    assert!(cache.pending_reads("w").unwrap().is_empty());
    assert!(cache.unread_messages("w", "bob").unwrap().is_empty());
}
