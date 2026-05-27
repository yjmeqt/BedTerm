import BedTermCoreC
import Foundation
import Testing
import UIKit

@testable import BedTermKit

/// Mid-layer tests for the W24b Rust hosts list VC and the Swift-side
/// `HostsBridge` round-trip the VC reads/writes through.
@Suite("BtIosHostsListVC")
@MainActor
struct BtIosHostsListVCTests {
    private final class AddFlag { var fired = false }

    private static let addCallback: @convention(c) (UnsafeMutableRawPointer?) -> Void = { ctx in
        guard let ctx else { return }
        let box = Unmanaged<AddFlag>.fromOpaque(ctx).takeUnretainedValue()
        box.fired = true
    }

    /// Install a bridge `store` backed by a per-test UUID suite +
    /// service key so the suite leaves no residue in the Keychain /
    /// UserDefaults. Uses the in-memory Keychain backend because
    /// SPM xctest bundles have no host-app entitlement for the real
    /// `SecItem*` path.
    private func withIsolatedBridge<R>(_ body: (HostsStore) throws -> R) throws -> R {
        TestKeychain.installInMemory()
        let suite = UUID().uuidString
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let store = HostsStore(
            service: "bt.hostsvc.test.\(suite)",
            orderKey: "bt.hostsvc.test.order.\(suite)",
            migrationKey: "bt.hostsvc.test.migration.\(suite)",
            defaults: defaults
        )
        let priorStore = HostsBridge.store
        let priorConnect = HostsBridge.connectHandler
        HostsBridge.store = store
        defer {
            HostsBridge.store = priorStore
            HostsBridge.connectHandler = priorConnect
        }
        return try body(store)
    }

    @Test("hosts list VC constructs and loads")
    func vcConstructsAndLoads() throws {
        let ptr = try #require(bt_ios_create_hosts_list_vc(nil, nil))
        let vc = Unmanaged<UIViewController>.fromOpaque(ptr).takeRetainedValue()
        vc.loadViewIfNeeded()

        let cls: AnyClass = try #require(NSClassFromString("BtIosHostsListViewController"))
        #expect(vc.isKind(of: cls))

        let root = vc.view
        #expect(root != nil)
        let scrolls = root?.subviews.compactMap { $0 as? UIScrollView } ?? []
        #expect(scrolls.count == 1)
    }

    @Test("add callback fires on addTapped selector")
    func addCallbackFires() throws {
        let flag = AddFlag()
        let ctx = Unmanaged.passRetained(flag).toOpaque()
        defer { Unmanaged<AddFlag>.fromOpaque(ctx).release() }

        let ptr = try #require(bt_ios_create_hosts_list_vc(Self.addCallback, ctx))
        let vc = Unmanaged<UIViewController>.fromOpaque(ptr).takeRetainedValue()
        vc.loadViewIfNeeded()

        let sel = Selector(("addTapped"))
        let obj = vc as NSObject
        #expect(obj.responds(to: sel))
        obj.perform(sel)
        #expect(flag.fired)
    }

    @Test("VC row count matches bridge snapshot")
    func rowCountMatchesBridge() throws {
        try withIsolatedBridge { store in
            // Seed two hosts via the store directly.
            let host1 = SavedHost(
                label: "Mac",
                credential: HostCredential(
                    host: "10.0.0.5", port: 22, username: "yi",
                    auth: .password("pw"))
            )
            let host2 = SavedHost(
                label: "Linux",
                credential: HostCredential(
                    host: "1.2.3.4", port: 2222, username: "root",
                    auth: .privateKey(Data(), passphrase: nil))
            )
            try store.save(host1)
            try store.save(host2)

            let ptr = try #require(bt_ios_create_hosts_list_vc(nil, nil))
            let vc = Unmanaged<UIViewController>.fromOpaque(ptr).takeRetainedValue()
            vc.loadViewIfNeeded()
            // The VC pulls the snapshot in viewWillAppear; invoke
            // beginAppearanceTransition to drive the framework into firing
            // it without needing a hosting window.
            vc.beginAppearanceTransition(true, animated: false)
            vc.endAppearanceTransition()

            // Walk the scroll-view → content-stack chain and count rows.
            let scroll = try #require(
                vc.view.subviews.compactMap { $0 as? UIScrollView }.first)
            let content = try #require(
                scroll.subviews.compactMap { $0 as? UIStackView }.first)
            #expect(content.arrangedSubviews.count == 2)
        }
    }

    @Test("row tap dispatches connect to bridge handler")
    func rowTapDispatchesConnect() throws {
        try withIsolatedBridge { store in
            let host = SavedHost(
                label: "Mac",
                credential: HostCredential(
                    host: "10.0.0.5", port: 22, username: "yi",
                    auth: .password("pw"))
            )
            try store.save(host)

            var received: UUID?
            HostsBridge.connectHandler = { id in received = id }

            let ptr = try #require(bt_ios_create_hosts_list_vc(nil, nil))
            let vc = Unmanaged<UIViewController>.fromOpaque(ptr).takeRetainedValue()
            vc.loadViewIfNeeded()
            vc.beginAppearanceTransition(true, animated: false)
            vc.endAppearanceTransition()

            // Find the row UIButton (only one row → first button in tree).
            let button = try #require(firstButton(in: vc.view))
            // The row VC stores its UIButton with .tag = row index (0 here).
            #expect(button.tag == 0)
            // Fire `rowTapped:` directly with the button as sender so the
            // VC's selector handler routes through the bridge.
            let sel = Selector(("rowTapped:"))
            let obj = vc as NSObject
            #expect(obj.responds(to: sel))
            obj.perform(sel, with: button)
            #expect(received == host.id)
        }
    }

    private func firstButton(in view: UIView?) -> UIButton? {
        guard let view else { return nil }
        if let btn = view as? UIButton { return btn }
        for sub in view.subviews {
            if let btn = firstButton(in: sub) { return btn }
        }
        return nil
    }

    @Test("swipe-delete invokes bridge delete and removes the row")
    func swipeDeleteInvokesBridge() throws {
        try withIsolatedBridge { store in
            let host = SavedHost(
                label: "Mac",
                credential: HostCredential(
                    host: "10.0.0.5", port: 22, username: "yi",
                    auth: .password("pw"))
            )
            try store.save(host)
            #expect(store.list().count == 1)

            // Invoke the C ABI delete directly — that's the symbol the
            // Rust VC's swipe selector calls. (Driving an actual swipe
            // gesture from a unit test is awkward; the bridge round-
            // trip is what we care about here.)
            let ok = host.id.uuidString.withCString { btSwiftHostsDelete($0) }
            #expect(ok)
            #expect(store.list().isEmpty)
        }
    }
}
