import Foundation

struct Person: Decodable, Identifiable {
    var id: String
    var email: String
    var displayName: String
    var handle: String?
    var bio: String
    var avatarUrl: String?
    var createdAt: Int64
}
struct Workspace: Decodable, Identifiable {
    var id: String
    var name: String
    var slug: String
}
struct Channel: Decodable, Identifiable {
    var id: String
    var name: String
    var topic: String?
    var isPrivate: Bool
    var isDirect: Bool
    var isSaved: Bool
    var peerUserId: String?
    var createdBy: String
}
struct Message: Decodable, Identifiable {
    var id: String
    var channelId: String
    var userId: String
    var parentId: String?
    var bodyMd: String
    var topicTag: String?
    var createdAt: Int64
    var editedAt: Int64?
    var deletedAt: Int64?
}
struct Reaction: Decodable {
    var emoji: String
    var count: Int64
    var mine: Bool
}
struct DeliveryState: Decodable {
    var label: String
    var failed = false
    init(label: String, failed: Bool = false) { self.label = label; self.failed = failed }
    init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        if let value = try? container.decode(String.self) {
            label = value == "Pending" ? "Queued · sends when connected" : ""
        } else {
            failed = true
            let value = try container.decode([String: String].self)
            if let failure = value["Failed"] { label = "Not sent: \(failure)" } else { label = "" }
        }
    }
}
struct CachedMessage: Decodable, Identifiable {
    var message: Message
    var status: DeliveryState
    var reactions: [Reaction]
    var id: String { message.id }
}
struct MessageNotification: Decodable {
    var workspaceId: String
    var message: Message
    var author: String
}
struct MobileSnapshot: Decodable {
    var unreadMessages: [Message]?
    var openedMessage: String?
    var notifications: [MessageNotification]?
    var server = ""
    var userId: String?
    var workspaces: [Workspace] = []
    var workspace: String?
    var channels: [Channel] = []
    var users: [Person] = []
    var me: Person?
    var avatars: [String: String] = [:]
    var imagePreviews: [String: String] = [:]
    var messageAttachments: [String: [Attachment]] = [:]
    var attachments: [Attachment] = []
    var people: [Member] = []
    var pins: [PinnedMessage] = []
    var moderation: [ModerationAction] = []
    var channelMembers: [String] = []
    var channel: String?
    var messages: [CachedMessage] = []
    var thread: String?
    var threadMessages: [CachedMessage] = []
    var unread: [String: Int64] = [:]
    var replyCounts: [String: Int] = [:]
    var searchResults: [Message] = []
    var connected = false
    var busy = false
    var hasOlder = false
    var error: String?
    var notice: String?
}

struct Attachment: Decodable, Identifiable {
    var id: String
    var messageId: String?
    var filename: String
    var mimeType: String
    var sizeBytes: Int64
    var isImage: Bool { mimeType.hasPrefix("image/") }
}
struct Member: Decodable, Identifiable {
    var user: Person
    var role: String
    var joinedAt: Int64
    var id: String { user.id }
}
struct PinnedMessage: Decodable, Identifiable {
    var message: Message
    var pinnedBy: String
    var createdAt: Int64
    var id: String { message.id }
}
struct ModerationAction: Decodable, Identifiable {
    var seq: Int64
    var channelId: String
    var messageId: String
    var actorId: String
    var action: String
    var createdAt: Int64
    var id: Int64 { seq }
}
enum MobilePanel: String, Identifiable {
    case unread
    case settings, people, workspace, pins, profile, thread, members, channel
    var id: String { rawValue }
}
let reactionChoices = ["👍", "👎", "❤️", "😂", "😮", "😢", "😡", "🎉", "🚀", "👀", "✅", "🙏", "🔥", "💯", "🤔", "👏", "💪", "🙌", "🤝", "🫡", "😄", "😍", "😎", "🤯", "🧠", "💡", "☕", "🐛", "🛠️", "⚠️", "❌", "✨"]

enum MediaSource {
    static func fileURL(_ path: String?) -> URL? {
        guard let path, !path.isEmpty else { return nil }
        return URL(fileURLWithPath: path)
    }
}
