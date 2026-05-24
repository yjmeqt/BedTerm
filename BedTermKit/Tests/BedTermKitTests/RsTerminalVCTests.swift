import BedTermCoreC
import Testing
import UIKit

@testable import BedTermKit

@Suite("RsTerminalViewController lifecycle")
@MainActor
struct RsTerminalVCTests {
    @Test("create returns non-nil and view loads without crashing")
    func createAndLoadView() throws {
        let ptr = bt_rs_terminal_create_vc(nil, nil)
        try #require(ptr != nil)
        let vc = Unmanaged<UIViewController>.fromOpaque(ptr!).takeRetainedValue()

        // Force viewDidLoad to run by accessing .view.
        _ = vc.view

        // Force a layout pass so viewDidLayoutSubviews runs against a real frame.
        vc.view.frame = CGRect(x: 0, y: 0, width: 390, height: 844)
        vc.view.layoutIfNeeded()

        #expect(vc.view.subviews.count >= 2)
    }
}
