import SwiftUI
struct ThreadDetailView: View {
    @ObservedObject var core: CoreAdapter
    var body: some View {
        VStack(spacing: 0) {
            if core.snapshot.thread == core.openingThread {
                ConversationFeed(core: core, thread: true)
                ComposerView(core: core, thread: true)
            } else { ProgressView("Opening thread…").frame(maxWidth: .infinity, maxHeight: .infinity) }
        }.navigationTitle("Thread").compactTitle()
    }
}
