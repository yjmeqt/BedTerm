import UIKit
import XCTest

/// UI smoke tests for the Rust-backed terminal VC (`BtIosTerminalViewController`).
///
/// Launches the app with `-uitest-rustTerminalDirect <fixture>` so the
/// `BedTermApp` bypasses Hosts + SSH and mounts `RustTerminalUITestHarness`
/// — a deterministic Rust VC pumped with a baked-in byte stream. Every
/// assertion below targets a11y identifiers registered by the Rust VC:
///
/// - `terminal.rust.root` — VC's root view
/// - `terminal.rust.metalView` — Metal canvas (`BtIosMetalInputView`)
/// - `terminal.rust.composer` — view2 / composer text view
/// - `terminal.rust.hud` + `terminal.rust.hud.input1` / `.input23`
/// - `keybar.run` / `keybar.composer` / `keybar.close`
/// - `keybar.tab` / `keybar.esc` / `keybar.ctrl` / `keybar.newline`
final class RustTerminalSmokeUITests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    // MARK: - Test helpers

    /// Generic descendant-by-identifier lookup. UIView containers (like the
    /// Rust VC root + metal view + HUD) are not natively accessibility
    /// elements, so `app.otherElements["…"]` may miss them. The `.any`
    /// descendant query walks everything and matches the identifier.
    @MainActor
    private func element(_ app: XCUIApplication, _ identifier: String) -> XCUIElement {
        app.descendants(matching: .any).matching(identifier: identifier).firstMatch
    }

    @MainActor
    private func launchWithFixture(_ fixture: String = "hello") -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments.append("-uitest-skipOnboarding")
        app.launchArguments.append("-uitest-rustTerminalDirect")
        app.launchArguments.append(fixture)
        app.launch()
        return app
    }

    /// Count distinct (R,G,B) triplets in `image` to a cap of `cap`. Used
    /// as a coarse "the view actually rendered something non-uniform"
    /// signal — we don't care about exact glyph fidelity, just that the
    /// pixel buffer isn't a single solid colour.
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
        // Sample every ~Nth pixel on a coarse grid so the scan is fast
        // even on a 3x device snapshot.
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

    // MARK: - Tests

    @MainActor
    func test_rustVC_mountsOnDirectLaunch() throws {
        let app = launchWithFixture()
        let root = element(app, "terminal.rust.root")
        XCTAssertTrue(
            root.waitForExistence(timeout: 8),
            "Rust VC root view should mount when launched with -uitest-rustTerminalDirect"
        )
        let metal = element(app, "terminal.rust.metalView")
        XCTAssertTrue(metal.waitForExistence(timeout: 4))
    }

    @MainActor
    func test_rustVC_rendersGlyphs() throws {
        let app = launchWithFixture("hello")
        let metal = element(app, "terminal.rust.metalView")
        XCTAssertTrue(metal.waitForExistence(timeout: 8))
        // Give the Rust feed pump + Metal draw cycle a moment to paint
        // the fixture bytes. 1 s is plenty at 60 fps.
        Thread.sleep(forTimeInterval: 1.0)
        let shot = metal.screenshot().image
        let colors = distinctColors(in: shot)
        // Five distinct colours covers: background, body text, plus a
        // few SGR-coloured glyphs from the fixture. If the metal view
        // is a solid clear-colour rectangle (regression: glyph renderer
        // didn't paint) we'd see ~1.
        XCTAssertGreaterThanOrEqual(
            colors, 5,
            "Metal view should render at least 5 distinct colours (saw \(colors))"
        )
    }

    @MainActor
    func test_keybar_chipsRespond() throws {
        let app = launchWithFixture()
        XCTAssertTrue(element(app, "terminal.rust.root").waitForExistence(timeout: 8))
        // The standard keybar (State2 default) exposes tab/esc/ctrl.
        let tab = app.buttons["keybar.tab"]
        let esc = app.buttons["keybar.esc"]
        let ctrl = app.buttons["keybar.ctrl"]
        XCTAssertTrue(tab.waitForExistence(timeout: 4))
        XCTAssertTrue(esc.exists)
        XCTAssertTrue(ctrl.exists)
        // Tapping each must not crash the app + the VC root must still
        // be alive after the round-trip.
        tab.tap()
        esc.tap()
        ctrl.tap()
        XCTAssertTrue(element(app, "terminal.rust.root").exists)
    }

    @MainActor
    func test_composer_opensFromComposerChip() throws {
        let app = launchWithFixture()
        XCTAssertTrue(element(app, "terminal.rust.root").waitForExistence(timeout: 8))
        // State2 (default) renders `keybar.composer` as the trailing chip.
        let composerChip = app.buttons["keybar.composer"]
        XCTAssertTrue(composerChip.waitForExistence(timeout: 4))
        composerChip.tap()
        // After flipping to State3, the close-composer chip + composer
        // text view become visible. The newline keybar variant also
        // swaps in.
        let close = app.buttons["keybar.close"]
        XCTAssertTrue(close.waitForExistence(timeout: 4))
        let newline = app.buttons["keybar.newline"]
        XCTAssertTrue(newline.exists)
        let composer = app.textViews["terminal.rust.composer"]
        XCTAssertTrue(composer.exists)
        // Tap close — the app must stay alive. We intentionally don't
        // assert the composer chip reappears immediately: in State3 the
        // software keyboard is up and XCUI's hit-testing of buttons
        // immediately after a mode flip is timing-sensitive on the sim.
        // The State3 → State2 flip itself is covered by the unit-tested
        // `ModeState::close_composer_tapped` (input_mode.rs).
        close.tap()
        XCTAssertTrue(
            element(app, "terminal.rust.root")
                .exists,
            "Rust VC must survive close-composer tap"
        )
    }

    @MainActor
    func test_hudSwitcher_flipsMode() throws {
        let app = launchWithFixture()
        XCTAssertTrue(element(app, "terminal.rust.root").waitForExistence(timeout: 8))
        let hud = element(app, "terminal.rust.hud")
        XCTAssertTrue(hud.waitForExistence(timeout: 4))
        let input1 = app.buttons["terminal.rust.hud.input1"]
        let input23 = app.buttons["terminal.rust.hud.input23"]
        XCTAssertTrue(input1.exists)
        XCTAssertTrue(input23.exists)
        // Flip to State1 — the Send/Run chip becomes the trailing action.
        input1.tap()
        let runChip = app.buttons["keybar.run"]
        XCTAssertTrue(runChip.waitForExistence(timeout: 4))
        // Flip back to State2 — composer chip returns.
        input23.tap()
        let composerChip = app.buttons["keybar.composer"]
        XCTAssertTrue(composerChip.waitForExistence(timeout: 4))
    }

    @MainActor
    func test_metalView_isHittable() throws {
        let app = launchWithFixture()
        let metal = element(app, "terminal.rust.metalView")
        XCTAssertTrue(metal.waitForExistence(timeout: 8))
        XCTAssertTrue(metal.isHittable, "metal view should be hit-testable for tap routing")
        // Tapping view1 in State2 should not crash; the coordinator's
        // tap gesture-recognizer handles the tap and tries to make
        // view1 first responder.
        metal.tap()
        XCTAssertTrue(element(app, "terminal.rust.root").exists)
    }
}
