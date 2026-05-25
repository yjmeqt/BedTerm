import BedTermCoreC
import Testing
import UIKit

@testable import BedTermKit

@Suite("IosTerminalViewController lifecycle")
@MainActor
struct IosTerminalVCTests {
    @Test("create returns non-nil and view loads without crashing")
    func createAndLoadView() throws {
        let ptr = try #require(bt_ios_create_vc(nil, nil))
        let vc = Unmanaged<UIViewController>.fromOpaque(ptr).takeRetainedValue()

        // Force viewDidLoad to run.
        _ = vc.view

        // Force a layout pass.
        vc.view.frame = CGRect(x: 0, y: 0, width: 390, height: 844)
        vc.view.layoutIfNeeded()

        // view2 (UITextView) is always added; view1 (Metal) requires a GPU.
        #expect(vc.view.subviews.count >= 1)
    }

    @Test("view1 is BtIosMetalInputView when a Metal device is available")
    func view1IsMetal() throws {
        try #require(MTLCreateSystemDefaultDevice() != nil)

        let ptr = try #require(bt_ios_create_vc(nil, nil))
        let vc = Unmanaged<UIViewController>.fromOpaque(ptr).takeRetainedValue()
        _ = vc.view

        // The Metal-backed view is registered under the ObjC name "BtIosMetalInputView".
        let metalCls: AnyClass = try #require(NSClassFromString("BtIosMetalInputView"))
        let hasMetalSubview = vc.view.subviews.contains { $0.isKind(of: metalCls) }
        #expect(hasMetalSubview)
    }

    @Test("view1 accepts UITextInput selectors")
    func view1AcceptsInput() throws {
        try #require(MTLCreateSystemDefaultDevice() != nil)
        let ptr = try #require(bt_ios_create_vc(nil, nil))
        let vc = Unmanaged<UIViewController>.fromOpaque(ptr).takeRetainedValue()
        _ = vc.view

        let metalCls: AnyClass = try #require(NSClassFromString("BtIosMetalInputView"))
        let view1 = try #require(vc.view.subviews.first { $0.isKind(of: metalCls) })

        // view1 should claim it can become first responder.
        #expect(view1.canBecomeFirstResponder)

        // view1 should respond to UIKeyInput / UITextInput selectors.
        #expect(view1.responds(to: Selector("insertText:")))
        #expect(view1.responds(to: Selector("deleteBackward")))
        #expect(view1.responds(to: Selector("hasText")))
        #expect(view1.responds(to: Selector("selectedTextRange")))
        #expect(view1.responds(to: Selector("beginningOfDocument")))
    }
}
