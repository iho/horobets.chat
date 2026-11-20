import SwiftUI

struct ComposerView: View {
    @ObservedObject var core: CoreAdapter
    let thread: Bool
    @State private var preview = false
    @State private var topic = ""
    @State private var showTopic = false
    var draft: Binding<String> { Binding(get: { core.draft(thread) }, set: { core.setDraft($0, thread: thread) }) }
    var matches: [Completion] { MessageText.suggestions(core.draft(thread), users: core.snapshot.users, channels: core.snapshot.channels) }
    var body: some View {
        VStack(spacing: 8) {
            if !preview && !matches.isEmpty { completions }
            if !core.snapshot.attachments.isEmpty { attachments }
            if core.uploading { HStack { ProgressView(); Text("Uploading…").font(.caption); Spacer() } }
            if showTopic && !thread { TextField("Topic (optional)", text: $topic).font(.caption).textFieldStyle(.roundedBorder) }
            if preview {
                ScrollView { MarkdownBody(core: core, text: core.draft(thread)).padding(8) }.frame(maxHeight: 180)
            } else {
                TextField(thread ? "Reply…" : "Write a message…", text: draft, axis: .vertical)
                    .lineLimit(6).textFieldStyle(.plain).padding(.horizontal, 12).padding(.vertical, 10)
                    .background(Color.primary.opacity(0.045)).clipShape(RoundedRectangle(cornerRadius: 12))
            }
            HStack(spacing: 16) {
                Menu("+ Attach") {
                    PhotoUploadButton(core: core, avatar: false)
                    Button("Choose file") { core.chooseFile(avatar: false) }
                }.disabled(!core.snapshot.connected || core.uploading)
                Menu("Format") {
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
                    if !thread { Button(showTopic ? "Hide topic" : "Add topic") { showTopic.toggle() } }
                }.disabled(preview)
                Button(preview ? "Write" : "Preview") { preview.toggle() }.font(.caption)
                Spacer(minLength: 0)
                Button("Send") {
                    if core.send(core.draft(thread), thread: thread, topic: topic) { core.setDraft("", thread: thread); preview = false }
                }.buttonStyle(.borderedProminent)
                    .disabled(core.uploading || (core.draft(thread).trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && core.snapshot.attachments.isEmpty))
            }.font(.caption)
        }
        .padding(12).background(Color.primary.opacity(0.025))
        .overlay(alignment: .top) { Rectangle().fill(Color.primary.opacity(0.1)).frame(height: 1) }
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
