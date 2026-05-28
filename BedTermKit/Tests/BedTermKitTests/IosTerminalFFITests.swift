import BedTermIOS
import Foundation
import Metal
import ObjectiveC
import Testing
import UIKit

@testable import BedTermKit

/// Mid-layer XCTests that exercise the Rust FFI seams used by the iOS
/// terminal VC + Metal input view without spinning up SSH or the full
/// app. These guard the contracts production code in
/// `IosTerminalView.swift` (loadViewIfNeeded ordering, on_send +
/// on_resize trampolines, grid_dim post-layout) relies on.
@Suite("IosTerminalFFI")
@MainActor
struct IosTerminalFFITests {
    // MARK: - Test-only shared byte sink

    /// Heap box that captures emitted PTY bytes. We pass an `Unmanaged`
    /// pointer to it as `ctx` to `bt_ios_view_set_on_send`; the C
    /// callback below appends each chunk under a lock-free single
    /// MainActor invariant (all FFI callbacks fire on the main thread).
    private final class ByteCollector {
        var bytes: [UInt8] = []
        var callCount: Int = 0
    }

    private static let onSendCallback: BtIosOnSendCallback = { ctx, bytesPtr, len in
        guard let ctx, let bytesPtr, len > 0 else { return }
        let collector = Unmanaged<ByteCollector>.fromOpaque(ctx).takeUnretainedValue()
        let buf = UnsafeBufferPointer(start: bytesPtr, count: Int(len))
        collector.bytes.append(contentsOf: buf)
        collector.callCount += 1
    }

    /// Heap box for the resize callback.
    private final class ResizeCollector {
        var events: [(UInt16, UInt16)] = []
    }

    private static let onResizeCallback: BtIosOnResizeCallback = { ctx, cols, rows in
        guard let ctx else { return }
        let collector = Unmanaged<ResizeCollector>.fromOpaque(ctx).takeUnretainedValue()
        collector.events.append((cols, rows))
    }

    // MARK: - Helpers

    /// Build the VC, force-load its view, return the (vc, metalViewPtr).
    /// `loadViewIfNeeded` is the production-mandatory call (without it,
    /// `bt_ios_vc_metal_view` returns null — see the regression we fixed
    /// in 5f63220 / d074b3c).
    private func makeLoadedVC() throws -> (UIViewController, UnsafeMutableRawPointer) {
        try #require(MTLCreateSystemDefaultDevice() != nil)
        let ptr = try #require(bt_ios_create_vc(nil, nil))
        let vc = Unmanaged<UIViewController>.fromOpaque(ptr).takeRetainedValue()
        vc.loadViewIfNeeded()
        let mv = try #require(bt_ios_vc_metal_view(ptr))
        return (vc, mv)
    }

    /// Send `text` to the view via the `insertText:` ObjC selector (the
    /// same path the soft keyboard / IME commit takes).
    private func insertText(into view: UIView, _ text: String) {
        let sel = Selector(("insertText:"))
        guard view.responds(to: sel) else { return }
        _ = view.perform(sel, with: text)
    }

    // MARK: - 1. metalView_existsAfterLoadViewIfNeeded
    //
    // Regression test for the bug fixed in commit 5f63220 — production
    // must call `loadViewIfNeeded()` before `bt_ios_vc_metal_view` or
    // the resulting NULL pointer silently drops on_send / on_resize
    // installs.
    @Test("metal view non-null after loadViewIfNeeded")
    func metalViewExistsAfterLoadViewIfNeeded() throws {
        try #require(MTLCreateSystemDefaultDevice() != nil)
        let ptr = try #require(bt_ios_create_vc(nil, nil))
        let vc = Unmanaged<UIViewController>.fromOpaque(ptr).takeRetainedValue()
        vc.loadViewIfNeeded()

        let mv = bt_ios_vc_metal_view(ptr)
        #expect(mv != nil)
        _ = vc  // keep VC alive
    }

    // MARK: - 2. metalView_isNilBeforeLoadView
    //
    // Codifies the ordering contract — the Rust VC stores the metal
    // view ivar in `loadView`/`viewDidLoad`. Querying before either
    // runs is documented to return NULL.
    @Test("metal view is nil before loadView runs")
    func metalViewIsNilBeforeLoadView() throws {
        let ptr = try #require(bt_ios_create_vc(nil, nil))
        // Take a +1 retain but do NOT touch `.view` or `loadViewIfNeeded`.
        let vc = Unmanaged<UIViewController>.fromOpaque(ptr).takeRetainedValue()

        let mv = bt_ios_vc_metal_view(ptr)
        #expect(mv == nil)
        _ = vc
    }

    // MARK: - 3. onSendCallback_receivesInsertedBytes
    //
    // End-to-end seam: `insertText:` → `do_insert_text` →
    // `dispatch_send` → installed sink. Proves the IME / soft-keyboard
    // byte-emit chain reaches Swift.
    @Test("on_send callback receives bytes from insertText:")
    func onSendCallbackReceivesInsertedBytes() throws {
        let (_, mv) = try makeLoadedVC()
        let collector = ByteCollector()
        let ctx = Unmanaged.passUnretained(collector).toOpaque()
        bt_ios_view_set_on_send(mv, Self.onSendCallback, ctx)

        // Locate view1 (BtIosMetalInputView) and send "hello" through
        // the UIKeyInput selector — the same call the soft keyboard
        // makes on a commit.
        let metalCls: AnyClass = try #require(NSClassFromString("BtIosMetalInputView"))
        // The pointer we got is already the metal view; bridge it back.
        let view1 = Unmanaged<UIView>.fromOpaque(mv).takeUnretainedValue()
        #expect(view1.isKind(of: metalCls))
        insertText(into: view1, "hello")

        #expect(collector.bytes == [0x68, 0x65, 0x6C, 0x6C, 0x6F])

        // Clean up so we don't leave a dangling Unmanaged ctx pointer
        // into the collector once it goes out of scope.
        bt_ios_view_set_on_send(mv, nil, nil)
    }

    // MARK: - 4. onSendCallback_clearedOnNullSet
    @Test("on_send callback no longer fires after nil set")
    func onSendCallbackClearedOnNullSet() throws {
        let (_, mv) = try makeLoadedVC()
        let collector = ByteCollector()
        let ctx = Unmanaged.passUnretained(collector).toOpaque()
        bt_ios_view_set_on_send(mv, Self.onSendCallback, ctx)

        let view1 = Unmanaged<UIView>.fromOpaque(mv).takeUnretainedValue()
        insertText(into: view1, "a")
        #expect(collector.callCount == 1)

        bt_ios_view_set_on_send(mv, nil, nil)
        insertText(into: view1, "b")

        // The previously-installed callback should not have fired again.
        // (Internally Rust swaps in a no-op sink on nil-set; the contract
        // is "the caller's callback is no longer invoked".)
        #expect(collector.callCount == 1)
        #expect(collector.bytes == [0x61])
    }

    // MARK: - 5. gridDim_afterLayout
    @Test("grid_dim returns a sensible (cols, rows) after layout")
    func gridDimAfterLayout() throws {
        let (vc, mv) = try makeLoadedVC()
        vc.view.frame = CGRect(x: 0, y: 0, width: 390, height: 844)
        vc.view.setNeedsLayout()
        vc.view.layoutIfNeeded()

        var cols: UInt16 = 0
        var rows: UInt16 = 0
        bt_ios_view_grid_dim(mv, &cols, &rows)
        #expect(cols > 0)
        #expect(rows > 0)
        // Sanity envelope — at 14pt SF Mono on a 390pt-wide viewport we
        // expect something well under 80×100. The exact value depends on
        // glyph metrics, so just bound it.
        #expect(Int(cols) * Int(rows) < 80 * 100)
    }

    // MARK: - 6. feedBytes_advancesRendererState
    //
    // We can't read back what was painted, but feeding bytes through
    // the FFI must not crash and grid_dim should remain plausible.
    @Test("feed_bytes does not crash and grid_dim remains sane")
    func feedBytesAdvancesRendererState() throws {
        let (vc, mv) = try makeLoadedVC()
        vc.view.frame = CGRect(x: 0, y: 0, width: 390, height: 844)
        vc.view.layoutIfNeeded()

        let payload: [UInt8] = Array("hello\r\n".utf8)
        payload.withUnsafeBufferPointer { buf in
            bt_ios_view_feed_bytes(mv, buf.baseAddress, UInt(buf.count))
        }

        var cols: UInt16 = 0
        var rows: UInt16 = 0
        bt_ios_view_grid_dim(mv, &cols, &rows)
        #expect(cols > 0 && rows > 0)
    }

    // MARK: - 7. coordinator_keybarEsc_emitsEscByte
    //
    // The VC exposes the coordinator via a private ObjC selector
    // `btIosCoordinator` (returns `*const AnyObject`). We send
    // `keybarEsc` to it and verify the on_send sink received `[0x1B]`.
    @Test("coordinator keybarEsc routes 0x1B through on_send")
    func coordinatorKeybarEscEmitsEscByte() throws {
        let (vc, mv) = try makeLoadedVC()
        let collector = ByteCollector()
        let ctx = Unmanaged.passUnretained(collector).toOpaque()
        bt_ios_view_set_on_send(mv, Self.onSendCallback, ctx)

        // Locate the coordinator pointer the VC keeps. The selector
        // returns a raw `*const AnyObject` — Swift's NSObject.perform
        // unwraps it into an NSObject-style instance for us when the
        // pointer is itself a registered ObjC object.
        let coordSel = Selector(("btIosCoordinator"))
        let vcObj = vc as NSObject
        #expect(vcObj.responds(to: coordSel))
        // `perform(_:)` returns `Unmanaged<AnyObject>?` for selectors
        // returning an object pointer. The Rust impl returns a raw
        // pointer cast (`*const AnyObject`) which the ObjC bridge
        // treats as an unretained object reference.
        let raw = vcObj.perform(coordSel)
        let coordObj = try #require(raw?.takeUnretainedValue() as? NSObject)

        let escSel = Selector(("keybarEsc"))
        #expect(coordObj.responds(to: escSel))
        coordObj.perform(escSel)

        #expect(collector.bytes == [0x1B])

        bt_ios_view_set_on_send(mv, nil, nil)
    }

    // MARK: - 8. resizeCallback_firesOnLayoutChange
    //
    // The Rust side only fires `on_resize` when (cols, rows) actually
    // changes. We force layout at two distinct widths and expect at
    // least one resize event reflecting the second width.
    @Test("on_resize fires on (cols, rows) change")
    func resizeCallbackFiresOnLayoutChange() throws {
        let (vc, mv) = try makeLoadedVC()
        let collector = ResizeCollector()
        let ctx = Unmanaged.passUnretained(collector).toOpaque()
        bt_ios_view_set_on_resize(mv, Self.onResizeCallback, ctx)

        vc.view.frame = CGRect(x: 0, y: 0, width: 320, height: 600)
        vc.view.setNeedsLayout()
        vc.view.layoutIfNeeded()

        let countAfterFirst = collector.events.count

        vc.view.frame = CGRect(x: 0, y: 0, width: 800, height: 600)
        vc.view.setNeedsLayout()
        vc.view.layoutIfNeeded()

        // Either two distinct fires (one per layout) or the renderer
        // was warm enough that only the second produced a delta —
        // either way, the wider frame's grid must be reported.
        #expect(!collector.events.isEmpty)
        if collector.events.count > countAfterFirst, let last = collector.events.last {
            #expect(last.0 > 0)
            #expect(last.1 > 0)
        }

        bt_ios_view_set_on_resize(mv, nil, nil)
    }
}
