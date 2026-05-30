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

        // -- Production path: RootCoordinator handles everything -----------
        let raw = Unmanaged.passUnretained(window).toOpaque()
        if UITestSupport.skipOnboarding {
            bt_ios_start_root_coordinator(raw, nil)
        } else {
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
            bt_ios_start_root_coordinator(raw, requestLocalNetwork)
        }
    }

    // MARK: - UI test fixture path

    /// Creates a terminal VC via `bt_ios_install_terminal_fixture` —
    /// a single Rust call that creates the VC, feeds baked-in ANSI bytes,
    /// and installs it as the window root.
    private func installRustTerminalFixture(_ window: UIWindow, fixture: String) {
        let bytes = TerminalHarnessFixtures.bytes(for: fixture)
        let raw = Unmanaged.passUnretained(window).toOpaque()
        bytes.withUnsafeBytes { buf in
            guard let base = buf.baseAddress?.assumingMemoryBound(to: UInt8.self) else {
                return
            }
            bt_ios_install_terminal_fixture(raw, base, UInt(buf.count))
        }
    }
}

// MARK: - Private helpers

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
