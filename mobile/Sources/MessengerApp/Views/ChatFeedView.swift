import SwiftUI

struct ChatFeedView: View {
    @ObservedObject var core: CoreAdapter
    let channelId: String
    @State private var search = ""
    @State private var searching = false
    var channel: Channel? { core.snapshot.channels.first { $0.id == channelId } }
    var body: some View {
        VStack(spacing: 0) {
            conversationHeader
            if searching {
                HStack(spacing: 10) {
                    TextField("Search cached messages", text: $search).textFieldStyle(.roundedBorder).autocorrectionDisabled()
                    Button("Done") { search = ""; searching = false }.font(.caption)
                }.padding(.horizontal, 16).padding(.vertical, 8)
            }
            if !core.snapshot.connected { Text("Offline · saved history is available, new messages are queued").font(.caption).foregroundStyle(.secondary).padding(8) }
            if !search.isEmpty { searchResults }
            else { ConversationFeed(core: core, thread: false) }
            ComposerView(core: core, thread: false)
        }
        .navigationTitle(channel.map { core.label($0) } ?? "Conversation").compactTitle()
        .onChange(of: search) { _, value in core.command("search", payload: value) }
        .onAppear { if core.snapshot.channel != channelId { core.selectChannel(channelId) } }
        .onChange(of: channelId) { _, value in search = ""; if core.snapshot.channel != value { core.selectChannel(value) } }
    }
    private var conversationHeader: some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 3) {
                Text(channel?.isSaved == true ? "Private notes and files · only you" : channel?.topic ?? (channel?.isPrivate == true ? "Private conversation" : "Team conversation"))
                    .font(.caption).foregroundStyle(.secondary).lineLimit(2)
                Text(core.snapshot.channelMembers.count == 1 ? "1 member" : "\(core.snapshot.channelMembers.count) members").font(.caption2).foregroundStyle(.secondary)
            }
            Spacer(minLength: 0)
            Button("Search") { searching.toggle(); if !searching { search = "" } }.font(.caption)
            Menu("Details") {
                Button("Pinned messages") { core.panel = .pins }
                if let channel, core.canEdit(channel) { Button("Edit channel") { core.panel = .channel } }
                if core.canManageMembers { Button("Manage members") { core.panel = .members } }
                Button("Workspace & people") { core.panel = .workspace }
                Button("Refresh") { core.command("refresh") }
            }.font(.subheadline)
        }.padding(.horizontal, 16).padding(.vertical, 10).background(Color.primary.opacity(0.035))
    }
    private var searchResults: some View {
        List(core.snapshot.searchResults) { message in
            Button { core.openMessage(message) } label: {
                VStack(alignment: .leading, spacing: 5) {
                    Text(core.snapshot.channels.first { $0.id == message.channelId }.map { core.label($0) } ?? "Conversation").font(.caption).foregroundStyle(.secondary)
                    Text(message.bodyMd).lineLimit(3).foregroundStyle(.primary)
                }.padding(.vertical, 4)
            }
        }.overlay { if core.snapshot.searchResults.isEmpty { Text("No messages found").foregroundStyle(.secondary) } }
    }
}

struct ConversationFeed: View {
    @ObservedObject var core: CoreAdapter
    let thread: Bool
    @State private var following = true
    var messages: [CachedMessage] { thread ? core.snapshot.threadMessages : core.snapshot.messages.filter { $0.message.parentId == nil } }
    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    if !thread && core.snapshot.hasOlder { Button("Load earlier messages") { core.command("load_older") }.padding() }
                    if messages.isEmpty {
                        VStack(spacing: 10) {
                            Text(thread ? "Keep the conversation going" : "Start the conversation").font(.headline)
                            Text(thread ? "Reply below to add to this thread." : "Send a message, photo, or file below.").font(.subheadline).foregroundStyle(.secondary)
                        }.frame(maxWidth: .infinity).padding(.vertical, 48)
                    }
                    ForEach(Array(messages.enumerated()), id: \.element.id) { index, message in
                        if index == 0 || MessageText.day(message.message.createdAt) != MessageText.day(messages[index - 1].message.createdAt) {
                            HStack {
                                Rectangle().fill(Color.primary.opacity(0.12)).frame(height: 1)
                                Text(MessageText.day(message.message.createdAt)).font(.caption).foregroundStyle(.secondary).fixedSize()
                                Rectangle().fill(Color.primary.opacity(0.12)).frame(height: 1)
                            }.padding(.vertical, 18)
                        }
                        MessageRow(core: core, cached: message, thread: thread,
                            grouped: index > 0 && MessageText.grouped(messages[index - 1].message, message.message))
                            .id(message.id)
                    }
                    Color.clear.frame(height: 1).onAppear { following = true }.onDisappear { following = false }
                }.padding(.horizontal, 16).padding(.bottom, 16)
            }
            .onChange(of: messages.last?.id) { old, value in
                if let value, old == nil || following || messages.last?.message.userId == core.snapshot.userId { proxy.scrollTo(value, anchor: .bottom) }
            }
            .onAppear { if let id = messages.last?.id { proxy.scrollTo(id, anchor: .bottom) } }
            .overlay(alignment: .bottomTrailing) {
                if !following { Button("Latest ↓") { if let id = messages.last?.id { proxy.scrollTo(id, anchor: .bottom) } }.font(.caption.bold()).buttonStyle(.borderedProminent).padding(12) }
            }
        }
    }
}

struct MessageRow: View {
    @ObservedObject var core: CoreAdapter
    let cached: CachedMessage
    let thread: Bool
    var grouped = false
    @State private var editing = false
    @State private var deleting = false
    @State private var reacting = false
    @State private var editedBody = ""
    var message: Message { cached.message }
    var sent: Bool { cached.status.label.isEmpty && message.deletedAt == nil }
    var body: some View {
        HStack(alignment: .top, spacing: 10) {
            if grouped { Color.clear.frame(width: 34, height: 1) }
            else {
                Button { core.openProfile(message.userId) } label: { AvatarView(core: core, userId: message.userId, size: 34) }.buttonStyle(.plain)
            }
            VStack(alignment: .leading, spacing: 7) {
                if !grouped {
                    HStack(alignment: .firstTextBaseline, spacing: 8) {
                        Button(core.author(message.userId)) { core.openProfile(message.userId) }.font(.subheadline.bold()).foregroundStyle(.primary)
                        Text(MessageText.time(message.createdAt)).font(.caption2).foregroundStyle(.secondary)
                        Spacer(minLength: 0)
                        if sent { Menu("•••") { actions }.accessibilityLabel("Message actions") }
                    }
                } else if sent {
                    HStack { Spacer(); Menu("•••") { actions }.font(.caption).accessibilityLabel("Message actions") }.frame(height: 12)
                }
                if core.isPinned(cached.id) { Text("📌 Pinned").font(.caption).foregroundStyle(.secondary) }
                if let topic = message.topicTag { Text(topic).font(.caption).foregroundStyle(.teal) }
                if message.deletedAt != nil { Text("Message deleted").italic().foregroundStyle(.secondary) }
                else {
                    if !message.bodyMd.isEmpty { MarkdownBody(core: core, text: message.bodyMd) }
                    ForEach(core.snapshot.messageAttachments[message.id] ?? []) { attachment in AttachmentView(core: core, attachment: attachment) }
                    if !cached.reactions.isEmpty { reactionPills }
                    if !thread, let count = core.snapshot.replyCounts[cached.id], count > 0 {
                        Button("\(count) replies →") { core.openThread(cached.id) }.font(.caption.bold())
                    }
                    if message.editedAt != nil { Text("edited").font(.caption2).foregroundStyle(.secondary) }
                }
                if !cached.status.label.isEmpty {
                    HStack {
                        Text(cached.status.label).font(.caption).foregroundStyle(.secondary)
                        if cached.status.failed { Button("Retry") { core.command("retry", payload: cached.id) }.font(.caption) }
                    }
                }
            }.frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(.top, grouped ? 4.0 : 16.0).padding(.bottom, 6)
        .contextMenu { if sent { actions } }
        .sheet(isPresented: $editing) { editSheet }
        .sheet(isPresented: $reacting) { ReactionPicker(core: core, messageId: cached.id, close: { reacting = false }) }
        .alert("Delete message?", isPresented: $deleting) {
            Button("Cancel", role: .cancel) {}
            Button("Delete", role: .destructive) { core.event("delete_message", payload: ["message_id": cached.id]) }
        } message: { Text("This message will be removed for everyone in the conversation.") }
    }
    @ViewBuilder private var actions: some View {
        Button("Copy message") { core.copyText(message.bodyMd) }
        if !thread { Button("Reply in thread") { core.openThread(cached.id) } }
        Button("Add reaction") { reacting = true }.disabled(!core.snapshot.connected)
        if core.canPin(message) {
            Button(core.isPinned(cached.id) ? "Unpin" : "Pin message") { core.event("pin_message", payload: ["message_id": cached.id, "pinned": !core.isPinned(cached.id)]) }.disabled(!core.snapshot.connected)
        }
        if message.userId == core.snapshot.userId { Button("Edit message") { editedBody = message.bodyMd; editing = true } }
        if core.canDelete(message) { Button("Delete message", role: .destructive) { deleting = true } }
    }
    private var reactionPills: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) {
                ForEach(cached.reactions, id: \.emoji) { reaction in
                    Button("\(reaction.emoji) \(reaction.count)") { core.event("toggle_reaction", payload: ["message_id": cached.id, "emoji": reaction.emoji]) }
                        .font(.caption).buttonStyle(.bordered).tint(reaction.mine ? .teal : .gray)
                }
            }
        }
    }
    private var editSheet: some View {
        NavigationStack {
            VStack(spacing: 16) {
                TextEditor(text: $editedBody).frame(minHeight: 160).padding(8)
                Button("Save changes") { core.event("edit_message", payload: ["message_id": cached.id, "body_md": editedBody]); editing = false }
                    .buttonStyle(.borderedProminent).disabled(!core.snapshot.connected || editedBody.utf8.count > 64000)
            }.padding().navigationTitle("Edit message").compactTitle().toolbar { Button("Cancel") { editing = false } }
        }
    }
}

struct ReactionPicker: View {
    @ObservedObject var core: CoreAdapter
    let messageId: String
    let close: () -> Void
    var body: some View {
        NavigationStack {
            ScrollView {
                LazyVGrid(columns: [GridItem(.flexible()), GridItem(.flexible()), GridItem(.flexible()), GridItem(.flexible())], spacing: 12) {
                    ForEach(reactionChoices, id: \.self) { emoji in
                        Button(emoji) { core.event("toggle_reaction", payload: ["message_id": messageId, "emoji": emoji]); close() }
                            .font(.largeTitle).frame(maxWidth: .infinity).padding(10).background(Color.primary.opacity(0.045)).clipShape(RoundedRectangle(cornerRadius: 12))
                    }
                }.padding()
            }.navigationTitle("Add a reaction").compactTitle().toolbar { Button("Done") { close() } }
        }
    }
}
