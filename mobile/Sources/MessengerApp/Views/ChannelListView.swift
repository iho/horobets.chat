import SwiftUI

struct ChannelListView: View {
    @ObservedObject var core: CoreAdapter
    @State private var newWorkspace = false
    @State private var newChannel = false
    @State private var name = ""
    @State private var slug = ""
    @State private var isPrivate = false
    var body: some View {
        NavigationStack(path: $core.navigation) {
            List {
                Section {
                    HStack(spacing: 12) {
                        Button { if let user = core.snapshot.userId { core.openProfile(user) } } label: {
                            AvatarView(core: core, userId: core.snapshot.userId ?? "", size: 42)
                        }.buttonStyle(.plain)
                        VStack(alignment: .leading, spacing: 3) {
                            Text(core.snapshot.me?.displayName ?? "Your workspace").font(.headline)
                            Text(core.snapshot.connected ? "● Connected" : "○ Offline · cached history").font(.caption).foregroundStyle(.secondary)
                        }
                        Spacer(minLength: 0)
                    }.padding(.vertical, 4)
                    Picker("Workspace", selection: Binding(get: { core.snapshot.workspace ?? "" }, set: { core.navigation = []; core.command("select_workspace", payload: $0) })) {
                        ForEach(core.snapshot.workspaces) { workspace in Text(workspace.name).tag(workspace.id) }
                    }
                }
                Section("Channels") {
                    ForEach(core.snapshot.channels.filter { !$0.isDirect && !$0.isSaved }) { channel in channelRow(channel) }
                    Button("+ New channel") { name = ""; isPrivate = false; newChannel = true }.disabled(core.snapshot.workspace == nil)
                }
                Section("Direct messages") {
                    ForEach(core.snapshot.channels.filter { $0.isDirect }) { channel in channelRow(channel) }
                    Button("+ Message a teammate") { core.panel = .people }.disabled(core.snapshot.workspace == nil)
                }
                Section("Workspace") {
                    Button("Saved messages") { if let id = core.snapshot.userId { core.openDirect(id) } }.disabled(core.snapshot.workspace == nil)
                    Button("Pinned messages") { core.panel = .pins }.disabled(core.snapshot.workspace == nil)
                    Button("People & profiles") { core.panel = .people }.disabled(core.snapshot.workspace == nil)
                    Button("Workspace & moderation") { core.panel = .workspace }.disabled(core.snapshot.workspace == nil)
                    Button("Create workspace") { name = ""; slug = ""; newWorkspace = true }
                }
                if core.snapshot.workspaces.isEmpty {
                    Section {
                        Text("No workspaces for this account").font(.headline)
                        Text("Sign in with your existing desktop account, ask a workspace owner to invite this email, or create a new workspace.").font(.subheadline).foregroundStyle(.secondary)
                        Text(core.snapshot.me?.email ?? "").font(.caption).foregroundStyle(.secondary)
                    }
                }
            }
            .navigationTitle("horobets.chat").compactTitle()
            .toolbar { ToolbarItem(placement: .primaryAction) { Button("Settings") { core.panel = .settings } } }
            .navigationDestination(for: String.self) { id in ChatFeedView(core: core, channelId: id) }
            .refreshable { core.command("refresh") }
            .sheet(isPresented: $newWorkspace) { workspaceForm }
            .sheet(isPresented: $newChannel) { channelForm }
        }
    }
    private func channelRow(_ channel: Channel) -> some View {
        Button { core.openChannel(channel.id) } label: {
            HStack(spacing: 10) {
                if channel.isDirect, let peer = channel.peerUserId { AvatarView(core: core, userId: peer, size: 28) }
                VStack(alignment: .leading, spacing: 3) {
                    Text(core.label(channel)).foregroundStyle(.primary).font(.subheadline).fontWeight((core.snapshot.unread[channel.id] ?? 0) > 0 ? .semibold : .regular)
                    if let topic = channel.topic, !topic.isEmpty { Text(topic).font(.caption).foregroundStyle(.secondary).lineLimit(1) }
                }
                Spacer()
                if let count = core.snapshot.unread[channel.id], count > 0 { Text("\(count)").font(.caption.bold()).padding(.horizontal, 8).padding(.vertical, 3).background(Color.teal.opacity(0.15)).clipShape(Capsule()) }
                Text("›").foregroundStyle(.secondary)
            }.padding(.vertical, 3)
        }.buttonStyle(.plain)
    }
    private var workspaceForm: some View {
        NavigationStack {
            Form {
                TextField("Workspace name", text: $name)
                TextField("workspace-slug", text: $slug).autocorrectionDisabled()
                Button("Create workspace") { if core.command("create_workspace", payload: ["name": name, "slug": slug]) { newWorkspace = false } }.disabled(name.isEmpty || slug.isEmpty || core.snapshot.busy)
            }.navigationTitle("New workspace").compactTitle().toolbar { Button("Cancel") { newWorkspace = false } }
        }
    }
    private var channelForm: some View {
        NavigationStack {
            Form {
                TextField("Channel name", text: $name)
                Toggle("Private channel", isOn: $isPrivate)
                Button("Create channel") { if core.command(isPrivate ? "create_private_channel" : "create_channel", payload: name) { newChannel = false } }.disabled(name.isEmpty || !core.snapshot.connected)
            }.navigationTitle("New channel").compactTitle().toolbar { Button("Cancel") { newChannel = false } }
        }
    }
}
