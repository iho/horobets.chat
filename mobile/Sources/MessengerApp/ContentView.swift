import SwiftUI

struct ContentView: View {
    @StateObject private var core = CoreAdapter.shared
    @AppStorage("darkMode") private var darkMode = true
    var body: some View {
        VStack(spacing: 0) {
            ErrorBanner(core: core)
            if core.snapshot.userId == nil { SignInView(core: core) }
            else { ChannelListView(core: core) }
        }
        .tint(darkMode ? Color.teal : Color(red: 0.07, green: 0.42, blue: 0.36)).preferredColorScheme(darkMode ? .dark : .light)
        .sheet(item: $core.panel, onDismiss: { if core.panel == nil { core.command("select_thread", payload: NSNull()) } }) { panel in PanelView(core: core, panel: panel) }
        .modifier(FileImportModifier(core: core, active: core.panel == nil))
        .modifier(MediaPresentation(core: core, active: core.panel == nil))
        .environment(\.openURL, OpenURLAction { url in core.openReference(url) ? .handled : .systemAction })
        .onChange(of: core.navigation) { _, _ in core.updateActivity() }
        .onChange(of: core.panel) { _, _ in core.updateActivity() }
        .onChange(of: core.snapshot.userId) { _, value in if value == nil { core.panel = nil; core.navigation = [] } }
    }
}
struct ErrorBanner: View {
    @ObservedObject var core: CoreAdapter
    var body: some View {
        if let message = core.localError ?? core.snapshot.error {
            HStack(alignment: .top) {
                Text(message).font(.caption).frame(maxWidth: .infinity, alignment: .leading)
                Button("Dismiss") { core.localError = nil; core.command("clear_error") }.font(.caption.bold())
            }.padding(12).foregroundStyle(.red).background(Color.red.opacity(0.1))
        } else if let notice = core.snapshot.notice {
            HStack {
                Text(notice == "File saved" ? "Download ready" : notice).font(.caption).frame(maxWidth: .infinity, alignment: .leading)
                Button("Dismiss") { core.command("clear_error") }.font(.caption)
            }.padding(10).background(Color.teal.opacity(0.1))
        }
    }
}
struct MediaPresentation: ViewModifier {
    @ObservedObject var core: CoreAdapter
    var active = true
    func body(content: Content) -> some View {
        content
            .sheet(isPresented: Binding(get: { active && core.previewPath != nil }, set: { if !$0 { core.previewPath = nil } })) {
                MediaSheet(core: core, path: core.previewPath ?? "", export: false)
            }
            .sheet(isPresented: Binding(get: { active && core.exportedPath != nil }, set: { if !$0 { core.exportedPath = nil } })) {
                MediaSheet(core: core, path: core.exportedPath ?? "", export: true)
            }
    }
}
