import BedTermCoreC
import Foundation
import Testing
import UIKit

@testable import BedTermKit

/// Construction + Done-callback smoke tests for the Rust Settings VC.
/// Persistence lives in the Rust `settings_store` module and writes
/// straight into `NSUserDefaults.standard`; round-tripping that store
/// from Swift would just be exercising Foundation, so it isn't covered
/// here.
@Suite("BtIosSettingsVC")
@MainActor
struct BtIosSettingsVCTests {
    @Test("settings VC constructs and loads")
    func settingsVCConstructsAndLoads() throws {
        let ptr = try #require(bt_ios_create_settings_vc(nil, nil))
        let vc = Unmanaged<UIViewController>.fromOpaque(ptr).takeRetainedValue()
        vc.loadViewIfNeeded()

        let cls: AnyClass = try #require(NSClassFromString("BtIosSettingsViewController"))
        #expect(vc.isKind(of: cls))

        let root = vc.view
        #expect(root != nil)
        let scrolls = root?.subviews.compactMap { $0 as? UIScrollView } ?? []
        #expect(scrolls.count == 1)
        if let scroll = scrolls.first {
            let stacks = scroll.subviews.compactMap { $0 as? UIStackView }
            #expect(!stacks.isEmpty)
            if let content = stacks.first {
                #expect(content.arrangedSubviews.count >= 2)
            }
        }
    }

    @Test("settings VC done callback fires on doneTapped")
    func settingsVCDoneCallbackFires() throws {
        final class DoneFlag { var fired = false }
        let flag = DoneFlag()
        let ctx = Unmanaged.passRetained(flag).toOpaque()
        defer { Unmanaged<DoneFlag>.fromOpaque(ctx).release() }

        let callback: @convention(c) (UnsafeMutableRawPointer?) -> Void = { ctx in
            guard let ctx else { return }
            Unmanaged<DoneFlag>.fromOpaque(ctx).takeUnretainedValue().fired = true
        }

        let ptr = try #require(bt_ios_create_settings_vc(callback, ctx))
        let vc = Unmanaged<UIViewController>.fromOpaque(ptr).takeRetainedValue()
        vc.loadViewIfNeeded()

        let sel = Selector(("doneTapped"))
        let obj = vc as NSObject
        #expect(obj.responds(to: sel))
        obj.perform(sel)
        #expect(flag.fired)
    }

    @Test("persisted settings round-trip through the C ABI")
    func settingsRoundTripThroughCABI() {
        // The store is a singleton wrapping NSUserDefaults.standard, so
        // capture + restore whatever the device already has.
        let prevReserve = bt_ios_settings_reserve_top_safe_area()
        let prevBlocks = bt_ios_settings_show_command_blocks()
        defer {
            bt_ios_settings_set_reserve_top_safe_area(prevReserve)
            bt_ios_settings_set_show_command_blocks(prevBlocks)
        }

        bt_ios_settings_set_reserve_top_safe_area(false)
        #expect(!bt_ios_settings_reserve_top_safe_area())
        bt_ios_settings_set_reserve_top_safe_area(true)
        #expect(bt_ios_settings_reserve_top_safe_area())

        bt_ios_settings_set_show_command_blocks(true)
        #expect(bt_ios_settings_show_command_blocks())
        bt_ios_settings_set_show_command_blocks(false)
        #expect(!bt_ios_settings_show_command_blocks())
    }
}
