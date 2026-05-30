import BedTermIOS
import BedTermKit
import UIKit

final class SceneDelegate: UIResponder, UIWindowSceneDelegate {
    var window: UIWindow?

    func scene(
        _ scene: UIScene,
        willConnectTo session: UISceneSession,
        options connectionOptions: UIScene.ConnectionOptions
    ) {
        guard let windowScene = scene as? UIWindowScene else { return }
        let window = UIWindow(windowScene: windowScene)
        self.window = window

        // -- UI test: direct Rust terminal VC mount (no SSH / onboarding) ---
        if let fixture = UITestSupport.rustTerminalDirectFixture {
            installRustTerminalFixture(window, fixture: fixture)
            return
        }

        // -- UI test shortcut or completed onboarding: skip to hosts root ---
        if bt_ios_settings_onboarding_completed() || UITestSupport.skipOnboarding {
            let raw = Unmanaged.passUnretained(window).toOpaque()
            bt_ios_start_root_coordinator(raw)
        } else {
            let box = OnboardingBox { [weak self] in self?.startRustCoordinator(window) }
            let ctx = Unmanaged.passRetained(box).toOpaque()
            let cb: @convention(c) (UnsafeMutableRawPointer?) -> Void = { ctx in
                guard let ctx else { return }
                let b = Unmanaged<OnboardingBox>.fromOpaque(ctx).takeRetainedValue()
                b.fire()
            }

            // Inject the Local Network permission probe as a callback instead
            // of reaching through a global @_cdecl symbol. Rust calls this
            // with (ctx, completion); we run the Bonjour probe and fire the
            // completion when the OS resolves the prompt.
            let requestLocalNetwork:
                @convention(c) (
                    UnsafeMutableRawPointer?,
                    (@convention(c) (UnsafeMutableRawPointer?) -> Void)?
                ) -> Void = { ctx, completion in
                    let bridge = LocalNetworkBridgeBox(ctx: ctx, completion: completion)
                    Task { @MainActor in
                        _ = await LocalNetworkPrewarmer.shared.requestPermission()
                        bridge.fire()
                    }
                }

            guard let raw = bt_ios_create_onboarding_flow_vc(cb, ctx, requestLocalNetwork) else {
                Unmanaged<OnboardingBox>.fromOpaque(ctx).release()
                return
            }
            let vc = Unmanaged<UIViewController>.fromOpaque(raw).takeRetainedValue()
            window.rootViewController = vc as? UINavigationController ?? vc
            window.makeKeyAndVisible()
        }
    }

    private func startRustCoordinator(_ window: UIWindow) {
        let raw = Unmanaged.passUnretained(window).toOpaque()
        bt_ios_start_root_coordinator(raw)
    }

    // MARK: - UI test fixture path

    /// Mount the Rust terminal VC directly, feed it a baked-in ANSI byte
    /// fixture, and skip onboarding + Hosts + SSH entirely. Used by
    /// `RustTerminalSmokeUITests` via `-uitest-rustTerminalDirect <fixture>`.
    private func installRustTerminalFixture(_ window: UIWindow, fixture: String) {
        let nav = UINavigationController()
        guard let raw = bt_ios_create_vc(nil, nil) else {
            window.rootViewController = nav
            window.makeKeyAndVisible()
            return
        }
        let vc = Unmanaged<UIViewController>.fromOpaque(raw).takeUnretainedValue()
        nav.setViewControllers([vc], animated: false)

        // Pump fixture bytes after the VC has gone through viewDidLoad.
        let bytes = TerminalHarnessFixtures.bytes(for: fixture)
        DispatchQueue.main.async {
            if let mv = bt_ios_vc_metal_view(raw) {
                bytes.withUnsafeBytes { buf in
                    guard let base = buf.baseAddress?.assumingMemoryBound(to: UInt8.self) else {
                        return
                    }
                    bt_ios_view_feed_bytes(mv, base, UInt(buf.count))
                }
            }
        }
        window.rootViewController = nav
        window.makeKeyAndVisible()
    }
}

// MARK: - Private helpers

private final class OnboardingBox {
    let fire: () -> Void
    init(fire: @escaping () -> Void) { self.fire = fire }
}

/// Sendable wrapper that pins a C function pointer + opaque ctx so the
/// Swift 6 task-isolation checker accepts crossing them into a `Task`.
private final class LocalNetworkBridgeBox: @unchecked Sendable {
    let ctx: UnsafeMutableRawPointer?
    let completion: (@convention(c) (UnsafeMutableRawPointer?) -> Void)?

    init(
        ctx: UnsafeMutableRawPointer?,
        completion: (@convention(c) (UnsafeMutableRawPointer?) -> Void)?
    ) {
        self.ctx = ctx
        self.completion = completion
    }

    func fire() {
        completion?(ctx)
    }
}

// MARK: - Baked-in terminal fixture data

/// Baked-in byte fixtures for the Rust terminal UI test harness.
/// Kept as inline `Data` (rather than bundled files) so the test harness
/// has zero resource-bundle ordering dependencies.
enum TerminalHarnessFixtures {
    /// Returns the byte stream for the named fixture, or a sensible
    /// default when the name is unknown.
    static func bytes(for name: String) -> Data {
        switch name {
        case "hello":
            return helloFixture
        case "ansiColors":
            return ansiColorsFixture
        case "blank":
            return Data()
        default:
            return helloFixture
        }
    }

    /// Three lines of ANSI-coloured text — enough to exercise multiple
    /// SGR foreground colours so the glyph-diversity assertion sees more
    /// than one non-background pixel value.
    private static let helloFixture: Data = {
        let payload =
            "\u{1B}[31mHello\u{1B}[0m \u{1B}[32mRust\u{1B}[0m \u{1B}[33mTerminal\u{1B}[0m\r\n"
            + "\u{1B}[34mBedTerm\u{1B}[0m UI Test Harness\r\n"
            + "$ ls -la\r\n"
        return Data(payload.utf8)
    }()

    /// Wider colour palette stress fixture — every ANSI fg colour.
    private static let ansiColorsFixture: Data = {
        var out = ""
        for code in 30...37 {
            out += "\u{1B}[\(code)mC\(code)\u{1B}[0m "
        }
        out += "\r\n"
        return Data(out.utf8)
    }()
}
