import XCTest
@testable import BedTermKit
import SwiftTerm

/// For each fixture: feed identical bytes to SwiftTerm and TerminalCore at the
/// same grid size, then compare the rendered character grid cell-by-cell.
///
/// What we compare: only the glyph (`ch`). Colours are NOT compared here — the
/// known `ansi_colors_lost_with_metal` bug means SwiftTerm's Metal path
/// produces flattened colours, while the Rust core produces correct ones.
/// Plan B's renderer-replacement will assert colours separately.
///
/// SwiftTerm API notes (1.2+):
/// - `Terminal.init(delegate:options:)` — `options` defaults to `TerminalOptions.default`
/// - `TerminalOptions` is a plain struct with `cols` and `rows` properties; init
///   has labeled args for each field with defaults, so we can pass just `cols`
///   and `rows`.
/// - `Terminal.feed(byteArray:[UInt8])` — accepts a plain array (not ArraySlice).
/// - `Terminal.getLine(row:)` returns `BufferLine?`; index it with `line[col]`
///   (returns `CharData`), then call `.getCharacter()` → `Character`.
/// - `TerminalDelegate` requires only `send(source:data:)` to be implemented;
///   all other methods have default no-op implementations in a public extension.
final class TerminalCoreShadowTests: XCTestCase {
    private static let cols = 80
    private static let rows = 24

    private func fixtureURLs() throws -> [URL] {
        let bundle = Bundle(for: Self.self)
        let dir = bundle.bundleURL.appendingPathComponent("Fixtures/byte_streams")
        return try FileManager.default.contentsOfDirectory(at: dir, includingPropertiesForKeys: nil)
            .filter { $0.pathExtension == "bin" }
            .sorted { $0.lastPathComponent < $1.lastPathComponent }
    }

    func testCharacterGridMatchesSwiftTerm() throws {
        let urls = try fixtureURLs()
        XCTAssertFalse(urls.isEmpty, "No fixture files found — check Fixtures folder reference in Xcode project")

        for url in urls {
            let bytes = try Data(contentsOf: url)
            let byteArray = [UInt8](bytes)

            // --- TerminalCore (Rust-backed) ---
            let core = TerminalCore(cols: Self.cols, rows: Self.rows)
            core.feed(bytes)
            let snap = core.snapshot()

            // --- SwiftTerm reference ---
            let options = TerminalOptions(cols: Self.cols, rows: Self.rows)
            let swiftTerm = SwiftTerm.Terminal(delegate: NoOpTerminalDelegate(), options: options)
            swiftTerm.feed(byteArray: byteArray)

            // --- Compare glyphs cell-by-cell ---
            for row in 0..<Self.rows {
                guard swiftTerm.getLine(row: row) != nil else { continue }
                for col in 0..<Self.cols {
                    // Use Terminal.getCharacter(col:row:) which routes through the
                    // private indexToCharMap for extended grapheme clusters (emoji,
                    // ZWJ sequences). CharData.getCharacter() falls back to " " for
                    // codes above maxRune — that path must NOT be used here.
                    guard let stChar = swiftTerm.getCharacter(col: col, row: row) else { continue }
                    let rsCell = snap.cell(col: col, row: row)
                    // Build the Character from the Rust cell's scalar; default to space
                    let rsScalar = rsCell.flatMap { Unicode.Scalar($0.ch) }.map { Character($0) } ?? " "
                    let stIsBlank = stChar == " " || stChar == "\0"
                    let rsIsBlank = rsScalar == " " || rsCell?.ch == 0
                    if stIsBlank && rsIsBlank { continue }
                    XCTAssertEqual(
                        stChar, rsScalar,
                        "[\(url.lastPathComponent)] mismatch at (\(col),\(row)): SwiftTerm=\(String(stChar).debugDescription) Core=\(String(rsScalar).debugDescription)"
                    )
                }
            }
        }
    }
}

private final class NoOpTerminalDelegate: TerminalDelegate {
    func send(source: SwiftTerm.Terminal, data: ArraySlice<UInt8>) {}
}
