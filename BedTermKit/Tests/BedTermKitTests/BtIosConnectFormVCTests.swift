import BedTermCoreC
import Foundation
import Testing
import UIKit

@testable import BedTermKit

/// Mid-layer tests for the W24c Rust connect-form VC and the Swift-side
/// `ConnectFormBridge` round-trip the VC reads/writes through.
@Suite("BtIosConnectFormVC")
@MainActor
struct BtIosConnectFormVCTests {
    private func withIsolatedBridge<R>(_ body: (HostsStore) throws -> R) throws -> R {
        let suffix = UUID().uuidString
        let svc = "bt.connectformvc.test.\(suffix)"
        let ord = "bt.connectformvc.test.order.\(suffix)"
        svc.withCString { sPtr in
            ord.withCString { oPtr in
                bt_ios_hosts_set_test_service(sPtr, oPtr)
            }
        }
        defer { bt_ios_hosts_set_test_service(nil, nil) }
        let store = HostsStore()
        return try body(store)
    }

    @Test("connect-form VC constructs in add mode")
    func vcConstructsInAddMode() throws {
        let ptr = try #require(bt_ios_create_connect_form_vc(nil, false, nil, nil, nil))
        let vc = Unmanaged<UIViewController>.fromOpaque(ptr).takeRetainedValue()
        vc.loadViewIfNeeded()

        let cls: AnyClass = try #require(NSClassFromString("BtIosConnectFormViewController"))
        #expect(vc.isKind(of: cls))
        #expect(vc.navigationItem.title == "New Host")
    }

    @Test("connect-form VC constructs in edit mode and pulls prefill")
    func vcConstructsInEditMode() throws {
        try withIsolatedBridge { store in
            let host = SavedHost(
                label: "Mac",
                credential: HostCredential(
                    host: "10.0.0.5", port: 22, username: "yi", auth: .password("pw")
                )
            )
            try store.save(host)

            let ptr = try #require(
                host.id.uuidString.withCString { idPtr in
                    bt_ios_create_connect_form_vc(idPtr, false, nil, nil, nil)
                })
            let vc = Unmanaged<UIViewController>.fromOpaque(ptr).takeRetainedValue()
            vc.loadViewIfNeeded()
            #expect(vc.navigationItem.title == "Edit Host")
        }
    }

    @Test("cancel button dispatches the cancel callback")
    func cancelCallbackFires() throws {
        final class Flag { var fired = false }
        let flag = Flag()
        let ctx = Unmanaged.passRetained(flag).toOpaque()
        defer { Unmanaged<Flag>.fromOpaque(ctx).release() }
        let onCancel: @convention(c) (UnsafeMutableRawPointer?) -> Void = { ctx in
            guard let ctx else { return }
            Unmanaged<Flag>.fromOpaque(ctx).takeUnretainedValue().fired = true
        }
        let ptr = try #require(bt_ios_create_connect_form_vc(nil, false, nil, onCancel, ctx))
        let vc = Unmanaged<UIViewController>.fromOpaque(ptr).takeRetainedValue()
        vc.loadViewIfNeeded()

        let sel = Selector(("cancelTapped"))
        let obj = vc as NSObject
        #expect(obj.responds(to: sel))
        obj.perform(sel)
        #expect(flag.fired)
    }

    @Test("save button surfaces validation errors when fields blank")
    func saveValidationSurfacesError() throws {
        let ptr = try #require(bt_ios_create_connect_form_vc(nil, false, nil, nil, nil))
        let vc = Unmanaged<UIViewController>.fromOpaque(ptr).takeRetainedValue()
        vc.loadViewIfNeeded()

        // Save without filling out the form — the VC should NOT crash and
        // should surface a visible error label.
        let sel = Selector(("saveTapped"))
        let obj = vc as NSObject
        #expect(obj.responds(to: sel))
        obj.perform(sel)

        // Walk the view tree to look for a UILabel with the validation copy.
        func walk(_ view: UIView, _ out: inout [String]) {
            if let label = view as? UILabel, let text = label.text, !text.isEmpty {
                out.append(text)
            }
            view.subviews.forEach { walk($0, &out) }
        }
        var texts: [String] = []
        walk(vc.view, &texts)
        #expect(texts.contains { $0.contains("required") })
    }
}
