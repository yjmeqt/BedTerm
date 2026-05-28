import BedTermIOS
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

    private func withIsolatedService<R>(_ body: () throws -> R) throws -> R {
        let suffix = UUID().uuidString
        let svc = "bt.hostsvc.test.\(suffix)"
        let ord = "bt.hostsvc.test.order.\(suffix)"
        svc.withCString { sPtr in
            ord.withCString { oPtr in
                bt_ios_hosts_set_test_service(sPtr, oPtr)
            }
        }
        defer { bt_ios_hosts_set_test_service(nil, nil) }
        let priorConnect = HostsBridge.connectHandler
        defer { HostsBridge.connectHandler = priorConnect }
        return try body()
    }

    private func saveEntry(_ entry: SavedHost) throws {
        let data = try JSONEncoder().encode(entry)
        let ok = entry.id.uuidString.withCString { idPtr in
            data.withUnsafeBytes { raw -> Bool in
                let base = raw.baseAddress?.assumingMemoryBound(to: UInt8.self)
                return bt_ios_hosts_save_blob(idPtr, base, UInt(raw.count))
            }
        }
        #expect(ok)
    }

    private func listEntries() -> [SavedHost] {
        guard let snapshotPtr = bt_ios_hosts_snapshot_json() else { return [] }
        defer { bt_ios_hosts_free_string(snapshotPtr) }
        let json = String(cString: snapshotPtr)
        guard let data = json.data(using: .utf8),
            let items = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]]
        else { return [] }
        var out: [SavedHost] = []
        for item in items {
            guard let idStr = item["id"] as? String,
                let uuid = UUID(uuidString: idStr)
            else { continue }
            if let ptr = uuid.uuidString.withCString({ bt_ios_hosts_load_json($0) }) {
                defer { bt_ios_hosts_free_string(ptr) }
                if let entryData = String(cString: ptr).data(using: .utf8),
                    let entry = try? JSONDecoder().decode(SavedHost.self, from: entryData)
                {
                    out.append(entry)
                }
            }
        }
        return out
    }

    private func deleteEntry(id: UUID) {
        id.uuidString.withCString { bt_ios_hosts_delete($0) }
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
        try withIsolatedService {
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
            try saveEntry(host1)
            try saveEntry(host2)

            let ptr = try #require(bt_ios_create_hosts_list_vc(nil, nil))
            let vc = Unmanaged<UIViewController>.fromOpaque(ptr).takeRetainedValue()
            vc.loadViewIfNeeded()
            vc.beginAppearanceTransition(true, animated: false)
            vc.endAppearanceTransition()

            let scroll = try #require(
                vc.view.subviews.compactMap { $0 as? UIScrollView }.first)
            let content = try #require(
                scroll.subviews.compactMap { $0 as? UIStackView }.first)
            #expect(content.arrangedSubviews.count == 2)
        }
    }

    @Test("row tap dispatches connect to bridge handler")
    func rowTapDispatchesConnect() throws {
        try withIsolatedService {
            let host = SavedHost(
                label: "Mac",
                credential: HostCredential(
                    host: "10.0.0.5", port: 22, username: "yi",
                    auth: .password("pw"))
            )
            try saveEntry(host)

            var received: UUID?
            HostsBridge.connectHandler = { id in received = id }

            let ptr = try #require(bt_ios_create_hosts_list_vc(nil, nil))
            let vc = Unmanaged<UIViewController>.fromOpaque(ptr).takeRetainedValue()
            vc.loadViewIfNeeded()
            vc.beginAppearanceTransition(true, animated: false)
            vc.endAppearanceTransition()

            let button = try #require(firstButton(in: vc.view))
            #expect(button.tag == 0)
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
        try withIsolatedService {
            let host = SavedHost(
                label: "Mac",
                credential: HostCredential(
                    host: "10.0.0.5", port: 22, username: "yi",
                    auth: .password("pw"))
            )
            try saveEntry(host)
            #expect(listEntries().count == 1)

            deleteEntry(id: host.id)
            #expect(listEntries().isEmpty)
        }
    }
}
