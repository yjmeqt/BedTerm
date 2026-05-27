import UIKit
import XCTest

/// End-to-end UI tests that exercise the full production stack —
/// `HostsScreen` → `HostsViewModel` → `ConnectAttempt` → injected
/// `MockSSHClient` → `TerminalSession` → `IosTerminalView` →
/// `bt_ios_view_feed_bytes` → Rust glyph renderer.
///
/// Unlike `RustTerminalSmokeUITests` (which mounts the Rust VC
/// directly via `-uitest-rustTerminalDirect` and bypasses SSH +
/// `TerminalSession` entirely), this suite drives the same code path
/// production users hit: tap a saved-host row, watch the connecting
/// overlay clear, assert the metal view paints scripted bytes from the
/// mock SSH client.
///
/// Launch args:
///   `-uitest-skipOnboarding`     — skip onboarding flow
///   `-uitest-stubSSH <script>`   — swap `CitadelSSHClient` for a scripted
///                                  `MockSSHClient` (scripts:
///                                  `hello` / `ansiColors` / `prompt` /
///                                  `echo`)
///   `-uitest-injectStubHost`     — prepend an in-memory fake "stub" row
///                                  to the saved-hosts list so the test
///                                  has something to tap without filling
///                                  out the add-host form.
final class RustTerminalEndToEndSSHUITests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    // MARK: - Helpers

    @MainActor
    private func element(_ app: XCUIApplication, _ identifier: String) -> XCUIElement {
        app.descendants(matching: .any).matching(identifier: identifier).firstMatch
    }

    @MainActor
    private func launch(stubScript: String) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments += [
            "-uitest-skipOnboarding",
            "-uitest-stubSSH", stubScript,
            "-uitest-injectStubHost"
        ]
        app.launch()
        return app
    }

    /// Stable injected-stub host identifier. Mirrors the UUID baked
    /// into `BedTermApp.stubHost()` so the test can scope queries to
    /// the right row when prior keychain state surfaces extra hosts.
    private static let stubHostID = "00000000-0000-0000-0000-00000000B0B0"

    /// Tap the injected stub host's Connect button and wait for the
    /// Rust VC's metal view to mount. Returns the metal-view element.
    @MainActor
    private func connectToStub(_ app: XCUIApplication) -> XCUIElement {
        // Scope to the row whose container carries the stub UUID — the
        // simulator keychain can hold unrelated rows from prior runs
        // and `hosts.row.connect.firstMatch` would otherwise dispatch
        // to whichever of those happened to render first.
        let stubRow = app.buttons["hosts.row.\(Self.stubHostID)"]
        XCTAssertTrue(
            stubRow.waitForExistence(timeout: 8),
            "Injected stub host row must appear in the saved-hosts list"
        )
        let connect = stubRow.descendants(matching: .button)
            .matching(identifier: "hosts.row.connect").firstMatch
        XCTAssertTrue(
            connect.waitForExistence(timeout: 4),
            "Stub host row must expose its Connect chip"
        )
        connect.tap()

        let metal = element(app, "terminal.rust.metalView")
        if !metal.waitForExistence(timeout: 10) {
            XCTFail(
                "Rust metal view must mount after stub Connect tap. "
                    + "Hierarchy:\n\(app.debugDescription)"
            )
        }
        return metal
    }

    /// Count distinct (R,G,B) triplets in `image` to a cap of `cap`.
    /// Mirrors `RustTerminalSmokeUITests.distinctColors` — a coarse
    /// "the view actually rendered something non-uniform" signal.
    private func distinctColors(in image: UIImage, cap: Int = 32) -> Int {
        guard let cg = image.cgImage,
            let provider = cg.dataProvider,
            let data = provider.data,
            let ptr = CFDataGetBytePtr(data)
        else { return 0 }
        let bpp = cg.bitsPerPixel / 8
        let bytesPerRow = cg.bytesPerRow
        let width = cg.width
        let height = cg.height
        let stride = max(4, min(width, height) / 32)
        var seen = Set<UInt32>()
        var row = 0
        while row < height {
            var col = 0
            while col < width {
                let offset = row * bytesPerRow + col * bpp
                let red = UInt32(ptr[offset])
                let green = UInt32(ptr[offset + 1])
                let blue = UInt32(ptr[offset + 2])
                let key = (red << 16) | (green << 8) | blue
                seen.insert(key)
                if seen.count >= cap { return seen.count }
                col += stride
            }
            row += stride
        }
        return seen.count
    }

    /// Rough perceptual hash of an image: sum the RGB channels at a
    /// coarse grid of sample points. Two images with the same hash
    /// are extremely likely to be visually identical; a divergence
    /// indicates the metal view repainted.
    private func contentFingerprint(_ image: UIImage) -> UInt64 {
        guard let cg = image.cgImage,
            let provider = cg.dataProvider,
            let data = provider.data,
            let ptr = CFDataGetBytePtr(data)
        else { return 0 }
        let bpp = cg.bitsPerPixel / 8
        let bytesPerRow = cg.bytesPerRow
        let width = cg.width
        let height = cg.height
        let stride = max(4, min(width, height) / 24)
        var acc: UInt64 = 0
        var row = 0
        while row < height {
            var col = 0
            while col < width {
                let offset = row * bytesPerRow + col * bpp
                let red = UInt64(ptr[offset])
                let green = UInt64(ptr[offset + 1])
                let blue = UInt64(ptr[offset + 2])
                acc = acc &* 31 &+ (red << 16 | green << 8 | blue)
                col += stride
            }
            row += stride
        }
        return acc
    }

    // MARK: - Tests

    /// Production stack smoke: tapping the injected host with the
    /// `hello` script produces "Hello, world!\r\n" through the real
    /// `TerminalSession` → SSH → bridge pipeline and the Rust glyph
    /// renderer paints non-trivial pixels.
    @MainActor
    func test_stubSSH_helloRendersViaProductionPath() throws {
        let app = launch(stubScript: "hello")
        let metal = connectToStub(app)

        // Wait for the connecting overlay to clear + the feed pump to
        // push the scripted bytes into the Rust BtTerm + the next
        // Metal draw cycle to land. 2 s is generous on the simulator.
        Thread.sleep(forTimeInterval: 2.0)

        let shot = metal.screenshot().image
        let colors = distinctColors(in: shot)
        XCTAssertGreaterThanOrEqual(
            colors, 3,
            "Metal view should render glyphs (saw \(colors) distinct colours)"
        )
    }

    /// `ansiColors` script emits red/green/blue SGR runs. After the
    /// feed lands the metal view should show notably more distinct
    /// colours than the `hello` baseline.
    @MainActor
    func test_stubSSH_ansiColorsRendersMultipleHues() throws {
        let app = launch(stubScript: "ansiColors")
        let metal = connectToStub(app)
        Thread.sleep(forTimeInterval: 2.0)
        let shot = metal.screenshot().image
        let colors = distinctColors(in: shot)
        // Background + foreground + at least one SGR colour = 3+.
        // Cap is 32 so the upper bound is comfortable.
        XCTAssertGreaterThanOrEqual(
            colors, 4,
            "ANSI-colour fixture should paint multiple hues (saw \(colors))"
        )
    }

    /// Hardware-keyboard typing routes through the Rust metal view's
    /// `on_send` callback → `TerminalSession.send` → SSH client. With
    /// the `echo` script the mock loops written bytes back as a
    /// magenta SGR run, so the metal view's content fingerprint must
    /// change between pre-type and post-type.
    ///
    /// Note: the on-screen keybar (`keybar.tab` / `keybar.esc` / …)
    /// dispatches to an in-Rust `BtIosTerminalSession` that's
    /// intentionally **not** wired to the Swift `TerminalSession` here
    /// (see `IosTerminalView` "Deferrals" docstring), so we exercise
    /// the production text-input path via `metalView.typeText`
    /// instead — that's the path real users hit with a bluetooth
    /// keyboard / IME commit.
    @MainActor
    func test_hardwareTyping_reachesSessionAndRepaintsMetalView() throws {
        let app = launch(stubScript: "echo")
        let metal = connectToStub(app)
        Thread.sleep(forTimeInterval: 1.5)
        let pre = contentFingerprint(metal.screenshot().image)

        // Tapping the metal view should make it first responder so
        // `typeText` can reach `insertText:` on the `UIKeyInput`
        // conformance. Worth retrying because XCUI first-responder
        // dance is timing-sensitive on the simulator.
        metal.tap()
        Thread.sleep(forTimeInterval: 0.3)
        metal.typeText("xy")

        // Allow the round-trip: typeText → on_send → TerminalSession.send →
        // MockSSHClient.write → echo back via output stream → feed pump →
        // bt_ios_view_feed_bytes → next Metal draw.
        Thread.sleep(forTimeInterval: 1.5)
        let post = contentFingerprint(metal.screenshot().image)

        XCTAssertNotEqual(
            pre, post,
            "Metal view content must change after typed input echoes through SSH"
        )
    }
}
