import BedTermIOS
import SwiftUI
import UIKit

/// DEBUG-only test harness that mounts the Rust-backed terminal VC
/// **without** any SSH client / TerminalSession plumbing.
///
/// `BedTermUITests/RustTerminalSmokeUITests.swift` launches the app with
/// `-uitest-rustTerminalDirect <fixture>` and asserts against the a11y
/// identifiers registered by the Rust VC (`terminal.rust.root`,
/// `terminal.rust.metalView`, `keybar.run`, …).
///
/// The fixture name selects a byte stream that is pumped into the metal
/// view via `bt_ios_view_feed_bytes` once on appear. We deliberately do
/// **not** spin up a `TerminalSession` or Citadel client — the goal is to
/// exercise the Rust glyph renderer + chip / composer wiring deterministically.
public struct RustTerminalUITestHarness: View {
    /// Fixture identifier — currently the only baked-in fixture is
    /// `"hello"`, an ANSI byte stream that renders multi-colour text so
    /// the glyph-diversity assertion has something to chew on.
    let fixture: String

    public init(fixture: String = "hello") {
        self.fixture = fixture
    }

    public var body: some View {
        RustTerminalHarnessRepresentable(fixture: fixture)
            .ignoresSafeArea(edges: [.bottom, .horizontal])
            .navigationBarTitleDisplayMode(.inline)
    }
}

private struct RustTerminalHarnessRepresentable: UIViewControllerRepresentable {
    let fixture: String

    func makeUIViewController(context: Context) -> UIViewController {
        // Construct the Rust VC. We don't wire a back callback —
        // the harness sits at the root of the test-only nav stack so
        // there's nothing to pop back to.
        guard let raw = bt_ios_create_vc(nil, nil) else {
            return UIViewController()
        }
        let vc = Unmanaged<UIViewController>.fromOpaque(raw).takeUnretainedValue()
        context.coordinator.vcPtr = raw

        // Pump the fixture bytes once the view is laid out.
        // We schedule on the main runloop so the Rust VC has gone
        // through `viewDidLoad` + initial layout before we feed.
        let bytes = RustTerminalHarnessFixtures.bytes(for: fixture)
        DispatchQueue.main.async {
            if let mv = bt_ios_vc_metal_view(raw) {
                bytes.withUnsafeBytes { raw in
                    guard let base = raw.baseAddress?.assumingMemoryBound(to: UInt8.self) else {
                        return
                    }
                    bt_ios_view_feed_bytes(mv, base, UInt(raw.count))
                }
            }
        }
        return vc
    }

    func updateUIViewController(_ uiViewController: UIViewController, context: Context) {}

    func makeCoordinator() -> Coordinator { Coordinator() }

    static func dismantleUIViewController(
        _ uiViewController: UIViewController, coordinator: Coordinator
    ) {
        if let ptr = coordinator.vcPtr {
            bt_ios_release_vc(ptr)
        }
        coordinator.vcPtr = nil
    }

    final class Coordinator {
        var vcPtr: UnsafeMutableRawPointer?
    }
}

/// Baked-in byte fixtures used by `RustTerminalUITestHarness`. Kept as
/// inline `Data` (rather than bundled files) so the test harness has zero
/// resource-bundle ordering dependencies.
enum RustTerminalHarnessFixtures {
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
        // ESC[31m red, ESC[32m green, ESC[33m yellow, ESC[34m blue,
        // ESC[0m reset. Three lines, varying colours per line.
        let payload =
            "\u{1B}[31mHello\u{1B}[0m \u{1B}[32mRust\u{1B}[0m \u{1B}[33mTerminal\u{1B}[0m\r\n"
            + "\u{1B}[34mBedTerm\u{1B}[0m UI Test Harness\r\n"
            + "$ ls -la\r\n"
        return Data(payload.utf8)
    }()

    /// Wider colour palette stress fixture — every ANSI fg colour, useful
    /// when we want the glyph-diversity check to have many distinct
    /// foreground RGBs to sample.
    private static let ansiColorsFixture: Data = {
        var out = ""
        for code in 30...37 {
            out += "\u{1B}[\(code)mC\(code)\u{1B}[0m "
        }
        out += "\r\n"
        return Data(out.utf8)
    }()
}
