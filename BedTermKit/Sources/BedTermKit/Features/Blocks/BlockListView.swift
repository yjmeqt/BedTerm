import SwiftUI

/// Warp-style read-only list of command blocks. Each row's body is
/// rendered through `BlockMetalView` — same Metal pipeline the live
/// terminal uses, so ANSI colours / wide chars / underlines all show up
/// correctly.
///
/// Input still flows through the underlying terminal (existing Classic
/// behaviour). This view is a *retrospective* visualisation; running
/// blocks update live as output streams in.
struct BlockListView: View {
    let session: TerminalSession

    @State private var expandedBlockID: Block.ID?

    var body: some View {
        let blocks = session.blockStore.blocks
        ScrollViewReader { proxy in
            ScrollView {
                if blocks.isEmpty {
                    emptyState
                        .frame(maxWidth: .infinity)
                        .padding(.top, 48)
                } else {
                    LazyVStack(spacing: 10) {
                        ForEach(blocks) { block in
                            BlockRowView(
                                block: block,
                                core: session.terminalCore,
                                isExpanded: expandedBlockID == block.id,
                                onToggle: { toggle(block) }
                            )
                            .id(block.id)
                        }
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 16)
                }
            }
            .onChange(of: blocks.count) { _, _ in
                guard let last = blocks.last else { return }
                withAnimation(.smooth(duration: 0.22)) {
                    proxy.scrollTo(last.id, anchor: .bottom)
                }
            }
        }
        .background(Color("ShadcnBackground", bundle: .module))
    }

    private func toggle(_ block: Block) {
        expandedBlockID = expandedBlockID == block.id ? nil : block.id
    }

    @ViewBuilder
    private var emptyState: some View {
        VStack(spacing: 12) {
            Image(systemName: "rectangle.split.1x2")
                .font(.title)
                .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
            VStack(spacing: 4) {
                Text("No command blocks yet")
                    .font(.headline)
                    .foregroundStyle(Color("ShadcnPrimary", bundle: .module))
                Text(
                    """
                    Run a command at the remote shell prompt. Each prompt → \
                    command → output cycle becomes a block here.
                    """
                )
                .font(.footnote)
                .multilineTextAlignment(.center)
                .foregroundStyle(Color("ShadcnMutedForeground", bundle: .module))
            }
            .padding(.horizontal, 32)
        }
    }
}
