import SwiftUI

struct PanelView: View {
    @ObservedObject var core: CoreAdapter
    let panel: MobilePanel
    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                ErrorBanner(core: core)
                switch panel {
                case .settings: SettingsView(core: core)
                case .people: PeopleView(core: core)
                case .workspace: WorkspaceView(core: core)
                case .pins: PinsView(core: core)
                case .profile: UserProfileView(core: core, userId: core.profileId ?? "")
                case .thread: ThreadDetailView(core: core)
                case .channel: ChannelEditView(core: core)
                case .members: ChannelMembersView(core: core)
                }
            }.toolbar { ToolbarItem(placement: .cancellationAction) { Button("Done") { core.dismissPanel() } } }
        }
        .modifier(FileImportModifier(core: core))
        .modifier(MediaPresentation(core: core))
    }
}
struct PeopleView: View {
    @ObservedObject var core: CoreAdapter
    @State private var search = ""
    var body: some View {
        List {
            ForEach(core.snapshot.people.filter { search.isEmpty || $0.user.displayName.lowercased().contains(search.lowercased()) || ($0.user.handle ?? "").lowercased().contains(search.lowercased()) }) { member in
                HStack(spacing: 12) {
                    Button { core.openProfile(member.id) } label: { AvatarView(core: core, userId: member.id) }.buttonStyle(.plain)
                    Button { core.openProfile(member.id) } label: {
                        VStack(alignment: .leading, spacing: 3) {
                            Text(member.user.displayName).font(.subheadline.bold()).foregroundStyle(.primary)
                            Text(member.user.handle.map { "@" + $0 } ?? member.user.email).font(.caption).foregroundStyle(.secondary)
                        }
                    }.buttonStyle(.plain)
                    Spacer(minLength: 0)
                    Menu("Actions") {
                        Button("View profile") { core.openProfile(member.id) }
                        Button(member.id == core.snapshot.userId ? "Saved messages" : "Send message") { core.openDirect(member.id) }
                        if core.canManage && ["member", "moderator"].contains(member.role) {
                            Button(member.role == "moderator" ? "Remove moderator role" : "Make moderator") {
                                core.command("set_role", payload: ["user_id": member.id, "role": member.role == "moderator" ? "member" : "moderator"])
                            }.disabled(!core.snapshot.connected)
                        }
                    }.font(.caption)
                }.padding(.vertical, 5)
            }
        }.searchable(text: $search, prompt: "Name or handle").navigationTitle("People").compactTitle()
    }
}
struct UserProfileView: View {
    @ObservedObject var core: CoreAdapter
    let userId: String
    var member: Member? { core.snapshot.people.first { $0.id == userId } }
    var user: Person? { member?.user ?? core.snapshot.users.first { $0.id == userId } }
    var body: some View {
        ScrollView {
            if let user {
                VStack(alignment: .leading, spacing: 18) {
                    HStack(spacing: 18) {
                        AvatarView(core: core, userId: userId, size: 80)
                        VStack(alignment: .leading, spacing: 5) {
                            Text(user.displayName).font(.title2.bold())
                            if let handle = user.handle { Text("@" + handle).foregroundStyle(.teal) }
                            if let member { Text(member.role.capitalized).font(.caption).foregroundStyle(.secondary) }
                        }
                    }
                    if !user.bio.isEmpty { Text(user.bio) }
                    Text(user.email).font(.subheadline).foregroundStyle(.secondary)
                    Divider()
                    if let member { Text("Joined workspace").font(.caption).foregroundStyle(.secondary); Text(MessageText.day(member.joinedAt)) }
                    Text("Account created").font(.caption).foregroundStyle(.secondary)
                    Text(MessageText.day(user.createdAt))
                    Button(userId == core.snapshot.userId ? "Open saved messages" : "Send private message") { core.openDirect(userId) }.buttonStyle(.borderedProminent)
                    if userId == core.snapshot.userId { Button("Edit your profile") { core.panel = .settings }.buttonStyle(.bordered) }
                }.padding(24).frame(maxWidth: .infinity, alignment: .leading)
            } else { Text("This profile is no longer available in the workspace.").foregroundStyle(.secondary).padding() }
        }.navigationTitle("Profile").compactTitle()
    }
}
struct WorkspaceView: View {
    @ObservedObject var core: CoreAdapter
    @State private var name = ""
    @State private var slug = ""
    @State private var invite = ""
    var body: some View {
        Form {
            Section("Workspace") {
                if core.canManage {
                    TextField("Name", text: $name)
                    TextField("Slug", text: $slug).autocorrectionDisabled()
                    Button("Save workspace") { core.command("edit_workspace", payload: ["name": name, "slug": slug]) }.disabled(!core.snapshot.connected || name.isEmpty || slug.isEmpty)
                } else { Text(core.snapshot.workspaces.first { $0.id == core.snapshot.workspace }?.name ?? "Workspace") }
            }
            Section("People & access") {
                Button("People and roles") { core.panel = .people }
                if core.canManage {
                    TextField("Registered teammate’s email", text: $invite).autocorrectionDisabled()
                    Button("Invite teammate") { core.command("invite", payload: invite.trimmingCharacters(in: .whitespacesAndNewlines)) }.disabled(!core.snapshot.connected || invite.isEmpty)
                }
            }
            Section("Channels") {
                ForEach(core.snapshot.channels.filter { !$0.isDirect && !$0.isSaved }) { channel in
                    HStack {
                        Text(core.label(channel))
                        Spacer()
                        if core.canEdit(channel) {
                            Button("Edit") { core.openChannel(channel.id); core.panel = .channel }.disabled(!core.snapshot.connected)
                        }
                    }
                }
            }
            if core.canModerate {
                Section("Moderation activity") {
                    if core.snapshot.moderation.isEmpty { Text("No recent moderation actions.").foregroundStyle(.secondary) }
                    ForEach(core.snapshot.moderation) { action in
                        VStack(alignment: .leading, spacing: 4) {
                            Text("\(core.author(action.actorId)) · \(action.action.replacingOccurrences(of: "_", with: " "))").font(.subheadline)
                            Text((core.snapshot.channels.first { $0.id == action.channelId }.map { core.label($0) } ?? "Channel") + " · " + MessageText.day(action.createdAt)).font(.caption).foregroundStyle(.secondary)
                        }
                    }
                }
            }
        }.navigationTitle("Workspace & moderation").compactTitle()
        .onAppear {
            let workspace = core.snapshot.workspaces.first { $0.id == core.snapshot.workspace }
            name = workspace?.name ?? ""; slug = workspace?.slug ?? ""
        }
    }
}
struct ChannelEditView: View {
    @ObservedObject var core: CoreAdapter
    @State private var name = ""
    @State private var topic = ""
    var body: some View {
        Form {
            if let channel = core.currentChannel, core.canEdit(channel) {
                Section("Channel details") {
                    TextField("Name", text: $name)
                    TextField("Topic", text: $topic, axis: .vertical)
                    Button("Save channel") { core.command("edit_channel", payload: ["id": channel.id, "name": name, "topic": topic]) }.disabled(!core.snapshot.connected || name.isEmpty)
                }
                if core.canManageMembers { Button("Manage private members") { core.panel = .members } }
            } else { Text("Select an editable channel to continue.") }
        }.navigationTitle("Edit channel").compactTitle().onAppear { name = core.currentChannel?.name ?? ""; topic = core.currentChannel?.topic ?? "" }
        .onChange(of: core.snapshot.channel) { _, _ in name = core.currentChannel?.name ?? ""; topic = core.currentChannel?.topic ?? "" }
    }
}
struct ChannelMembersView: View {
    @ObservedObject var core: CoreAdapter
    var body: some View {
        List {
            Text("Only invited members can read this channel.").font(.caption).foregroundStyle(.secondary)
            if core.canManageMembers {
                ForEach(core.snapshot.people) { member in
                    Toggle(isOn: Binding(get: { core.snapshot.channelMembers.contains(member.id) }, set: { add in
                        core.command("channel_member", payload: ["user_id": member.id, "add": add])
                    })) {
                        HStack(spacing: 10) {
                            AvatarView(core: core, userId: member.id, size: 30)
                            VStack(alignment: .leading) { Text(member.user.displayName); Text(member.user.handle.map { "@" + $0 } ?? member.role).font(.caption).foregroundStyle(.secondary) }
                        }
                    }.disabled(!core.snapshot.connected || member.id == core.currentChannel?.createdBy)
                }
            } else { Text("Only the channel creator and authorized workspace administrators can change membership.").foregroundStyle(.secondary) }
        }.navigationTitle("Channel members").compactTitle()
    }
}
struct PinsView: View {
    @ObservedObject var core: CoreAdapter
    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 18) {
                if core.snapshot.pins.isEmpty { Text("No pinned messages yet.").foregroundStyle(.secondary).padding(.vertical, 40).frame(maxWidth: .infinity) }
                ForEach(core.snapshot.pins) { pin in
                    VStack(alignment: .leading, spacing: 10) {
                        Text(core.snapshot.channels.first { $0.id == pin.message.channelId }.map { core.label($0) } ?? "Conversation").font(.caption.bold()).foregroundStyle(.teal)
                        Text("Pinned by " + core.author(pin.pinnedBy)).font(.caption).foregroundStyle(.secondary)
                        MessageRow(core: core, cached: core.snapshot.messages.first { $0.id == pin.id } ?? CachedMessage(message: pin.message, status: DeliveryState(label: ""), reactions: []), thread: true)
                        Button("Open conversation / thread") { core.openMessage(pin.message) }.font(.caption.bold())
                    }.padding(14).background(Color.primary.opacity(0.035)).clipShape(RoundedRectangle(cornerRadius: 14))
                }
            }.padding(16)
        }.navigationTitle("Pinned messages").compactTitle()
    }
}
