import BedTermCoreC
import Foundation
import Testing
import UIKit

@testable import BedTermKit

/// Mid-layer tests for the W23b Rust Settings VC and the Swift-side
/// `SettingsBridge` round-trip the VC reads/writes through.
@Suite("BtIosSettingsVC")
@MainActor
struct BtIosSettingsVCTests {
    // MARK: - SettingsBridge round-trip

    @Test("settings bridge round-trips reserveTopSafeArea")
    func settingsBridgeRoundTripsReserveTopSafeArea() throws {
        let defaults = try #require(UserDefaults(suiteName: UUID().uuidString))
        let store = BedTermSettings(defaults: defaults)
        let previous = SettingsBridge.observableHandle
        SettingsBridge.observableHandle = store
        defer { SettingsBridge.observableHandle = previous }

        btSwiftSettingsSetReserveTopSafeArea(false)
        #expect(!store.reserveTopSafeAreaInAltScreen)
        #expect(!btSwiftSettingsGetReserveTopSafeArea())

        btSwiftSettingsSetReserveTopSafeArea(true)
        #expect(store.reserveTopSafeAreaInAltScreen)
        #expect(btSwiftSettingsGetReserveTopSafeArea())
    }

    @Test("settings bridge round-trips showCommandBlocks")
    func settingsBridgeRoundTripsShowCommandBlocks() throws {
        let defaults = try #require(UserDefaults(suiteName: UUID().uuidString))
        let store = BedTermSettings(defaults: defaults)
        let previous = SettingsBridge.observableHandle
        SettingsBridge.observableHandle = store
        defer { SettingsBridge.observableHandle = previous }

        btSwiftSettingsSetShowCommandBlocks(true)
        #expect(store.showCommandBlocks)
        #expect(btSwiftSettingsGetShowCommandBlocks())

        btSwiftSettingsSetShowCommandBlocks(false)
        #expect(!store.showCommandBlocks)
        #expect(!btSwiftSettingsGetShowCommandBlocks())
    }

    @Test("settings bridge returns defaults when handle is unset")
    func settingsBridgeReturnsDefaultsWhenHandleUnset() {
        let previous = SettingsBridge.observableHandle
        SettingsBridge.observableHandle = nil
        defer { SettingsBridge.observableHandle = previous }

        // Defaults documented in `BedTermSettings`: reserve = true, blocks = false.
        #expect(btSwiftSettingsGetReserveTopSafeArea())
        #expect(!btSwiftSettingsGetShowCommandBlocks())
    }

    // MARK: - VC construction

    @Test("settings VC constructs and loads")
    func settingsVCConstructsAndLoads() throws {
        let ptr = try #require(bt_ios_create_settings_vc(nil, nil))
        let vc = Unmanaged<UIViewController>.fromOpaque(ptr).takeRetainedValue()
        vc.loadViewIfNeeded()

        // Class identity matches the ObjC name declared by `define_class!`.
        let cls: AnyClass = try #require(NSClassFromString("BtIosSettingsViewController"))
        #expect(vc.isKind(of: cls))

        // After load, the view tree should have a scroll view containing
        // a content stack with two arranged sections.
        let root = vc.view
        #expect(root != nil)
        let scrolls = root?.subviews.compactMap { $0 as? UIScrollView } ?? []
        #expect(scrolls.count == 1)
        // Inside the scroll view: at least one UIStackView with >= 2
        // arranged subviews (the two sections).
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

        // Invoke the doneTapped selector directly (UIKit would fire it
        // through the bar-button target/action).
        let sel = Selector(("doneTapped"))
        let obj = vc as NSObject
        #expect(obj.responds(to: sel))
        obj.perform(sel)
        #expect(flag.fired)
    }
}
