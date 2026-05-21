import SwiftUI

/// Warp-style read-only list of command blocks. Renders through a
/// single Metal surface (`TerminalBlocksMetalView`) overlaid by
/// SwiftUI header strips inside a UIScrollView — see
/// `BlockListContainerViewController` for the architecture.
struct BlockListView: View {
    let session: TerminalSession

    var body: some View {
        BlockListContainerRepresentable(session: session)
            .background(Color("ShadcnBackground", bundle: .module))
    }
}

private struct BlockListContainerRepresentable: UIViewControllerRepresentable {
    let session: TerminalSession

    func makeUIViewController(context: Context) -> BlockListContainerViewController {
        BlockListContainerViewController(session: session)
    }

    func updateUIViewController(
        _ controller: BlockListContainerViewController, context: Context
    ) {
        // Touch the observed blocks array so SwiftUI's Observation
        // tracker connects updateUIViewController to BlockStore.
        _ = session.blockStore.blocks.count
        controller.refresh()
    }

    static func dismantleUIViewController(
        _ controller: BlockListContainerViewController, coordinator: ()
    ) {
        // Break the CADisplayLink → controller retain cycle before
        // SwiftUI drops its ref. Without this, the controller (and the
        // TerminalSession it holds strongly) would leak per session.
        controller.teardown()
    }
}
