import SwiftUI
#if !SKIP
import UniformTypeIdentifiers
#if os(iOS)
import UIKit
import PhotosUI
#elseif os(macOS)
import AppKit
#endif
#endif

/// A shared 4-point rhythm; compact visuals keep native-sized hit areas.
enum ChatLayout {
    static let gutter: CGFloat = 12
    static let gap: CGFloat = 8
    static let avatar: CGFloat = 32
    static let target: CGFloat = 48
}

extension View {
    @ViewBuilder func composerTextInsets() -> some View {
        #if SKIP
        self.textFieldContentPadding(EdgeInsets(top: 12, leading: 0, bottom: 12, trailing: 0))
        #else
        self.padding(.vertical, 12)
        #endif
    }

    @ViewBuilder func selectableText() -> some View {
        #if SKIP
        self
        #else
        self.textSelection(.enabled)
        #endif
    }
    @ViewBuilder func compactTitle() -> some View {
        #if os(iOS) || SKIP
        self.navigationBarTitleDisplayMode(.inline)
        #else
        self
        #endif
    }
}

struct CachedImage: View {
    let path: String
    var body: some View {
        #if SKIP
        AsyncImage(url: MediaSource.fileURL(path)) { image in image.resizable().scaledToFit() }
            placeholder: { ProgressView() }
        #elseif os(iOS)
        if let image = UIImage(contentsOfFile: path) { Image(uiImage: image).resizable().scaledToFit() }
        else { Text("Image unavailable").font(.caption).foregroundStyle(.secondary) }
        #elseif os(macOS)
        if let image = NSImage(contentsOfFile: path) { Image(nsImage: image).resizable().scaledToFit() }
        else { Text("Image unavailable").font(.caption).foregroundStyle(.secondary) }
        #endif
    }
}
struct AvatarView: View {
    @ObservedObject var core: CoreAdapter
    let userId: String
    var size: CGFloat = 36
    var body: some View {
        Group {
            if let path = core.snapshot.avatars[userId] { CachedImage(path: path) }
            else {
                Text(String(core.author(userId).prefix(1)).uppercased()).font(.headline).foregroundStyle(.teal)
                    .frame(maxWidth: .infinity, maxHeight: .infinity).background(Color.teal.opacity(0.15))
            }
        }.frame(width: size, height: size).clipShape(Circle())
    }
}
struct AttachmentView: View {
    @ObservedObject var core: CoreAdapter
    let attachment: Attachment
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if let path = core.snapshot.imagePreviews[attachment.id] {
                Button { core.previewPath = path } label: {
                    CachedImage(path: path).frame(maxWidth: .infinity, maxHeight: 192).clipShape(RoundedRectangle(cornerRadius: 12))
                }.buttonStyle(.plain).accessibilityLabel("Open image: " + attachment.filename)
            } else if attachment.isImage {
                HStack { Text("Image preview unavailable").font(.caption); Button("Reload") { core.command("refresh") } }
            }
            Button {
                core.download(attachment)
            } label: {
                HStack(spacing: 8) {
                    Text("↓").font(.title3)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(attachment.filename).font(.subheadline).lineLimit(1)
                        Text("\((attachment.sizeBytes + 1023) / 1024) KB · Save original").font(.caption).foregroundStyle(.secondary)
                    }
                    Spacer(minLength: 0)
                }.padding(10).background(Color.primary.opacity(0.045)).clipShape(RoundedRectangle(cornerRadius: 10))
            }.buttonStyle(.plain).disabled(core.downloading)
        }
    }
}
struct PhotoUploadButton: View {
    @ObservedObject var core: CoreAdapter
    let avatar: Bool
    #if !SKIP && os(iOS)
    @State private var photo: PhotosPickerItem?
    #endif
    var body: some View {
        #if !SKIP && os(iOS)
        PhotosPicker(avatar ? "Change photo" : "Photo library", selection: $photo, matching: .images)
            .disabled(core.snapshot.userId == nil || core.uploading)
            .onChange(of: photo) { _, item in
                guard let item else { return }
                Task { @MainActor in
                    do {
                        if let data = try await item.loadTransferable(type: Data.self) {
                            let ext = item.supportedContentTypes.first?.preferredFilenameExtension ?? "jpg"
                            core.importPhoto(data, filename: "photo." + ext, avatar: avatar)
                        }
                    } catch { core.localError = error.localizedDescription }
                    photo = nil
                }
            }
        #else
        Button(avatar ? "Change photo" : "Photo library") { core.chooseFile(avatar: avatar, imagesOnly: true) }
            .disabled(core.snapshot.userId == nil || core.uploading)
        #endif
    }
}
struct FileImportModifier: ViewModifier {
    @ObservedObject var core: CoreAdapter
    var active = true
    @ViewBuilder func body(content: Content) -> some View {
        #if SKIP
        content
        #else
        content.fileImporter(isPresented: Binding(get: { active && core.importingFile }, set: { core.importingFile = $0 }), allowedContentTypes: core.importImages ? [.image] : [.item]) { result in
            switch result {
            case .success(let url): core.imported(url, avatar: core.importingAvatar)
            case .failure(let error): core.localError = error.localizedDescription
            }
        }
        #endif
    }
}
struct MediaSheet: View {
    @ObservedObject var core: CoreAdapter
    let path: String
    let export: Bool
    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(spacing: 20) {
                    if !export { CachedImage(path: path).frame(maxWidth: .infinity) }
                    #if !SKIP
                    if let url = MediaSource.fileURL(path) { ShareLink(item: url) { Text(export ? "Save or share file" : "Share preview") } }
                    #endif
                }.padding()
            }.navigationTitle(export ? "File ready" : "Image").compactTitle()
                .toolbar { Button("Done") { core.previewPath = nil; core.exportedPath = nil } }
        }
    }
}
struct MarkdownBody: View {
    @ObservedObject var core: CoreAdapter
    let text: String
    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            ForEach(MessageText.blocks(text)) { block in
                if block.kind.hasPrefix("code:") {
                    VStack(alignment: .leading, spacing: 4) {
                        if block.kind.count > 5 { Text(String(block.kind.dropFirst(5))).font(.caption).foregroundStyle(.secondary) }
                        ScrollView(.horizontal) { Text(block.text).font(.system(.footnote, design: .monospaced)).selectableText() }
                    }.padding(12).frame(maxWidth: .infinity, alignment: .leading).background(Color.primary.opacity(0.065)).clipShape(RoundedRectangle(cornerRadius: 10))
                } else if block.kind == "rule" {
                    Divider().padding(.vertical, 4)
                } else if block.kind == "tableRule" {
                    EmptyView()
                } else if block.kind == "table" {
                    HStack(alignment: .top, spacing: 10) {
                        ForEach(MessageText.cells(block.text)) { cell in
                            Text(LocalizedStringKey(MessageText.linked(cell.text.trimmingCharacters(in: .whitespaces), workspace: core.snapshot.workspace, users: core.snapshot.users, channels: core.snapshot.channels)))
                                .font(.subheadline).frame(maxWidth: .infinity, alignment: .leading)
                        }
                    }.padding(8).background(Color.primary.opacity(0.04))
                } else if block.kind == "quote" {
                    HStack(alignment: .top, spacing: 10) {
                        RoundedRectangle(cornerRadius: 2).fill(Color.teal.opacity(0.6)).frame(width: 3)
                        inline(block.text).foregroundStyle(.secondary)
                    }.fixedSize(horizontal: false, vertical: true)
                } else if block.kind.hasPrefix("heading") {
                    inline(block.text).font(block.kind == "heading1" ? .title2 : .headline).fontWeight(.semibold)
                } else {
                    inline(block.text)
                }
            }
        }
    }
    private func inline(_ text: String) -> some View {
        Text(LocalizedStringKey(MessageText.linked(text, workspace: core.snapshot.workspace, users: core.snapshot.users, channels: core.snapshot.channels)))
            .selectableText().frame(maxWidth: .infinity, alignment: .leading)
    }

}
