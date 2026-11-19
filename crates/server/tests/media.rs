use messenger_server::{App, db::queries};
use reqwest::{
    Client,
    multipart::{Form, Part},
};
use serde_json::{Value, json};
use shared_proto::ClientEvent;
fn png() -> Vec<u8> {
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        32,
        16,
        image::Rgba([30, 160, 130, 255]),
    ))
    .write_to(&mut bytes, image::ImageFormat::Png)
    .unwrap();
    bytes.into_inner()
}
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
fn form(bytes: Vec<u8>) -> Form {
    Form::new().part("file", Part::bytes(bytes).file_name("photo.png"))
}
#[tokio::test]
async fn local_profile_and_message_images_are_persisted_and_authorized() {
    let dir = tempfile::tempdir().unwrap();
    let app = App::open(
        dir.path().to_owned(),
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
    let http = Client::new();
    let alice = account(&http, &base, "alice@example.com").await;
    let bob = account(&http, &base, "bob@example.com").await;
    let eve = account(&http, &base, "eve@example.com").await;
    let at = alice["token"].as_str().unwrap();
    let bt = bob["token"].as_str().unwrap();
    let et = eve["token"].as_str().unwrap();
    let profile: Value = http
        .post(format!("{base}/api/v1/users/me/avatar"))
        .bearer_auth(at)
        .multipart(form(png()))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let avatar_url = profile["avatar_url"].as_str().unwrap();
    let response = http
        .get(format!("{base}{avatar_url}"))
        .bearer_auth(at)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    assert_eq!(response.headers()["content-type"], "image/png");
    let image = image::load_from_memory(&response.bytes().await.unwrap()).unwrap();
    assert_eq!((image.width(), image.height()), (256, 256));
    assert_eq!(
        http.get(format!("{base}{avatar_url}"))
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        http.get(format!("{base}{avatar_url}"))
            .bearer_auth(bt)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        http.post(format!("{base}/api/v1/users/me/avatar"))
            .bearer_auth(at)
            .multipart(form(b"<svg>not a raster image</svg>".to_vec()))
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    let w: Value = http
        .post(format!("{base}/api/v1/workspaces"))
        .bearer_auth(at)
        .json(&json!({"slug":"media","name":"Media"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let wid = w["id"].as_str().unwrap();
    let root = format!("{base}/api/v1/workspaces/{wid}");
    http.post(format!("{root}/members"))
        .bearer_auth(at)
        .json(&json!({"email":"bob@example.com"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    assert_eq!(
        http.get(format!("{base}{avatar_url}"))
            .bearer_auth(bt)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    assert_eq!(
        http.get(format!("{base}{avatar_url}"))
            .bearer_auth(et)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    let ch: Value = http
        .post(format!("{root}/channels"))
        .bearer_auth(at)
        .json(&json!({"name":"photos"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let attachment: Value = http
        .post(format!("{root}/attachments"))
        .bearer_auth(at)
        .multipart(form(png()))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(attachment["mime_type"], "image/png");
    let aid = attachment["id"].as_str().unwrap().to_owned();
    let preview = format!("{root}/attachments/{aid}/preview");
    assert_eq!(
        http.get(&preview)
            .bearer_auth(bt)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    let channel = ch["id"].as_str().unwrap().to_owned();
    let user = alice["user_id"].as_str().unwrap().to_owned();
    let attachment_id = aid.clone();
    app.db
        .workspace(wid)
        .await
        .unwrap()
        .write(move |c| {
            queries::apply(
                c,
                &user,
                ClientEvent::SendMessage {
                    client_msg_id: messenger_server::id(),
                    channel_id: channel,
                    parent_id: None,
                    topic_tag: None,
                    body_md: String::new(),
                    attachment_ids: vec![attachment_id],
                },
            )
        })
        .await
        .unwrap();
    let response = http
        .get(&preview)
        .bearer_auth(bt)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    assert_eq!(response.headers()["content-type"], "image/png");
    assert!(image::load_from_memory(&response.bytes().await.unwrap()).is_ok());
    assert_eq!(
        http.get(&preview)
            .bearer_auth(et)
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(http.get(&preview).send().await.unwrap().status(), 401);
    let original = http
        .get(format!("{root}/attachments/{aid}"))
        .bearer_auth(bt)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .bytes()
        .await
        .unwrap();
    assert_eq!(original.to_vec(), png());
    // Blob files and avatar metadata survive opening a new server instance.
    let reopened = App::open(
        dir.path().to_owned(),
        "a-secret-with-more-than-thirty-two-bytes",
    )
    .unwrap();
    let key = avatar_url.rsplit('/').next().unwrap();
    assert!(!reopened.blobs.get(key).await.unwrap().is_empty());
    assert!(dir.path().join("blobs").join(&aid).is_file());
    let replacement: Value = http
        .post(format!("{base}/api/v1/users/me/avatar"))
        .bearer_auth(at)
        .multipart(form(png()))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_ne!(replacement["avatar_url"], profile["avatar_url"]);
    assert_eq!(
        http.get(format!("{base}{avatar_url}"))
            .bearer_auth(at)
            .send()
            .await
            .unwrap()
            .status(),
        404
    );
    task.abort();
}
#[test]
fn image_decoder_rejects_dimensions_over_the_limit() {
    let image = image::DynamicImage::ImageRgb8(image::RgbImage::new(8193, 1));
    let mut bytes = std::io::Cursor::new(Vec::new());
    image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    assert!(messenger_server::media::thumbnail(&bytes.into_inner(), true).is_err());
}
