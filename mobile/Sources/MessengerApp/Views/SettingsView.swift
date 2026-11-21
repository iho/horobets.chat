import SwiftUI
struct SettingsView: View {
    @ObservedObject var core: CoreAdapter
    @AppStorage("darkMode") private var darkMode = true
    @State private var signOut = false
    var body: some View {
        Form {
            Section("Your profile") {
                HStack(spacing: 16) {
                    AvatarView(core: core, userId: core.snapshot.userId ?? "", size: 64)
                    VStack(alignment: .leading, spacing: 8) {
                        PhotoUploadButton(core: core, avatar: true)
                        Button("Choose image file") { core.chooseFile(avatar: true) }.font(.caption).disabled(core.uploading)
                    }
                }.padding(.vertical, 8)
                if core.uploading { ProgressView("Uploading photo…") }
                Text(core.snapshot.me?.email ?? "").font(.caption).foregroundStyle(.secondary)
                Text("Display name").font(.caption).foregroundStyle(.secondary)
                TextField("Display name", text: $core.profileEditor.name).disabled(core.profileEditor.saving)
                Text("Handle").font(.caption).foregroundStyle(.secondary)
                #if SKIP || os(iOS)
                TextField("Handle", text: $core.profileEditor.handle).autocorrectionDisabled().textInputAutocapitalization(.never).disabled(core.profileEditor.saving)
                #else
                TextField("Handle", text: $core.profileEditor.handle).autocorrectionDisabled().disabled(core.profileEditor.saving)
                #endif
                Text("3–32 letters, numbers, or underscores. Start with a letter.").font(.caption).foregroundStyle(.secondary)
                Text("About you").font(.caption).foregroundStyle(.secondary)
                TextField("About you", text: $core.profileEditor.bio, axis: .vertical).disabled(core.profileEditor.saving)
                if let error = core.profileEditor.error { Text(error).font(.caption).foregroundStyle(.red) }
                if core.profileEditor.saving { ProgressView("Saving profile…") }
                else if core.profileEditor.saved && !core.profileEditor.hasChanges { Text("Profile saved").font(.caption).foregroundStyle(.teal) }
                Button("Save profile") { core.saveProfile() }.disabled(!core.profileEditor.canSave || core.snapshot.busy)
            }
            Section("Notifications") {
                Text(core.notificationStatus).font(.caption).foregroundStyle(.secondary)
                Button(core.notificationsEnabled ? "Check notification permission" : "Enable notifications") { core.enableNotifications() }
                if core.notificationsEnabled { Button("Turn off notifications") { core.disableNotifications() } }
            }
            Section("Appearance") { Toggle("Dark mode", isOn: $darkMode) }
            Section("Server") {
                Text(core.snapshot.server).font(.caption).selectableText()
                Button("Refresh") { core.command("refresh") }
                Button("Sign out", role: .destructive) { signOut = true }
            }
        }.navigationTitle("Settings").compactTitle()
        .alert("Sign out?", isPresented: $signOut) {
            Button("Cancel", role: .cancel) {}
            Button("Sign out", role: .destructive) { core.disableNotifications(); core.drafts = [:]; core.panel = nil; core.navigation = []; core.command("logout") }
        } message: { Text("The saved session is removed. Offline history stays on this device.") }
    }
}
