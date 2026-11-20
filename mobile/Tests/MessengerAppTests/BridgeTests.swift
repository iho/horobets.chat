import Foundation
import XCTest
import MessengerCore
@testable import MessengerApp

final class Callback: SnapshotListener, @unchecked Sendable {
    let expectation: XCTestExpectation
    init(_ expectation: XCTestExpectation) { self.expectation = expectation }
    func onSnapshot(snapshotJson: String) {
        do {
            let decoder = JSONDecoder()
            decoder.keyDecodingStrategy = .convertFromSnakeCase
            let state = try decoder.decode(MobileSnapshot.self, from: Data(snapshotJson.utf8))
            XCTAssertNil(state.userId)
            XCTAssertTrue(state.messages.isEmpty)
            expectation.fulfill()
        } catch { XCTFail("Cannot decode Rust snapshot: \(error)") }
    }
}
final class BridgeTests: XCTestCase {
    func testRustLibraryLoadsAndCallsSwiftWithDecodableSnapshot() throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: directory) }
        let client = try MobileClient(dataDir: directory.path)
        defer { client.shutdown() }
        let callback = expectation(description: "Rust snapshot delivered to Swift")
        try client.subscribe(listener: Callback(callback))
        wait(for: [callback], timeout: 10)
    }
    func testDeliveryStatesDecodeTheRustEnum() throws {
        let decoder = JSONDecoder()
        XCTAssertEqual(try decoder.decode(DeliveryState.self, from: Data("\"Pending\"".utf8)).label, "Queued · sends when connected")
        XCTAssertEqual(try decoder.decode(DeliveryState.self, from: Data("{\"Failed\":\"offline\"}".utf8)).label, "Not sent: offline")
    }
}

final class MobileParityTests: XCTestCase {
    func testProfileDraftSurvivesRefreshAndOnlyConfirmsServerAcceptedValues() {
        var editor = ProfileEditor()
        let user = Person(id: "u", email: "u@example.test", displayName: "Old name", handle: "old_handle", bio: "Old bio", avatarUrl: nil, createdAt: 0)
        editor.receive(user, error: nil)
        editor.name = " New name "
        editor.handle = " @NEW_HANDLE "
        editor.receive(user, error: nil)
        XCTAssertEqual(editor.name, " New name ")
        let request = editor.prepareSave()
        XCTAssertEqual(request?.name, "New name")
        XCTAssertEqual(request?.handle, "new_handle")
        XCTAssertTrue(editor.saving)
        editor.receive(user, error: nil)
        XCTAssertTrue(editor.saving)
        var accepted = user; accepted.displayName = "New name"; accepted.handle = "new_handle"
        editor.receive(accepted, error: nil)
        XCTAssertFalse(editor.saving)
        XCTAssertTrue(editor.saved)
        XCTAssertFalse(editor.hasChanges)
        editor.handle = "taken"
        XCTAssertNotNil(editor.prepareSave())
        editor.receive(accepted, error: "That handle is already taken")
        XCTAssertFalse(editor.saving)
        XCTAssertEqual(editor.handle, "taken")
        XCTAssertEqual(editor.error, "That handle is already taken")
        XCTAssertTrue(editor.canSave)
        editor.receive(nil, error: nil)
        XCTAssertEqual(editor.name, "")
        XCTAssertFalse(editor.canSave)
    }

    private func person(_ id: String = "u", handle: String = "ihor") -> Person {
        Person(id: id, email: "test@example.com", displayName: "Ihor", handle: handle, bio: "Hello", avatarUrl: nil, createdAt: 0)
    }
    private func channel(_ name: String = "general", direct: Bool = false, saved: Bool = false) -> Channel {
        Channel(id: name, name: name, topic: "Team", isPrivate: false, isDirect: direct, isSaved: saved, peerUserId: nil, createdBy: "u")
    }
    func testMentionCompletionPreservesUnicodeAndFiltersPersonalConversations() {
        let users = [person()]
        let channels = [channel(), channel("direct", direct: true), channel("notes", saved: true)]
        XCTAssertEqual(MessageText.suggestions("👋 @i", users: users, channels: channels).map(\.text), ["@ihor"])
        XCTAssertEqual(MessageText.complete("👋 @i", with: "@ihor"), "👋 @ihor ")
        XCTAssertEqual(MessageText.suggestions("#", users: users, channels: channels).map(\.text), ["#general"])
        XCTAssertTrue(MessageText.suggestions("mail@i", users: users, channels: channels).isEmpty)
        XCTAssertTrue(MessageText.suggestions("https://site/#g", users: users, channels: channels).isEmpty)
    }
    func testReferencesAreLinksWithoutChangingCodeOrExistingLinks() {
        let linked = MessageText.linked("👋 @ihor, #general! `@ihor` [@ihor](https://example.com)", workspace: "w", users: [person()], channels: [channel()])
        XCTAssertEqual(linked, "👋 [@ihor](horobets://workspace/w/user/u), [#general](horobets://workspace/w/channel/general)! `@ihor` [@ihor](https://example.com)")
        XCTAssertEqual(MessageText.linked("\\@ihor mail@ihor #general-other", workspace: "w", users: [person()], channels: [channel()]), "\\@ihor mail@ihor #general-other")
        let blocks = MessageText.blocks("Before\n\n```rust\n@ihor #general\n```\n\nAfter")
        XCTAssertEqual(blocks.map(\.kind), ["text", "code:rust", "text"])
        XCTAssertEqual(blocks[1].text, "@ihor #general")
    }
    func testMarkdownBlocksAndDuplicateTableCellsKeepStableIdentities() {
        let blocks = MessageText.blocks("# Heading\n\n> Quote\n- [x] Done\n| A | A |\n| --- | --- |")
        XCTAssertEqual(blocks.map(\.kind), ["heading1", "quote", "list", "table", "tableRule"])
        XCTAssertEqual(MessageText.cells("| A | A |").map(\.id), [0, 1])
    }
    func testImageSheetDismissalNeverConstructsAnEmptyFileURL() {
        XCTAssertNil(MediaSource.fileURL(nil))
        XCTAssertNil(MediaSource.fileURL(""))
        XCTAssertEqual(MediaSource.fileURL("/private/preview.png")?.path, "/private/preview.png")
    }
    func testSnapshotIncludesAuthenticatedMediaAndManagementState() throws {
        let json = """
        {"server":"http://server","workspaces":[],"channels":[],"users":[],"messages":[],"thread_messages":[],"unread":{},"reply_counts":{},"search_results":[],"connected":true,"busy":false,"has_older":false,
        "avatars":{"u":"/private/avatar.png"},"image_previews":{"a":"/private/preview.png"},
        "attachments":[],"message_attachments":{"m":[{"id":"a","message_id":"m","filename":"photo.png","mime_type":"image/png","size_bytes":100}]},
        "people":[],"pins":[],"moderation":[],"channel_members":["u"]}
        """
        let decoder = JSONDecoder(); decoder.keyDecodingStrategy = .convertFromSnakeCase
        let snapshot = try decoder.decode(MobileSnapshot.self, from: Data(json.utf8))
        XCTAssertEqual(snapshot.avatars["u"], "/private/avatar.png")
        XCTAssertEqual(snapshot.imagePreviews["a"], "/private/preview.png")
        XCTAssertTrue(snapshot.messageAttachments["m"]![0].isImage)
        XCTAssertEqual(snapshot.messageAttachments["m"]![0].filename, "photo.png")
        XCTAssertEqual(snapshot.channelMembers, ["u"])
    }
    @MainActor func testModeratorRightsDoNotExtendIntoOtherPeoplesDirectMessages() {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let core = CoreAdapter(dataDirectory: directory)
        defer { try? FileManager.default.removeItem(at: directory) }
        defer { core.shutdown() }
        let me = person()
        core.snapshot.userId = me.id
        core.snapshot.people = [Member(user: me, role: "moderator", joinedAt: 0)]
        let publicChannel = channel()
        let direct = channel("dm", direct: true)
        core.snapshot.channels = [publicChannel, direct]
        let message = Message(id: "m", channelId: publicChannel.id, userId: "other", parentId: nil, bodyMd: "Hello", topicTag: nil, createdAt: 0, editedAt: nil, deletedAt: nil)
        XCTAssertTrue(core.canDelete(message))
        var dmMessage = message; dmMessage.channelId = direct.id
        XCTAssertFalse(core.canDelete(dmMessage))
        XCTAssertTrue(core.canPin(dmMessage)) // Participant may pin their own accessible DM.
        core.snapshot.people = [Member(user: me, role: "member", joinedAt: 0)]
        XCTAssertFalse(core.canDelete(message))
        XCTAssertFalse(core.canPin(message))
    }
}
