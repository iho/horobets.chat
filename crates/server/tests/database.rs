use messenger_server::{
    db::{manager::Manager, queries},
    id,
};
use shared_proto::ClientEvent;
#[tokio::test]
async fn concurrent_writes_are_serialized_and_persisted() {
    let temp = tempfile::tempdir().unwrap();
    let manager = std::sync::Arc::new(Manager::open(temp.path()).unwrap());
    let workspace = id();
    // Race first access: all handles must share one writer and broadcast hub.
    let mut opens = tokio::task::JoinSet::new();
    for _ in 0..16 {
        let manager = manager.clone();
        let w = workspace.clone();
        opens.spawn(async move { manager.workspace(&w).await.unwrap() });
    }
    let mut handles = vec![];
    while let Some(db) = opens.join_next().await {
        handles.push(db.unwrap());
    }
    let db = handles[0].clone();
    let _rx = db.events.subscribe();
    assert!(handles.iter().all(|h| h.events.receiver_count() == 1));
    db.write(|c| {
        c.execute(
            "INSERT INTO channels VALUES('general','general',NULL,0,'alice',0)",
            [],
        )?;
        Ok(())
    })
    .await
    .unwrap();
    let mut writes = tokio::task::JoinSet::new();
    for n in 0..100 {
        let db = handles[n % handles.len()].clone();
        writes.spawn(async move {
            db.write(move |c| {
                queries::apply(
                    c,
                    "alice",
                    ClientEvent::SendMessage {
                        client_msg_id: id(),
                        channel_id: "general".into(),
                        parent_id: None,
                        topic_tag: None,
                        body_md: format!("concurrent message {n}"),
                        attachment_ids: vec![],
                    },
                )
            })
            .await
            .unwrap()
        });
    }
    while let Some(r) = writes.join_next().await {
        r.unwrap();
    }
    let counts = db
        .read(|c| {
            Ok(
                c.query_row("SELECT COUNT(*),MAX(seq) FROM messages", [], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
                })?,
            )
        })
        .await
        .unwrap();
    assert_eq!(counts, (100, 100));
    // Invalid attachment must roll back the message, FTS entry, and event.
    assert!(
        db.write(|c| queries::apply(
            c,
            "alice",
            ClientEvent::SendMessage {
                client_msg_id: "invalid".into(),
                channel_id: "general".into(),
                parent_id: None,
                topic_tag: None,
                body_md: "rollback sentinel".into(),
                attachment_ids: vec!["missing".into()]
            }
        ))
        .await
        .is_err()
    );
    let counts=db.read(|c|Ok(c.query_row("SELECT (SELECT COUNT(*) FROM messages),(SELECT COUNT(*) FROM messages_fts),(SELECT COUNT(*) FROM events)",[],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,r.get::<_,i64>(2)?)))?)).await.unwrap();
    assert_eq!(counts, (100, 100, 100));
    // A new manager reads the persisted workspace and event cursor after restart.
    let reopened = Manager::open(temp.path()).unwrap();
    let db = reopened.workspace(&workspace).await.unwrap();
    let count = db
        .read(|c| Ok(c.query_row("SELECT COUNT(*) FROM events", [], |r| r.get::<_, i64>(0))?))
        .await
        .unwrap();
    assert_eq!(count, 100);
}
