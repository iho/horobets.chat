# horobets.chat for iOS and Android

A Skip Lite app: shared SwiftUI becomes SwiftUI on iOS and Jetpack Compose on Android. Both platforms call the existing Rust `client-core` through UniFFI 0.32.2; the mobile UI does not implement its own REST/WebSocket sync, cache, search, or outbox.

## Build prerequisites

Use macOS with Xcode and its command-line tools. The project targets iOS 17+, Android API 28+, and Apple Silicon/Intel iOS Simulators. Android libraries are built for arm64-v8a and x86_64. Rust must be installed through rustup.

```sh
brew install skiptools/skip/skip
skip checkup --plain
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios aarch64-linux-android x86_64-linux-android
cargo install cargo-ndk --locked
```

Install Android Studio/SDK, a compatible JDK, and NDK 29.0.14206865. Accept the Android SDK licenses in Android Studio. For the default macOS SDK location:

```sh
export ANDROID_HOME="$HOME/Library/Android/sdk"
sdkmanager --sdk_root="$ANDROID_HOME" 'ndk;29.0.14206865'
```

Skip's Swift package is pinned to 1.9.21; `Package.resolved` pins the framework dependencies. Native bindings and libraries are generated artifacts, so build them before opening/resolving the mobile package on a fresh checkout.

## Build both apps

From the repository root:

```sh
./scripts/build-mobile.sh --all
```

This generates matching Swift/Kotlin bindings from the host Rust library, creates `mobile/Bridge/MessengerCoreFFI.xcframework` with macOS, iPhone and universal simulator slices, and places Android `.so` files in `mobile/Android/app/src/main/jniLibs/`. Skip copies the generated Kotlin file beside `skip.yml` into its Android module. Android uses JNA to invoke UniFFI, and includes keep rules for release minification.

For Apple only use `--apple`; for Android only use `--android`. `--host` builds a macOS-only framework for fast Swift/bridge testing; run `--apple` again before building iOS after using `--host`. Debug native libraries are currently produced; an optimized, signed store release is a separate step.

### iOS

Open `mobile/Project.xcworkspace`, select **MessengerApp App**, and run on an iPhone Simulator. For a physical iPhone, select your signing team in Xcode.

Unsigned simulator build from the repository root:

```sh
xcodebuild -project mobile/Darwin/MessengerApp.xcodeproj \
  -scheme 'MessengerApp App' -sdk iphonesimulator \
  -destination 'generic/platform=iOS Simulator' \
  -derivedDataPath target/mobile-ios -skipPackagePluginValidation \
  CODE_SIGNING_ALLOWED=NO SKIP_ACTION=none build
```

`SKIP_ACTION=none` skips the optional simultaneous Android launch; the SwiftPM Skip plugin still generates Kotlin. The built app is under `target/mobile-ios/Build/Products/Debug-iphonesimulator/`.

### Android

```sh
export ANDROID_HOME="$HOME/Library/Android/sdk"
gradle -p mobile/Android assembleDebug --console=plain
```

Open `mobile/Android` in Android Studio to run on an emulator or device. Skip redirects Gradle build output under `mobile/.build/Android/app/outputs/apk/debug/`. Use `adb install -r <path-to-apk>` to install manually. Do not commit `local.properties`, generated native binaries, signing keys, or `keystore.properties`.

## Connecting to your server

Start the existing backend, enter its HTTP/HTTPS URL on the sign-in screen, and use an existing account or register. Prefer HTTPS for a real deployment. The mobile app and desktop share accounts, workspaces, private channels, direct conversations and server-local storage.

For development:

- iOS Simulator can reach the Mac at `http://127.0.0.1:3000`.
- Android Emulator reaches the Mac at `http://10.0.2.2:3000`.
- A physical phone needs a reachable LAN/VPN/HTTPS server address. `127.0.0.1` refers to the phone itself. Bind the development backend to a reachable interface when using a physical device.

The app starts with an editable `https://` field and never embeds a deployment hostname or credential.

## Mobile and desktop feature coverage

The mobile UI now consumes the full Rust snapshot, including cached media and management state. Images and avatars load from authenticated files cached by Rust, with no public upload host or signed image URL. Cached previews remain available offline. Opening an image shows a larger preview; saving downloads the original through the authenticated Rust client.

| Feature | Mobile implementation |
| --- | --- |
| Images and profile pictures | Inline cached previews, image viewer, cached avatars, photo-library and document import |
| Files | Pending attachment chips and previews, removal before send, image-only messages, authenticated original download/save |
| Channels and DMs | Workspace selection, public/private creation, fixed-participant DMs, private saved messages, unread counts |
| Private membership | Creator/authorized-admin add/remove controls, preserving the server's access rules |
| Workspace management | Rename workspace/slug, edit accessible channels/topics, invite registered teammates, assign/remove moderators |
| Message management | Own-message editing, confirmed deletions, role-aware pin/unpin and moderator delete actions |
| Dedicated pages | Pinned messages, moderation activity, people/roles, user profiles, settings |
| Conversation UI | Compact channel header, avatars, date separators, grouped messages, contextual actions, thread sheets, recent-message navigation |
| Reactions | All 32 desktop choices and highlighted existing reaction counts |
| Markdown | Formatting snippets and preview; headings, quotes, lists/tasks, tables, fenced code, inline formatting and links |
| Mentions and channel references | Filtered suggestions while typing @/# prefixes, plus tappable profile/channel links; code and existing Markdown links remain literal |
| Search, topics and drafts | Local full-text search with result navigation, optional topics, per-conversation in-session drafts, /join channel-name |
| Offline behavior | Shared Rust cache, durable outbox, queued/failed delivery state, retry, reconnect/backfill |
| Profile and appearance | Name, handle, biography, photo import, user profile view, dark/light mode; persistent edit drafts and server-confirmed save/error feedback |

Native Android document pickers copy content URIs into the app's private cache before handing a real path to Rust. Downloads use Android's Save dialog. iOS uses the photo/document pickers and native save/share interface. No broad storage permission is requested. Mobile uses touch menus and navigation rather than desktop pane resizing and keyboard shortcuts. Markdown formatting controls insert editable snippets; typing suggestions operate on the current trailing @/# token. Android also exposes **Copy message** because Skip's text-selection modifier is not available in the pinned release.

Push delivery, presence, and WYSIWYG editing are not implemented by either client. APNs/FCM credentials and store signing/distribution are not configured.

## Data and lifecycle

`CoreAdapter.shared` owns one process-wide networking engine; rebuilding the root view reuses it instead of opening extra clients. `CoreAdapter` is the main-actor ObservableObject. `CoreListener` receives a coalesced JSON snapshot on a Rust worker thread and dispatches it to the main actor. Commands are tagged JSON decoded into the same Rust `Command` enum used by GPUI. A single native engine owns the SQLite cache, WebSocket reconnect logic and optimistic outbox. The bridge rejects duplicate listeners and supports explicit shutdown without a polling loop.

Data lives in a private application-support subdirectory. Passwords are not persisted. Like the desktop, the current Rust core saves its bearer token in a mode-0600 session file rather than the OS keychain; cached SQLite history is not encrypted and remains after logout. Android backup is disabled to avoid backing up those credentials and conversations. The process cannot maintain a permanent socket while the OS suspends the app; reconnect uses the existing sync cursor on resume.

## Validation

```sh
cargo test -p messenger-mobile -p client-core -p messenger-server
cargo clippy -p messenger-mobile -p client-core --all-targets --all-features -- -D warnings
swift test --package-path mobile
```

Rust bridge tests exercise callbacks, invalid commands, duplicate listener rejection, shutdown, live registration/workspace creation/message/reply exchange, authenticated avatar/attachment upload, cached image previews, byte-for-byte original downloads, and offline history/search restoration. Swift tests load the real Rust library and check media decoding, mention completion/link parsing, Markdown structure, empty image paths, and role permissions. Building with Skip transpiles the shared source; the Gradle APK build also checks the resulting Kotlin.

Android emulator checks against an isolated live backend cover sign-in, cached images and avatars, opening/closing the image viewer, profile navigation, mention completion, pinning and the pins page, native photo-library and document import, image-only sending, and saving an original through the native document picker with matching file bytes. Private-channel membership and moderator actions were also exercised, with the changes confirmed in the isolated server database. Profile form checks edit the name and handle, rotate the screen before saving, verify database persistence, reopen settings, reject a taken handle without losing the draft, retry successfully, and confirm saved values after app restart. iOS has been simulator-build validated; physical-device signing and an iOS interactive run are still outstanding.
