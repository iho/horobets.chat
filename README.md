# horobets.chat

A workspace messenger with a Rust GPUI desktop client and a single-binary backend built with Axum, Tokio, and bundled SQLite. Each workspace has its own WAL database, bounded single-writer actor, read-only connection pool, FTS5 index, and WebSocket broadcast hub. No database service is required.

## Website and CI

The dependency-free [project website](website/README.md) introduces the apps and links to self-hosting instructions. Preview it with `python3 -m http.server 8080 --directory website --bind 127.0.0.1`.

[GitHub Actions CI](.github/workflows/ci.yml) runs formatting, Rust core tests and Clippy on Linux, an optional S3 build check, desktop tests and Clippy on macOS, host Swift/UniFFI tests, and website link/asset checks. Native device builds, signing, and APNs/FCM delivery are outside CI. The separate **Deploy website** workflow publishes to GitHub Pages on manual dispatch after Pages is enabled; see the [hosting instructions](website/README.md#publish-with-github-pages).

## Run

```sh
export MESSENGER_JWT_SECRET="$(openssl rand -hex 32)"
cargo run -p messenger-server -- --data-dir ./data --listen 127.0.0.1:3000
```

Keep the same secret across restarts to preserve sessions. Tokens expire after seven days. The binary accepts `MESSENGER_DATA_DIR` and `MESSENGER_LISTEN` as alternatives to CLI arguments. `RUST_LOG=info` controls logging. SIGINT/SIGTERM stop the HTTP server gracefully.

On Unix, startup raises the process’s soft open-file limit to 8192 when the inherited hard limit allows it, and logs the effective limits. If it warns about a lower hard limit, increase your shell or service’s limit before starting the server. This budget covers network sockets and SQLite files.

```sh
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo build --release -p messenger-server
```

## Desktop client

```sh
cargo run -p messenger-desktop
```

Sign in to the running backend, then create a workspace and channel from the sidebar. The native client includes threaded Markdown conversations, reactions, editing, file upload/download, local full-text search (Cmd+K / Ctrl+K), and an offline SQLite outbox with reconnect recovery. See [desktop setup and controls](crates/desktop/README.md).

## Mobile apps

The Skip project in [mobile/](mobile/README.md) shares SwiftUI screens between iOS and Android Compose. `crates/mobile-core` exposes the existing Rust engine through UniFFI, including callback snapshots, SQLite history/search, reconnection, and the offline outbox.

```sh
./scripts/build-mobile.sh --all
# iOS: open mobile/Project.xcworkspace in Xcode
# Android (with ANDROID_HOME set):
gradle -p mobile/Android assembleDebug
```

See [mobile setup, supported features, and validation](mobile/README.md) for toolchain installation, simulator server addresses, native build artifacts, and the initial mobile client's limits.

## API

All paths below start with `/api/v1`. Except registration and login, requests require `Authorization: Bearer <token>`, including the WebSocket upgrade. `/health` is public.

| Method | Path | Body / query |
| --- | --- | --- |
| POST | `/auth/register` | `email`, `password` (12–1024 bytes), optional `display_name` |
| POST | `/auth/login` | `email`, `password`; returns `token`, `user_id` |
| GET, PATCH | `/users/me` | Current profile; update `display_name` and optional unique `handle` |
| POST | `/users/me/avatar` | Multipart raster image, up to 5 MiB |
| GET | `/users/{u}/avatar/{key}` | Avatar; self or shared workspace member |
| GET | `/workspaces/{w}/attachments/{a}/preview` | Authorized server-generated PNG thumbnail |
| GET, POST | `/workspaces` | Create: `slug`, `name` |
| GET, POST | `/workspaces/{w}/members` | `email` of registered user; owner/admin only |
| GET, POST | `/workspaces/{w}/channels` | Create: `name`, optional `topic`, `is_private` |
| POST | `/workspaces/{w}/directs` | `user_id` of another workspace member; creates/reuses a private two-person conversation |
| GET, POST | `/workspaces/{w}/channels/{c}/members` | List members / add `user_id`; creator or an invited workspace admin manages access |
| DELETE | `/workspaces/{w}/channels/{c}/members/{u}` | Remove a member; direct-message participants cannot change |
| GET | `/workspaces/{w}/channels/{c}/messages` | `before_seq`, `limit` (max 200), newest first |
| GET | `/workspaces/{w}/search` | `q` (literal phrase with stemming), `limit` |
| GET | `/workspaces/{w}/sync` | `since_seq`, `limit` (max 1000) |
| GET, POST | `/workspaces/{w}/attachments` | Upload: multipart file; request limit 10 MiB. List: `channel_id`, `since_seq` |
| GET | `/workspaces/{w}/attachments/{a}` | Authenticated download |
| GET | `/workspaces/{w}/ws` | WebSocket upgrade |

Workspace members can access public channels. Private channels allow invited members; the creator can manage membership. Invited workspace owners/admins can also manage membership through the API. Direct messages always have exactly two workspace members, and only those two can read, search, sync, or subscribe to their messages. Unattached uploads are accessible only to the uploader; attached files follow channel permissions. Downloads use attachment disposition and `nosniff`.

Example:

```sh
curl -s localhost:3000/api/v1/auth/register \
  -H 'Content-Type: application/json' \
  -d '{"email":"alice@example.com","password":"correct horse battery staple"}'
# Use the returned token:
curl -s localhost:3000/api/v1/workspaces \
  -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -d '{"slug":"engineering","name":"Engineering"}'
```

## Profiles, handles, and appearance

Open **Settings** in the desktop sidebar to set your display name and unique `@handle`, change your profile picture, and select Dark or Light appearance. Handles are case-insensitive, normalized to lowercase, and contain 3–32 ASCII letters, digits, or underscores starting with a letter. Clearing a handle releases it. Appearance is saved per device; profiles and handles are stored on the server. Successful profile and photo updates immediately notify connected clients in every shared workspace, including your other devices; reconnect refreshes missed updates. Existing accounts and caches migrate automatically without losing history.

Use **+** beside Channels and choose Private, then **Members** in the channel header to add teammates. Use **+** beside Direct Messages to search workspace teammates by name or handle. Repeated requests for the same pair reuse the existing DM.

## WebSocket and reconnect

Catalog changes emit an ephemeral `channels_changed` frame. Clients reload their channel catalog, purge messages/outbox/search for channels they can no longer access, and replay the event log when access to a channel is newly granted.

Wire enums live in `crates/shared-proto`; frames use `{"type":"send_message","payload":{...}}`. All message, edit, delete, reaction, read-state, typing, and ping client variants are handled. Committed mutations are broadcast in writer order. Message authors alone can edit/delete their messages. Rejected sends include an optional `client_msg_id` in the Error frame so clients can identify the failed outbox item. Send IDs are idempotent within a workspace; retrying acknowledges the canonical row without inserting or broadcasting again. Clients should generate a UUIDv7/ULID `client_msg_id`.

```json
{"type":"send_message","payload":{"client_msg_id":"019aa111-1111-7111-8111-111111111111","channel_id":"CHANNEL_ID","parent_id":null,"topic_tag":"backend","body_md":"Hello!","attachment_ids":[]}}
```

Subscribe before backfilling and deduplicate by message/event identity. `/sync` returns `events: [{seq, event}]`, `next_seq`, and `has_more`; continue until `has_more` is false and persist `next_seq`. **The sync cursor belongs to the durable event log, not to `messages.seq`.** A message insertion sequence alone cannot capture later edits/deletions/reactions. `Hello.latest_seq` reports the event cursor; do not skip directly to it before backfilling. Private-channel events are filtered while still advancing the returned cursor. Typing and ping are ephemeral. A lagging WebSocket receives `resync_required` and disconnects; reconnect and backfill.

JSON round-trip tests cover every protocol variant. The optional `shared-proto/messagepack` feature provides the MessagePack dependency for future client negotiation; WebSockets currently accept JSON only.

## Storage

Local files, inline image previews, and profile pictures default to `data/blobs`; no external storage service is needed. Profile photos are normalized to square 256px PNGs; message image previews fit within 1024×768. Raster uploads are decoded with dimension and allocation limits. Originals remain downloadable. Build S3/R2 support with:

```sh
cargo build --release -p messenger-server --features s3
# Set AWS_BUCKET, AWS_REGION, AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY.
# For R2 also set AWS_ENDPOINT to the account's HTTPS endpoint and AWS_REGION=auto.
./target/release/messenger-server --blob-store s3
```

S3 configuration uses the [object_store AWS environment configuration](https://docs.rs/object_store/latest/object_store/aws/struct.AmazonS3Builder.html#method.from_env). Local storage and API behavior are integration tested; S3/R2 requires an external bucket for runtime validation.

## Scope

This implements the backend, shared protocol, offline client engine, GPUI desktop client, and initial Skip iOS/Android client with UniFFI bindings. Magic links, presence tracking, session revocation, and upload garbage collection are not implemented. Native notification delivery and the unread inbox are implemented; mobile background delivery requires operator-configured APNs/FCM credentials. The current manager retains opened workspaces until shutdown; idle eviction is a future scaling improvement. FTS latency and message throughput have not been benchmarked. Use one server process per data directory; actor serialization is in-process.

## Notifications and unread inbox

Desktop and mobile provide **Unread messages**, cross-device read marks, focus-aware alerts, and notification routing across joined workspaces. Enable notifications in client Settings. For the macOS app bundle, Android Firebase setup, iOS provisioning and server APNs/FCM credentials, see [notification setup](docs/notifications.md).
