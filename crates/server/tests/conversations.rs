use reqwest::Client;
use serde_json::{Value, json};
async fn account(http: &Client, base: &str, email: &str) -> Value {
    http.post(format!("{base}/api/v1/auth/register"))
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
async fn post(http: &Client, url: &str, token: &str, body: Value) -> Value {
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
#[tokio::test]
async fn handles_private_membership_and_pairwise_directs() {
    let temp = tempfile::tempdir().unwrap();
    let app = messenger_server::App::open(
        temp.path().to_owned(),
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
    let http = Client::new();
    let a = account(&http, &base, "alice@example.com").await;
    let b = account(&http, &base, "bob@example.com").await;
    let c = account(&http, &base, "carol@example.com").await;
    let outside = account(&http, &base, "outside@example.com").await;
    let at = a["token"].as_str().unwrap();
    let bt = b["token"].as_str().unwrap();
    let ct = c["token"].as_str().unwrap();
    let aid = a["user_id"].as_str().unwrap();
    let bid = b["user_id"].as_str().unwrap();
    let cid = c["user_id"].as_str().unwrap();
    let me = format!("{base}/api/v1/users/me");
    let profile: Value = http
        .patch(&me)
        .bearer_auth(at)
        .json(&json!({"display_name":"Ihor","handle":"@IHOR"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(profile["handle"], "ihor");
    let response = http
        .patch(&me)
        .bearer_auth(bt)
        .json(&json!({"display_name":"must not save","handle":"Ihor"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
    let unchanged: Value = http
        .get(&me)
        .bearer_auth(bt)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_ne!(unchanged["display_name"], "must not save");
    assert_eq!(
        http.patch(&me)
            .bearer_auth(bt)
            .json(&json!({"display_name":"Bob","handle":"bad handle"}))
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    let w = post(
        &http,
        &format!("{base}/api/v1/workspaces"),
        at,
        json!({"slug":"team","name":"Team"}),
    )
    .await;
    let root = format!("{base}/api/v1/workspaces/{}", w["id"].as_str().unwrap());
    for email in ["bob@example.com", "carol@example.com"] {
        post(
            &http,
            &format!("{root}/members"),
            at,
            json!({"email":email}),
        )
        .await;
    }
    let private = post(
        &http,
        &format!("{root}/channels"),
        at,
        json!({"name":"private","is_private":true}),
    )
    .await;
    let pid = private["id"].as_str().unwrap();
    let members = format!("{root}/channels/{pid}/members");
    assert_eq!(
        http.get(&members)
            .bearer_auth(bt)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    post(&http, &members, at, json!({"user_id":bid})).await;
    let joined: Vec<String> = http
        .get(&members)
        .bearer_auth(bt)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(joined.contains(&bid.to_owned()));
    assert_eq!(
        http.post(&members)
            .bearer_auth(bt)
            .json(&json!({"user_id":cid}))
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        http.post(&members)
            .bearer_auth(at)
            .json(&json!({"user_id":outside["user_id"]}))
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    http.delete(format!("{members}/{bid}"))
        .bearer_auth(at)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    assert_eq!(
        http.get(format!("{root}/channels/{pid}/messages"))
            .bearer_auth(bt)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        http.delete(format!("{members}/{aid}"))
            .bearer_auth(at)
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    // Opposite requests race but resolve to exactly one conversation.
    let direct_url = format!("{root}/directs");
    let (da, db) = tokio::join!(
        post(&http, &direct_url, at, json!({"user_id":bid})),
        post(&http, &direct_url, bt, json!({"user_id":aid}))
    );
    assert_eq!(da["id"], db["id"]);
    assert_eq!(da["peer_user_id"], bid);
    assert_eq!(db["peer_user_id"], aid);
    assert_eq!(da["is_direct"], true);
    assert_eq!(da["is_private"], true);
    let dm = da["id"].as_str().unwrap();
    assert_eq!(
        http.get(format!("{root}/channels/{dm}/messages"))
            .bearer_auth(ct)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        http.post(format!("{root}/channels/{dm}/members"))
            .bearer_auth(at)
            .json(&json!({"user_id":cid}))
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    assert_eq!(
        http.delete(format!("{root}/channels/{dm}/members/{bid}"))
            .bearer_auth(at)
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    let list: Vec<Value> = http
        .get(format!("{root}/channels"))
        .bearer_auth(ct)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(!list.iter().any(|ch| ch["id"] == da["id"]));
    assert_eq!(
        http.post(format!("{root}/directs"))
            .bearer_auth(at)
            .json(&json!({"user_id":outside["user_id"]}))
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    task.abort();
}
