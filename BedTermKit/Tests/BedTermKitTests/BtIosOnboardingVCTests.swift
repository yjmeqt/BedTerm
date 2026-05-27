import BedTermCoreC
import Foundation
import Testing
import UIKit

@testable import BedTermKit

/// Mid-layer tests for the W23c Rust onboarding step VCs. Each test
/// constructs the relevant VC via FFI, forces a load, and asserts
/// the choice / continue callback fires when the corresponding ObjC
/// selector is invoked (the same selector UIKit fires on tap).
@Suite("BtIosOnboardingVC")
@MainActor
struct BtIosOnboardingVCTests {
    private final class ChoiceFlag {
        var fired = false
        var value: Int32 = -1
    }

    private final class ContinueFlag {
        var fired = false
    }

    private static let choiceCallback: @convention(c) (UnsafeMutableRawPointer?, Int32) -> Void = { ctx, choice in
        guard let ctx else { return }
        let box = Unmanaged<ChoiceFlag>.fromOpaque(ctx).takeUnretainedValue()
        box.fired = true
        box.value = choice
    }

    private static let continueCallback: @convention(c) (UnsafeMutableRawPointer?) -> Void = { ctx in
        guard let ctx else { return }
        let box = Unmanaged<ContinueFlag>.fromOpaque(ctx).takeUnretainedValue()
        box.fired = true
    }

    // MARK: - HostKindVC

    @Test("host-kind VC constructs and loads")
    func hostKindVCConstructsAndLoads() throws {
        let ptr = try #require(bt_ios_create_onboarding_host_kind_vc(nil, nil))
        let vc = Unmanaged<UIViewController>.fromOpaque(ptr).takeRetainedValue()
        vc.loadViewIfNeeded()
        let cls: AnyClass = try #require(NSClassFromString("BtIosOnboardingHostKindVC"))
        #expect(vc.isKind(of: cls))
        // Two choice buttons must exist in the view tree.
        let buttons = collectButtons(in: vc.view)
        #expect(buttons.count >= 2)
    }

    @Test("host-kind VC fires choice 0 for macOS selector")
    func hostKindVCFiresMacOSChoice() throws {
        let flag = ChoiceFlag()
        let ctx = Unmanaged.passRetained(flag).toOpaque()
        defer { Unmanaged<ChoiceFlag>.fromOpaque(ctx).release() }

        let ptr = try #require(bt_ios_create_onboarding_host_kind_vc(Self.choiceCallback, ctx))
        let vc = Unmanaged<UIViewController>.fromOpaque(ptr).takeRetainedValue()
        vc.loadViewIfNeeded()

        let sel = Selector(("choiceMacOSTapped"))
        let obj = vc as NSObject
        #expect(obj.responds(to: sel))
        obj.perform(sel)
        #expect(flag.fired)
        #expect(flag.value == 0)
    }

    @Test("host-kind VC fires choice 1 for other selector")
    func hostKindVCFiresOtherChoice() throws {
        let flag = ChoiceFlag()
        let ctx = Unmanaged.passRetained(flag).toOpaque()
        defer { Unmanaged<ChoiceFlag>.fromOpaque(ctx).release() }

        let ptr = try #require(bt_ios_create_onboarding_host_kind_vc(Self.choiceCallback, ctx))
        let vc = Unmanaged<UIViewController>.fromOpaque(ptr).takeRetainedValue()
        vc.loadViewIfNeeded()

        let sel = Selector(("choiceOtherTapped"))
        let obj = vc as NSObject
        #expect(obj.responds(to: sel))
        obj.perform(sel)
        #expect(flag.fired)
        #expect(flag.value == 1)
    }

    // MARK: - LocationVC

    @Test("location VC constructs and loads")
    func locationVCConstructsAndLoads() throws {
        let ptr = try #require(bt_ios_create_onboarding_location_vc(nil, nil))
        let vc = Unmanaged<UIViewController>.fromOpaque(ptr).takeRetainedValue()
        vc.loadViewIfNeeded()
        let cls: AnyClass = try #require(NSClassFromString("BtIosOnboardingLocationVC"))
        #expect(vc.isKind(of: cls))
        let buttons = collectButtons(in: vc.view)
        #expect(buttons.count >= 2)
    }

    @Test("location VC fires choice 0 for sameWifi selector")
    func locationVCFiresSameWifiChoice() throws {
        let flag = ChoiceFlag()
        let ctx = Unmanaged.passRetained(flag).toOpaque()
        defer { Unmanaged<ChoiceFlag>.fromOpaque(ctx).release() }

        let ptr = try #require(bt_ios_create_onboarding_location_vc(Self.choiceCallback, ctx))
        let vc = Unmanaged<UIViewController>.fromOpaque(ptr).takeRetainedValue()
        vc.loadViewIfNeeded()

        let sel = Selector(("choiceSameWifiTapped"))
        let obj = vc as NSObject
        #expect(obj.responds(to: sel))
        obj.perform(sel)
        #expect(flag.fired)
        #expect(flag.value == 0)
    }

    @Test("location VC fires choice 1 for remote selector")
    func locationVCFiresRemoteChoice() throws {
        let flag = ChoiceFlag()
        let ctx = Unmanaged.passRetained(flag).toOpaque()
        defer { Unmanaged<ChoiceFlag>.fromOpaque(ctx).release() }

        let ptr = try #require(bt_ios_create_onboarding_location_vc(Self.choiceCallback, ctx))
        let vc = Unmanaged<UIViewController>.fromOpaque(ptr).takeRetainedValue()
        vc.loadViewIfNeeded()

        let sel = Selector(("choiceRemoteTapped"))
        let obj = vc as NSObject
        #expect(obj.responds(to: sel))
        obj.perform(sel)
        #expect(flag.fired)
        #expect(flag.value == 1)
    }

    // MARK: - MacTutorialVC

    @Test("mac tutorial VC constructs, loads, and fires continue")
    func macTutorialVCFiresContinue() throws {
        let flag = ContinueFlag()
        let ctx = Unmanaged.passRetained(flag).toOpaque()
        defer { Unmanaged<ContinueFlag>.fromOpaque(ctx).release() }

        let ptr = try #require(bt_ios_create_onboarding_mac_tutorial_vc(Self.continueCallback, ctx))
        let vc = Unmanaged<UIViewController>.fromOpaque(ptr).takeRetainedValue()
        vc.loadViewIfNeeded()
        let cls: AnyClass = try #require(NSClassFromString("BtIosOnboardingMacTutorialVC"))
        #expect(vc.isKind(of: cls))

        let sel = Selector(("continueTapped"))
        let obj = vc as NSObject
        #expect(obj.responds(to: sel))
        obj.perform(sel)
        #expect(flag.fired)
    }

    // MARK: - LocalPermissionVC

    @Test("local-permission VC constructs in sameWifi variant and fires continue")
    func localPermissionVCSameWifiFiresContinue() throws {
        let flag = ContinueFlag()
        let ctx = Unmanaged.passRetained(flag).toOpaque()
        defer { Unmanaged<ContinueFlag>.fromOpaque(ctx).release() }

        let ptr = try #require(
            bt_ios_create_onboarding_local_permission_vc(Self.continueCallback, ctx, false)
        )
        let vc = Unmanaged<UIViewController>.fromOpaque(ptr).takeRetainedValue()
        vc.loadViewIfNeeded()
        let cls: AnyClass = try #require(NSClassFromString("BtIosOnboardingLocalPermissionVC"))
        #expect(vc.isKind(of: cls))

        let sel = Selector(("continueTapped"))
        let obj = vc as NSObject
        #expect(obj.responds(to: sel))
        obj.perform(sel)
        #expect(flag.fired)
    }

    @Test("local-permission VC constructs in remote variant")
    func localPermissionVCRemoteVariant() throws {
        let ptr = try #require(
            bt_ios_create_onboarding_local_permission_vc(nil, nil, true)
        )
        let vc = Unmanaged<UIViewController>.fromOpaque(ptr).takeRetainedValue()
        vc.loadViewIfNeeded()
        let cls: AnyClass = try #require(NSClassFromString("BtIosOnboardingLocalPermissionVC"))
        #expect(vc.isKind(of: cls))
    }

    // MARK: - helpers

    private func collectButtons(in view: UIView?) -> [UIButton] {
        guard let view else { return [] }
        var out: [UIButton] = []
        if let btn = view as? UIButton { out.append(btn) }
        for sub in view.subviews {
            out.append(contentsOf: collectButtons(in: sub))
        }
        return out
    }
}
