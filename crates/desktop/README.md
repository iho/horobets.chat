# Native desktop client

A Rust GPUI app connected to the workspace messenger server. Uses pinned GPUI 0.2.2 and the compatible GPUI Component 0.5.1 release for text editing, IME, Markdown, and native file dialogs. There is no browser or webview in the client.

## Run

Start the current backend from the repository root:

```sh
export MESSENGER_JWT_SECRET="$(openssl rand -hex 32)"
cargo run -p messenger-server
```

In a second terminal:

```sh
cargo run -p messenger-desktop
```

The sign-in screen defaults to `http://127.0.0.1:3000`; enter your own HTTP/HTTPS server URL if needed. Create an account (12+ character password), create a workspace with a lowercase slug, then add a channel using the sidebar's **+** buttons. Invite other registered accounts by email as the workspace owner/admin.

On macOS, GPUI requires Xcode and its Metal toolchain. If the compiler reports a missing `metal` executable, run `xcodebuild -downloadComponent MetalToolchain`. See [GPUI platform prerequisites](https://docs.rs/gpui/0.2.2/gpui/#dependencies) for supported systems. macOS is built and launch-tested here; Linux is not validated in this change, and Windows is outside the published GPUI release's supported platforms.

To produce a Finder-launchable macOS bundle:

```sh
./scripts/package-desktop-macos.sh --debug
open target/debug/horobets.chat.app
# For an optimized build:
./scripts/package-desktop-macos.sh --release
```

The bundle is unsigned; distribution signing/notarization is not included.

## Controls

- **Enter** sends; **Shift+Enter** inserts a newline. Channel drafts survive channel switching during a session.
- **Cmd+K / Ctrl+K** opens the local full-text search and channel switcher. Use arrows and Enter, or click a result. Search results open the full message/thread, including older cached history.
- **Reply in thread** opens the 360px thread pane; **Esc** closes a modal or thread.
- React with 👍 or toggle an existing emoji pill. Authors can edit/delete their messages.
- Add an optional topic in the field above the channel composer.
- **Attach file** opens the OS picker; dropping files onto a composer uploads them. Click a file chip's × before sending to remove it. Files must be under 10 MiB. Download buttons use an authenticated request and a native Save dialog.
- `/join channel-name` switches to an existing channel.
- **Load earlier messages** grows the cached feed window by 200 root messages.
- **Refresh** reloads workspace/channel/member metadata; **Sign out** removes the saved session token.

Drag the divider beside the channel sidebar to resize it (180–420px), or the divider beside an open thread to resize that pane (260–600px). Widths are saved in `layout.json` in the app data directory. The sidebar starts at 240px. Message lists use GPUI's variable-height virtual list so multiline Markdown and code are not clipped to a fixed row height; search results use `uniform_list`.

## Profile pictures and inline images

Click **Photo** beside your name at the bottom of the sidebar to upload a profile picture (PNG, JPEG, WebP, or GIF, up to 5 MiB). The server saves a square 256px PNG avatar. Photos appear beside authors' messages; other connected clients refresh profile images within about 30 seconds, or immediately with Refresh.

Use **Attach file** or drag an image onto the composer. A preview appears before sending and inline in the conversation. Images can be sent without a text caption. The original remains downloadable. Previews use the first frame of animated images.

All originals, thumbnails, and avatars use the server's **local directory by default**, `<data-dir>/blobs` (`data/blobs` with default settings). No AWS/R2 account, external image host, or additional service is required. Access to avatars and previews is authenticated. SQLite migrations run automatically on the existing data directory when the updated server starts. Restart both the server and desktop to use these features.

## Offline behavior and data

`client-core` owns a Tokio runtime and SQLite database on a dedicated worker thread. GPUI receives coalesced state snapshots through an async channel. HTTP, WebSockets, and SQLite run off the UI thread.

Sending inserts a local optimistic message and outbox entry in one transaction. The engine sends one outbox item at a time and removes it on canonical acknowledgment. Reconnect uses exponential backoff (1–30 seconds), a heartbeat, and the server's durable event cursor. WebSocket frames trigger authoritative HTTP delta sync so queued older frames cannot overwrite newer edits. Cache changes and cursor advancement commit together. Retries keep the original message ID, preventing duplicate server inserts. Server rejections show a failed state with Retry; only a matching `client_msg_id` rejection marks an outbox item failed.

The initial sync downloads the accessible event history; rendering starts with the latest 200 root messages. Replies and reactions remain in the same local cache. Search uses local FTS5, including while offline. A saved session allows reopening cached conversations and queuing sends while the server is unreachable. Files, workspace changes, edits, and reactions require a reachable server.

Data is stored in the OS per-user application-data directory, scoped by server and user. Override the location with `MESSENGER_CLIENT_DATA_DIR`. On Unix, the app data directory uses mode `0700` and the session file `0600`. Passwords are never saved. The bearer token is saved as a local file, not in the OS keychain; offline history remains on disk after signing out. SQLite data is not encrypted.

## Validation and limits

```sh
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Client tests cover atomic optimistic reconciliation, FTS changes/deletions, workspace isolation, a two-user live conversation with threads/reactions, and an offline outbox surviving an app restart before reconnecting exactly once. The backend additionally tests concurrent writers, private-channel permissions, file access, and message ownership.

Direct-message channels, notification delivery, presence indicators, and rich-text WYSIWYG editing depend on further product/backend work. The connection pill reports transport state, not user presence. UniFFI/mobile exports are a later milestone. No 120 FPS or search-latency benchmark is claimed.
