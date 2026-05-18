import Foundation
import Observation

/// State machine that turns the OSC 133 (FinalTerm) event stream into a
/// list of `Block`s.
///
/// The store doesn't capture any bytes itself. Block bodies are *row
/// ranges* into the shared terminal grid — running blocks re-snapshot
/// `[startLine, currentCursorLine + 1)` on every redraw, sealed blocks
/// freeze that range into an immutable `GridSnapshot` on `OSC 133 ; D`.
///
/// Lifecycle:
/// ```
///   PromptStart (133;A)  → open a block, record startLine = currentLine
///   CommandStart (133;B) → (informational; we already opened on A)
///   OutputStart (133;C)  → record command text from `cmd=<b64>` attr;
///                           startLine stays as the prompt row so the
///                           block visually includes the command echo
///   CommandEnd (133;D)   → endLine = currentLine + 1, freeze the snapshot,
///                           record exit code / dur / cwd from attrs
/// ```
///
/// Snapshot/lookup callbacks are injected so the store stays testable
/// without a live `TerminalCore`.
@MainActor
@Observable
public final class BlockStore {
    public typealias CurrentLineProvider = @MainActor () -> Int32
    public typealias RangeSnapshotProvider = @MainActor (Int32, Int32) -> GridSnapshot?

    /// Newest at the end. Stable IDs so `ForEach` can diff cleanly.
    public private(set) var blocks: [Block] = []

    @ObservationIgnored
    private var currentLineProvider: CurrentLineProvider?
    @ObservationIgnored
    private var rangeSnapshotProvider: RangeSnapshotProvider?

    private var openBlockIndex: Int?

    public init() {}

    /// Wire the store to a `TerminalCore`'s current-line + snapshot-range
    /// accessors. Must be called before any events are applied; otherwise
    /// blocks are recorded but contain no body (sealing fails silently).
    public func bind(
        currentLine: @escaping CurrentLineProvider,
        snapshotRange: @escaping RangeSnapshotProvider
    ) {
        self.currentLineProvider = currentLine
        self.rangeSnapshotProvider = snapshotRange
    }

    public func reset() {
        blocks.removeAll()
        openBlockIndex = nil
    }

    public func apply(_ event: Osc133Event) {
        switch event {
        case .promptStart(let attrs):
            // If a previous block was still running (no `D` arrived — user
            // Ctrl-C'd, or the integration shipped A without a closing D),
            // seal it with no exit code.
            sealOpenBlock(exitCode: nil, durMs: nil, cwd: nil)
            openNewBlock(cwd: decodeBase64(attrs["cwd"]))
        case .commandStart:
            break
        case .outputStart(let attrs):
            if openBlockIndex == nil {
                openNewBlock(cwd: nil)
            }
            if let idx = openBlockIndex, let cmd = decodeBase64(attrs["cmd"]) {
                blocks[idx].command = cmd
            }
        case .commandEnd(let exitCode, let attrs):
            let durMs = attrs["dur"].flatMap(UInt64.init)
            sealOpenBlock(
                exitCode: exitCode,
                durMs: durMs,
                cwd: decodeBase64(attrs["cwd"])
            )
        }
    }

    // MARK: - Internals

    private func openNewBlock(cwd: String?) {
        let line = currentLineProvider?() ?? 0
        var block = Block(startLine: line, workingDirectory: cwd)
        block.isRunning = true
        blocks.append(block)
        openBlockIndex = blocks.endIndex - 1
    }

    private func sealOpenBlock(exitCode: Int32?, durMs: UInt64?, cwd: String?) {
        guard let idx = openBlockIndex, idx < blocks.count else { return }
        guard blocks[idx].isRunning else { return }
        let endLine = (currentLineProvider?() ?? blocks[idx].startLine) + 1
        let snapshot = rangeSnapshotProvider?(blocks[idx].startLine, endLine)
        blocks[idx].isRunning = false
        blocks[idx].endLine = endLine
        blocks[idx].frozenSnapshot = snapshot
        blocks[idx].exitCode = exitCode
        if let durMs {
            blocks[idx].duration = TimeInterval(durMs) / 1000.0
        }
        if let cwd, blocks[idx].workingDirectory == nil {
            blocks[idx].workingDirectory = cwd
        }
        openBlockIndex = nil
    }

    private func decodeBase64(_ encoded: String?) -> String? {
        guard let encoded, !encoded.isEmpty,
            let data = Data(base64Encoded: encoded),
            let text = String(data: data, encoding: .utf8)
        else { return nil }
        return text
    }
}
