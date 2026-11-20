import Foundation

struct ProfileValues: Equatable {
    var name: String
    var handle: String
    var bio: String
    init(_ user: Person) {
        name = user.displayName; handle = user.handle ?? ""; bio = user.bio
    }
    init(name: String, handle: String, bio: String) {
        self.name = name; self.handle = handle; self.bio = bio
    }
}

/// The settings draft belongs to the client, so sheet/screen recreation cannot
/// replace typed values with an older snapshot while the user is editing.
struct ProfileEditor {
    var name = ""
    var handle = ""
    var bio = ""
    private var userId: String?
    private var baseline: ProfileValues?
    private var pending: ProfileValues?
    var saving = false
    var saved = false
    var error: String?

    var values: ProfileValues {
        var normalized = handle.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        while normalized.hasPrefix("@") { normalized = String(normalized.dropFirst()) }
        return ProfileValues(name: name.trimmingCharacters(in: .whitespacesAndNewlines), handle: normalized, bio: bio.trimmingCharacters(in: .whitespacesAndNewlines))
    }
    var hasChanges: Bool { baseline != values }
    var canSave: Bool { userId != nil && hasChanges && !saving && !values.name.isEmpty }

    mutating func receive(_ user: Person?, error serverError: String?) {
        guard let user else { reset(); return }
        if user.id != userId {
            reset(); userId = user.id
            baseline = ProfileValues(user)
            name = user.displayName; handle = user.handle ?? ""; bio = user.bio
            return
        }
        let received = ProfileValues(user)
        if saving {
            if received == pending {
                saving = false; pending = nil; saved = true; self.error = nil
                baseline = received
            } else if let serverError { fail(serverError) }
        }
        if !saving && !hasChanges {
            name = user.displayName; handle = user.handle ?? ""; bio = user.bio
            baseline = received
        }
    }
    mutating func prepareSave() -> ProfileValues? {
        guard canSave else { return nil }
        error = nil; saved = false
        let value = values
        guard value.name.utf8.count <= 200 else { error = "Display name must be at most 200 bytes."; return nil }
        guard bio.utf8.count <= 2000 else { error = "Bio must be at most 2000 bytes."; return nil }
        saving = true; pending = value
        return value
    }
    mutating func fail(_ message: String) {
        saving = false; pending = nil; saved = false; error = message
    }
    private mutating func reset() {
        name = ""; handle = ""; bio = ""; userId = nil; baseline = nil
        pending = nil; saving = false; saved = false; error = nil
    }
}
