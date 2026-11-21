import SwiftUI

struct UnreadMessagesView: View {
    @ObservedObject var core: CoreAdapter
    var messages: [Message] { core.snapshot.unreadMessages ?? [] }
    var body: some View {
        List {
            Section {
                Text("Unread conversations in this workspace. Opening this view keeps messages unread until you open or mark them read.").font(.caption).foregroundStyle(.secondary)
                Button("Mark all as read") { core.command("mark_all_read") }.disabled(core.snapshot.unread.isEmpty)
            }
            if messages.isEmpty {
                Section {
                    Label("You're all caught up", systemImage: "checkmark.circle").font(.headline)
                    Text("New messages will appear here.").foregroundStyle(.secondary)
                }
            }
            ForEach(core.snapshot.channels.filter { (core.snapshot.unread[$0.id] ?? 0) > 0 }) { channel in
                Section {
                    ForEach(messages.filter { $0.channelId == channel.id }) { message in
                        Button { core.openMessage(message) } label: {
                            HStack(alignment: .top, spacing: 8) {
                                AvatarView(core: core, userId: message.userId, size: 32)
                                VStack(alignment: .leading, spacing: 4) {
                                    HStack {
                                        Text(core.author(message.userId)).font(.subheadline.bold())
                                        Spacer()
                                        Text(MessageText.time(message.createdAt)).font(.caption).foregroundStyle(.secondary)
                                    }
                                    Text(message.bodyMd.isEmpty ? "Attachment" : message.bodyMd).font(.subheadline).lineLimit(3)
                                    if message.parentId != nil { Text("Reply in thread").font(.caption).foregroundStyle(.secondary) }
                                }
                            }.foregroundStyle(.primary).padding(.vertical, 4)
                        }.buttonStyle(.plain)
                    }
                    Button("Open conversation") { core.openChannel(channel.id) }
                    Button("Mark conversation as read") { core.command("mark_channel_read", payload: channel.id) }
                } header: { Text("\(core.label(channel)) · \(core.snapshot.unread[channel.id] ?? 0)") }
            }
            if messages.count >= 200 { Text("Showing the latest 200 unread messages. Open a conversation to read its full history.").font(.caption).foregroundStyle(.secondary) }
        }.navigationTitle("Unread messages").compactTitle()
    }
}
