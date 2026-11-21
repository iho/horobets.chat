import SwiftUI

struct ComposerView: View {
    @ObservedObject var core: CoreAdapter
    let thread: Bool
    @State private var preview = false
    @State private var topic = ""
    @State private var showTopic = false
    var draft: Binding<String> { Binding(get: { core.draft(thread) }, set: { core.setDraft($0, thread: thread) }) }
    var matches: [Completion] { MessageText.suggestions(core.draft(thread), users: core.snapshot.users, channels: core.snapshot.channels) }
    var canSend: Bool { !core.uploading && (!core.draft(thread).trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || !core.snapshot.attachments.isEmpty) }
    var body: some View {
        VStack(spacing: ChatLayout.gap) {
            if !preview && !matches.isEmpty { completions }
            if !core.snapshot.attachments.isEmpty { attachments }
            if core.uploading { HStack { ProgressView(); Text("Uploading…").font(.caption); Spacer() } }
            if showTopic && !thread { TextField("Topic (optional)", text: $topic).font(.subheadline).textFieldStyle(.roundedBorder) }
            if preview {
                ScrollView { MarkdownBody(core: core, text: core.draft(thread)).padding(8) }.frame(maxHeight: 160)
            }
            HStack(alignment: .bottom, spacing: 4) {
                Menu {
                    PhotoUploadButton(core: core, avatar: false).disabled(!core.snapshot.connected || core.uploading)
                    Button("Choose file") { core.chooseFile(avatar: false) }.disabled(!core.snapshot.connected || core.uploading)
                    Divider()
                    Menu("Formatting") {
                        Button("Bold") { append("**bold text**") }
                        Button("Italic") { append("*italic text*") }
                        Button("Inline code") { append("`code`") }
                        Button("Link") { append("[link text](https://example.com)") }
                        Button("Heading") { append("\n## Heading\n") }
                        Button("Quote") { append("\n> Quote\n") }
                        Button("List") { append("\n- List item\n") }
                        Button("Code block") { append("\n```\ncode\n```\n") }
                        Button("Mention someone") { append("@") }
                        Button("Link a channel") { append("#") }
                    }.disabled(preview)
                    if !thread { Button(showTopic ? "Hide topic" : "Add topic") { showTopic.toggle() } }
                    Button(preview ? "Write message" : "Preview Markdown") { preview.toggle() }
                } label: {
                    Label("Attach and format", systemImage: "plus").labelStyle(.iconOnly).accessibilityLabel("Attach and format")
                        .frame(width: ChatLayout.target, height: ChatLayout.target)
                }
                if preview {
                    Button("Write message") { preview = false }.font(.subheadline).frame(maxWidth: .infinity, minHeight: ChatLayout.target, alignment: .leading)
                } else {
                    TextField(thread ? "Reply…" : "Message…", text: draft, prompt: Text(thread ? "Reply…" : "Message…").foregroundColor(.secondary), axis: .vertical)
                        .lineLimit(5, reservesSpace: false).textFieldStyle(.plain).font(.body)
                        .composerTextInsets().frame(minHeight: ChatLayout.target)
                }
                Button {
                    if core.send(core.draft(thread), thread: thread, topic: topic) { core.setDraft("", thread: thread); preview = false }
                } label: {
                    Label(thread ? "Send reply" : "Send message", systemImage: "paperplane.fill").labelStyle(.iconOnly).accessibilityLabel(thread ? "Send reply" : "Send message")
                        .font(.headline).frame(width: ChatLayout.target, height: ChatLayout.target)
                        .foregroundStyle(canSend ? Color.primary : Color.secondary)
                        .background(canSend ? Color.teal.opacity(0.2) : Color.primary.opacity(0.04)).clipShape(Circle())
                }.buttonStyle(.plain).disabled(!canSend)
            }
        }
        .padding(.horizontal, ChatLayout.gutter).padding(.vertical, 8)
        .background(Color.primary.opacity(0.025))
        .overlay(alignment: .top) { Divider() }
        .onChange(of: core.draftKey(thread)) { _, _ in topic = ""; preview = false }
    }
    private func append(_ value: String) {
        let current = core.draft(thread)
        core.setDraft(current + (current.isEmpty || current.hasSuffix(" ") || value.hasPrefix("\n") ? "" : " ") + value, thread: thread)
    }
    private var completions: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                ForEach(matches) { match in
                    Button { core.setDraft(MessageText.complete(core.draft(thread), with: match.text), thread: thread) } label: {
                        HStack {
                            Text(match.text).font(.subheadline.bold())
                            Text(match.detail).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                            Spacer(minLength: 0)
                        }.padding(.vertical, 8).padding(.horizontal, 10)
                    }.buttonStyle(.plain)
                }
            }
        }.frame(maxHeight: 150).background(Color.teal.opacity(0.07)).clipShape(RoundedRectangle(cornerRadius: 10))
    }
    private var attachments: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 10) {
                ForEach(core.snapshot.attachments) { attachment in
                    HStack(spacing: 6) {
                        if let path = core.snapshot.imagePreviews[attachment.id] { CachedImage(path: path).frame(width: 44, height: 44).clipShape(RoundedRectangle(cornerRadius: 6)) }
                        Text(attachment.filename).font(.caption).lineLimit(1)
                        Button("×") { core.command("remove_attachment", payload: attachment.id) }.accessibilityLabel("Remove " + attachment.filename)
                    }.padding(8).background(Color.primary.opacity(0.06)).clipShape(RoundedRectangle(cornerRadius: 10))
                }
            }
        }
    }
}
