import Foundation
import SwiftUI
#if SKIP
import chat.horobets.core.MobileClient
import chat.horobets.core.SnapshotListener
import messenger.app.MobileFiles
#else
import MessengerCore
#if os(iOS)
import UIKit
#elseif os(macOS)
import AppKit
#endif
#endif

/// UniFFI delivers on a native worker; this callback never touches SwiftUI directly.
final class CoreListener: SnapshotListener {
    let receive: @Sendable (String) -> Void
    init(receive: @escaping @Sendable (String) -> Void) { self.receive = receive }
    // The protocol is generated Kotlin, so Skip cannot infer its override.
    // SKIP DECLARE: override fun onSnapshot(snapshotJson: String)
    func onSnapshot(snapshotJson: String) { receive(snapshotJson) }
}

@MainActor
final class CoreAdapter: ObservableObject {
    // Skip eagerly constructs StateObject's initial value when a view is rebuilt.
    // Keep the networking engine process-scoped, rather than starting discarded workers.
    static let shared = CoreAdapter()

    @Published var snapshot = MobileSnapshot()
    @Published var localError: String?
    @Published var profileEditor = ProfileEditor()
    @Published var navigation: [String] = []
    @Published var panel: MobilePanel?
    @Published var profileId: String?
    @Published var openingThread: String?
    @Published var drafts: [String: String] = [:]
    @Published var uploading = false
    @Published var importingFile = false
    @Published var importingAvatar = false
    @Published var importImages = false
    @Published var previewPath: String?
    @Published var exportedPath: String?
    @Published var downloading = false
    private var downloadPath: String?
    private var initialAttachmentCount = 0
    private var oldAvatar: String?
    private var avatarUpload = false
    private var pendingDirect: String?
    private var client: MobileClient?
    private var listener: CoreListener?

    init(dataDirectory: URL? = nil) {
        do {
            let directory = dataDirectory ?? URL.applicationSupportDirectory
                .appendingPathComponent("horobets-messenger", isDirectory: true)
            let client = try MobileClient(dataDir: directory.path)
            let listener = CoreListener { [weak self] json in
                Task { @MainActor [weak self] in self?.receive(json) }
            }
            self.client = client
            self.listener = listener
            try client.subscribe(listener: listener)
        } catch { localError = error.localizedDescription }
    }

    private func receive(_ json: String) {
        do {
            let decoder = JSONDecoder()
            decoder.keyDecodingStrategy = .convertFromSnakeCase
            let value = try decoder.decode(MobileSnapshot.self, from: Data(json.utf8))
            let previousWorkspace = snapshot.workspace
            snapshot = value
            profileEditor.receive(value.me, error: value.error)
            if previousWorkspace != value.workspace || navigation.last.map({ id in !value.channels.contains(where: { $0.id == id }) }) == true {
                navigation = []; panel = nil; profileId = nil; previewPath = nil; exportedPath = nil
            }
            if uploading && (value.error != nil || (avatarUpload ? value.me?.avatarUrl != oldAvatar : value.attachments.count > initialAttachmentCount)) { uploading = false }
            if value.error != nil { downloading = false; pendingDirect = nil }
            if let path = downloadPath, FileManager.default.fileExists(atPath: path) {
                downloadPath = nil; downloading = false
                #if SKIP
                MobileFiles.saveFile(path: path) { error in
                    Task { @MainActor [weak self] in
                        if let error { self?.localError = error }
                    }
                }
                #else
                exportedPath = path
                #endif
            }
            if let target = pendingDirect {
                let match = value.channels.first { channel in
                    target == value.userId ? channel.isSaved : channel.isDirect && channel.peerUserId == target
                }
                if let match { pendingDirect = nil; openChannel(match.id) }
            }
        } catch { localError = "Cannot read client update: \(error.localizedDescription)" }
    }

    @discardableResult
    func command(_ type: String, payload: Any? = nil) -> Bool {
        guard let client else { localError = "Client is unavailable. Restart the app."; return false }
        do {
            var value: [String: Any] = ["type": type]
            if let payload { value["payload"] = payload }
            let data = try JSONSerialization.data(withJSONObject: value)
            guard let json = String(data: data, encoding: .utf8) else { return false }
            try client.sendCommand(commandJson: json)
            localError = nil
            return true
        } catch { localError = error.localizedDescription; return false }
    }

    func saveProfile() {
        guard let value = profileEditor.prepareSave() else { return }
        if !command("save_profile", payload: ["display_name": value.name, "handle": value.handle, "bio": value.bio]) {
            profileEditor.fail(localError ?? "Could not save profile. Try again.")
        }
    }

    func event(_ type: String, payload: [String: Any]) {
        command("event", payload: ["type": type, "payload": payload])
    }
    func selectChannel(_ id: String) { command("select_channel", payload: id) }
    func label(_ channel: Channel) -> String {
        if channel.isSaved { return "Saved messages" }
        if channel.isDirect {
            return snapshot.users.first { $0.id == channel.peerUserId }?.displayName ?? "Direct message"
        }
        return "\(channel.isPrivate ? "🔒" : "#") \(channel.name)"
    }
    func author(_ id: String) -> String {
        snapshot.users.first { $0.id == id }?.displayName ?? (snapshot.me?.id == id ? snapshot.me?.displayName ?? "You" : "Member")
    }
    var currentChannel: Channel? { snapshot.channels.first { $0.id == snapshot.channel } }
    var role: String { snapshot.people.first { $0.id == snapshot.userId }?.role ?? "member" }
    var canModerate: Bool { ["owner", "admin", "moderator"].contains(role) }
    var canManage: Bool { ["owner", "admin"].contains(role) }
    func canPin(_ message: Message) -> Bool {
        guard let channel = snapshot.channels.first(where: { $0.id == message.channelId }) else { return false }
        return channel.isDirect || channel.isSaved || canModerate
    }
    func canDelete(_ message: Message) -> Bool {
        message.userId == snapshot.userId || (canModerate && snapshot.channels.contains { $0.id == message.channelId && !$0.isDirect && !$0.isSaved })
    }
    func canEdit(_ channel: Channel) -> Bool { !channel.isDirect && !channel.isSaved && (canModerate || channel.createdBy == snapshot.userId) }
    var canManageMembers: Bool {
        guard let channel = currentChannel else { return false }
        return channel.isPrivate && !channel.isDirect && !channel.isSaved && (channel.createdBy == snapshot.userId || (canManage && snapshot.channelMembers.contains(snapshot.userId ?? "")))
    }
    func isPinned(_ id: String) -> Bool { snapshot.pins.contains { $0.id == id } }
    func openChannel(_ id: String) {
        guard snapshot.channels.contains(where: { $0.id == id }) else { return }
        panel = nil
        navigation = [id]
        command("select_channel", payload: id)
    }
    func openProfile(_ id: String) { profileId = id; panel = .profile }
    func openDirect(_ id: String) { pendingDirect = id; panel = nil; command("open_direct", payload: id) }
    func openThread(_ id: String) { openingThread = id; command("select_thread", payload: id); panel = .thread }
    func dismissPanel() {
        if panel == .thread { openingThread = nil; command("select_thread", payload: NSNull()) }
        panel = nil
    }
    func openMessage(_ message: Message) {
        openChannel(message.channelId)
        Task { @MainActor in
            try? await Task.sleep(nanoseconds: 400_000_000)
            self.openThread(message.parentId ?? message.id)
        }
    }
    func draftKey(_ thread: Bool) -> String {
        "\(snapshot.workspace ?? "")/\(snapshot.channel ?? "")/\(thread ? snapshot.thread ?? "thread" : "main")"
    }
    func draft(_ thread: Bool) -> String { drafts[draftKey(thread)] ?? "" }
    func setDraft(_ value: String, thread: Bool) { drafts[draftKey(thread)] = value }
    func send(_ body: String, thread: Bool, topic: String = "") -> Bool {
        if body.hasPrefix("/join ") && !thread {
            let name = String(body.dropFirst(6)).trimmingCharacters(in: .whitespacesAndNewlines)
            guard let channel = snapshot.channels.first(where: { $0.name == name }) else { localError = "Channel not found"; return false }
            setDraft("", thread: thread); openChannel(channel.id); return true
        }
        if body.utf8.count > 64000 { localError = "Message exceeds 64000 bytes"; return false }
        var payload: [String: Any] = ["body": body]
        if thread, let parent = snapshot.thread { payload["parent_id"] = parent }
        if !topic.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { payload["topic_tag"] = topic }
        return command("send", payload: payload)
    }
    func uploadPath(_ path: String, avatar: Bool) {
        initialAttachmentCount = snapshot.attachments.count
        oldAvatar = snapshot.me?.avatarUrl
        avatarUpload = avatar
        uploading = command(avatar ? "upload_avatar" : "upload", payload: path)
    }
    func chooseFile(avatar: Bool, imagesOnly: Bool = false) {
        guard snapshot.userId != nil else { localError = "Sign in to upload a file."; return }
        #if SKIP
        MobileFiles.pickFile(imagesOnly: avatar || imagesOnly, photoLibrary: imagesOnly, avatar: avatar) { path, error in
            Task { @MainActor [weak self] in
                if let error { self?.localError = error }
                if let path { self?.uploadPath(path, avatar: avatar) }
            }
        }
        #else
        importingAvatar = avatar
        importImages = avatar || imagesOnly
        importingFile = true
        #endif
    }
    func imported(_ url: URL, avatar: Bool) {
        #if !SKIP
        Task { @MainActor in
            do {
                let path = try await Task.detached {
                    let access = url.startAccessingSecurityScopedResource()
                    defer { if access { url.stopAccessingSecurityScopedResource() } }
                    let size = try url.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? 0
                    guard size <= (avatar ? 5 : 10) * 1024 * 1024 else { throw CocoaError(.fileReadTooLarge) }
                    let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
                    try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
                    let file = folder.appendingPathComponent(url.lastPathComponent)
                    try FileManager.default.copyItem(at: url, to: file)
                    return file.path
                }.value
                self.uploadPath(path, avatar: avatar)
            } catch { self.localError = error.localizedDescription }
        }
        #endif
    }
    func importPhoto(_ data: Data, filename: String, avatar: Bool) {
        #if !SKIP
        guard data.count <= (avatar ? 5 : 10) * 1024 * 1024 else { localError = "Image is too large"; return }
        Task { @MainActor in
            do {
                let path = try await Task.detached {
                    let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
                    try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
                    let file = folder.appendingPathComponent(filename)
                    try data.write(to: file)
                    return file.path
                }.value
                self.uploadPath(path, avatar: avatar)
            } catch { self.localError = error.localizedDescription }
        }
        #endif
    }
    func openReference(_ url: URL) -> Bool {
        guard url.scheme == "horobets" else { return false }
        let parts = url.pathComponents.filter { $0 != "/" }
        guard url.host == "workspace", parts.count == 3, parts[0] == snapshot.workspace else {
            localError = "This reference belongs to another workspace."; return true
        }
        if parts[1] == "user", snapshot.users.contains(where: { $0.id == parts[2] }) { openProfile(parts[2]) }
        else if parts[1] == "channel", snapshot.channels.contains(where: { $0.id == parts[2] && !$0.isDirect && !$0.isSaved }) { openChannel(parts[2]) }
        else { localError = "This user or channel is no longer available." }
        return true
    }
    func download(_ attachment: Attachment) {
        guard snapshot.connected else { localError = "Connect to download the original file."; return }
        do {
            let folder = URL.applicationSupportDirectory.appendingPathComponent("exports").appendingPathComponent(UUID().uuidString)
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
            let filename = URL(fileURLWithPath: attachment.filename).lastPathComponent
            let path = folder.appendingPathComponent(["", ".", ".."].contains(filename) ? "attachment" : filename).path
            downloading = command("download", payload: ["id": attachment.id, "path": path])
            if downloading { downloadPath = path }
        } catch { localError = error.localizedDescription }
    }
    func copyText(_ text: String) {
        #if SKIP
        MobileFiles.copyText(text: text)
        #elseif os(iOS)
        UIPasteboard.general.string = text
        #elseif os(macOS)
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
        #endif
    }
    func shutdown() { client?.shutdown(); client = nil; listener = nil }
}
