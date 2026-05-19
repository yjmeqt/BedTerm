import SwiftUI

/// Warp-style read-only list of command blocks. Renders through a
/// single Metal surface (`TerminalBlocksMetalView`) overlaid by
/// SwiftUI header strips inside a UIScrollView — see
/// `BlockListContainerView` for the architecture.
struct BlockListView: View {
    let session: TerminalSession

    var body: some View {
        BlockListContainerRepresentable(session: session)
            .background(Color("ShadcnBackground", bundle: .module))
    }
}

private struct BlockListContainerRepresentable: UIViewRepresentable {
    let session: TerminalSession

    func makeUIView(context: Context) -> BlockListContainerView {
        BlockListContainerView(session: session)
    }

    func updateUIView(_ view: BlockListContainerView, context: Context) {
        // Touch the observed blocks array so SwiftUI's Observation
        // dependency tracker connects this updateUIView to BlockStore.
        _ = session.blockStore.blocks.count
        view.refresh()
    }
}
