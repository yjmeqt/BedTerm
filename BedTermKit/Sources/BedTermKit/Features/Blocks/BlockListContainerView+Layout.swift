import SwiftUI
import UIKit

/// Layout helpers split off the main controller for SwiftLint
/// `file_length`. The methods own no state of their own — they read /
/// mutate the controller's stored host dictionaries and content view.
@MainActor
extension BlockListContainerViewController {
    func mountHeader(for block: Block, blockTop: CGFloat, width: CGFloat) {
        let host: UIHostingController<BlockHeader>
        if let existing = headerHosts[block.id] {
            existing.rootView = BlockHeader(block: block)
            host = existing
        } else {
            host = UIHostingController(rootView: BlockHeader(block: block))
            host.view.backgroundColor = .clear
            headerHosts[block.id] = host
            addChild(host)
            contentView.addSubview(host.view)
            host.didMove(toParent: self)
        }
        let leftInset = BlockPanelStyle.cellLeftInsetPt
        host.view.frame = CGRect(
            x: leftInset, y: blockTop,
            width: width - leftInset, height: headerHeightPt)
    }

    /// Place (or remove) the hairline divider that sits in the gap
    /// above `block`. The very first block has no divider.
    func placeDivider(
        for block: Block, isFirst: Bool, rect: CGRect, color: UIColor
    ) {
        guard !isFirst else {
            if let stale = dividerHosts.removeValue(forKey: block.id) {
                stale.removeFromSuperview()
            }
            return
        }
        let divider: UIView
        if let existing = dividerHosts[block.id] {
            divider = existing
        } else {
            divider = UIView()
            divider.isUserInteractionEnabled = false
            contentView.addSubview(divider)
            dividerHosts[block.id] = divider
        }
        divider.backgroundColor = color
        divider.frame = rect
    }

    func recycleHostsAndDividers(keep: Set<UInt64>) {
        for (id, host) in headerHosts where !keep.contains(id) {
            host.willMove(toParent: nil)
            host.view.removeFromSuperview()
            host.removeFromParent()
            headerHosts.removeValue(forKey: id)
        }
        for (id, divider) in dividerHosts where !keep.contains(id) {
            divider.removeFromSuperview()
            dividerHosts.removeValue(forKey: id)
        }
    }

    func resolveDividerColor() -> UIColor {
        UIColor(named: "ShadcnBorder", in: .module, compatibleWith: view.traitCollection)?
            .withAlphaComponent(0.5) ?? UIColor.separator
    }
}
