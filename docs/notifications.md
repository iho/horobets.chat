# Notifications and unread messages

The desktop and mobile clients have an **Unread messages** view for the selected workspace, with message previews, thread links, per-conversation read controls and **Mark all as read**. The inbox stays unread while you browse it. Deleted and inaccessible messages are excluded. It shows the latest 200 unread previews; conversation counts include the full cached history.

Read cursors advance only while the app is active and a conversation is open, or through an explicit mark-read action. Local read marks survive offline restarts and synchronize with your other devices after reconnecting. Marking a conversation read includes its thread replies. Notifications exclude your own messages, suppress the conversation being read, and use message IDs to prevent duplicate alerts. History downloaded on first sign-in does not generate alerts.

The account-wide WebSocket stream delivers alerts across joined workspaces while a client is running. The server checks current workspace and channel access on every alert. Mobile background delivery uses APNs on iOS and FCM on Android; normal mobile apps cannot keep the chat socket running indefinitely when suspended. Notification taps check the signed-in account and current access before opening the target message/thread.

## macOS desktop

Build the app bundle, then open it and choose **Settings → Enable notifications**:

```sh
scripts/build-desktop-app.sh
open target/debug/horobets.chat.app
```

Add `--release` to create `target/release/horobets.chat.app`. The script includes the Swift Notification Center agent and applies an ad-hoc development signature. Distribution signing/notarization is separate. The app needs to remain running, including in the background. A standalone `cargo run` binary is useful for development but has no macOS app identity; notification settings explain how to build the bundle. Notification clicks are handled while the app and its notification agent are running. Other desktop operating systems do not yet have a native notification agent.

## Android

Foreground/local alerts work after **Settings → Enable notifications**, including without Firebase configuration. Android 13+ also asks for the system notification permission. Background delivery needs a Firebase project containing an Android application whose package name is `chat.horobets.mobile`.

Copy `mobile/Android/firebase.example.xml` to `mobile/Android/app/src/main/res/values/firebase.xml` and fill in the values from that application's `google-services.json`. The actual XML file is ignored by Git. Rebuild/install the APK. Firebase Messaging is initialized from these string resources; a Google Services Gradle plugin is not required. Messaging auto-initialization and analytics collection are disabled by default; registration is requested when notifications are enabled.

Enable the Firebase Cloud Messaging HTTP v1 API and provide the server a service-account JSON file authorized to send messages to that project:

```sh
export MESSENGER_FCM_SERVICE_ACCOUNT=/absolute/private/path/firebase-service-account.json
```

The app registers and refreshes its FCM token automatically. If Firebase rotates a token in a cold process, an Android JobScheduler task uses the Rust core to update the registration. FCM messages carry data only, so the native service checks the current account and notification preference before displaying an alert. Account changes replace device ownership; disabling notifications/signing out unregisters the device and clears local alerts. Firebase project configuration in the app is distinct from the private service-account key, which belongs only on the server.

## iOS

Use an Apple Developer team and enable the Push Notifications capability for the app identifier `chat.horobets.mobile`. Sign with a matching provisioning profile. The app includes the `aps-environment` entitlement, controlled by the `APS_ENVIRONMENT` Xcode setting (`development` by default; use `production` for production provisioning). The same setting is included in Info.plist so token registration selects the corresponding APNs endpoint.

Create an APNs token-signing key and provide these server variables:

```sh
export MESSENGER_APNS_KEY_FILE=/absolute/private/path/AuthKey.p8
export MESSENGER_APNS_KEY_ID=YOUR_KEY_ID
export MESSENGER_APNS_TEAM_ID=YOUR_TEAM_ID
export MESSENGER_APNS_TOPIC=chat.horobets.mobile
```

New APNs keys can be scoped to an environment. To support development and production devices together, configure a separate sandbox key with `MESSENGER_APNS_DEVELOPMENT_KEY_FILE` and `MESSENGER_APNS_DEVELOPMENT_KEY_ID`; its team/topic default to the main APNs values and can also be overridden with the corresponding `MESSENGER_APNS_DEVELOPMENT_TEAM_ID` and `MESSENGER_APNS_DEVELOPMENT_TOPIC` variables. Without that override, the main key is used for development requests too, which requires a key authorized for that environment. Provider JWTs are reused and refreshed within Apple's required interval.

Open the signed app and choose **Settings → Enable notifications**. iOS asks for alert/sound/badge permission. The app forwards its APNs device token to the authenticated server. If permission was previously denied, change it in iOS Settings. Unsigned simulator builds validate the code and UI; they do not prove physical-device push delivery or provisioning.

## Server delivery and operations

Restart the server after configuring provider credentials. The authenticated registration response reports whether that platform's provider is configured, so permission approval alone does not claim background delivery is ready.

Endpoints:

- `PUT /api/v1/users/me/devices/{installation_uuid}`: register `{ "platform": "ios" | "android", "token": "...", "environment": "development" | "production" }`.
- `DELETE /api/v1/users/me/devices/{installation_uuid}`: remove this account's registration and queued deliveries.
- `GET /api/v1/users/me/notifications/ws`: authenticated account-wide live alerts.

The server uses a persistent per-device queue with a unique workspace/message/device key. It retries failed provider requests with backoff, rechecks membership, private-channel access, deletion and read state before delivery, and removes rejected/unregistered devices. Transient failures are bounded to six attempts. Queue insertion happens after the workspace message transaction commits; a process failure in that narrow interval can miss a push, but the message and unread inbox remain available after reconnecting. Push is an attention signal, not the authoritative message transport.

Provider credentials are read from private files on the server, never shipped in the app or returned by API endpoints. Do not commit `.p8` keys, service-account files, device tokens, or generated Firebase configuration. Lock-screen preview policy is also controlled by the operating system. Android notifications request private lock-screen visibility.

## Validation

Rust tests cover unread persistence and ownership, monotonic cross-device read cursors, active-conversation suppression, cross-workspace alerts and routing, outsider/private-channel/revocation filtering, authenticated token registration, provider payloads, and registration/job cleanup. Native builds check Kotlin/Swift permissions, service declarations and notification callbacks. End-to-end APNs/FCM delivery requires the operator's real credentials, matching platform configuration and a signed device build.

References: [APNs provider requests](https://developer.apple.com/documentation/usernotifications/sending-notification-requests-to-apns), [FCM Android receiving behavior](https://firebase.google.com/docs/cloud-messaging/android/receive-messages), [Firebase app initialization](https://firebase.google.com/docs/reference/android/com/google/firebase/FirebaseApp).

Android emulator verification exercised the real notification permission prompt and tray, notification taps (including process recreation), inbox preservation while browsing, and mark-all-read. Rust tests and the Swift bridge tests pass; the macOS app bundle and iOS Simulator build are validated. The packaged desktop agent's app identity was checked without requesting OS permission. Actual APNs/FCM delivery and physical-device provisioning remain unverified until deployment credentials are supplied.
