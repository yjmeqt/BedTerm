import SwiftUI

/// Warp-style read-only list of command blocks. The shared
/// `TerminalMetalUIView` renders block bodies, headers, and dividers
/// in a single Metal pass; this view provides the transparent gesture
/// overlay (pan, selection) and pushes per-frame layout to the shared
/// render surface.
struct BlockListView: View {
    let session: TerminalSession
    let metalView: TerminalMetalUIView

    var body: some View {
        BlockListContainerRepresentable(session: session, metalView: metalView)
    }
}

private struct BlockListContainerRepresentable: UIViewControllerRepresentable {
    let session: TerminalSession
    let metalView: TerminalMetalUIView

    func makeUIViewController(context: Context) -> BlockListContainerViewController {
        BlockListContainerViewController(session: session, sharedMetalView: metalView)
    }

    func updateUIViewController(
        _ controller: BlockListContainerViewController, context: Context
    ) {
        _ = session.blockStore.blocks.count
        controller.refresh()
    }

    static func dismantleUIViewController(
        _ controller: BlockListContainerViewController, coordinator: ()
    ) {
        controller.teardown()
    }
}
