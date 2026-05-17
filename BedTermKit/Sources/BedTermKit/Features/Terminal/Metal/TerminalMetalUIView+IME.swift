import UIKit

// MARK: - UITextInput conformance for IME preedit (marked text)
//
// `TerminalMetalUIView` is not a text editor — the terminal grid is owned by the
// Rust core and the document model the user actually edits is the empty string.
// However, iOS IMEs (Pinyin, Japanese Romaji, Korean 2-set) require the input
// surface to conform to `UITextInput` so they can display marked / preedit text
// while the user composes. Without it, the user types "nihao" and sees nothing
// until they confirm a candidate.
//
// The "document" we expose here is just the current marked-text string. All
// positions / ranges are integer UTF-16 offsets into that string. On commit
// (`insertText`), the marked text is cleared and the committed bytes flow into
// the existing PTY-send path unchanged.

/// Integer-offset position into the marked-text string.
final class TerminalMetalTextPosition: UITextPosition {
    let offset: Int
    init(offset: Int) { self.offset = offset }
}

/// Integer-offset range into the marked-text string.
final class TerminalMetalTextRange: UITextRange {
    let startOffset: Int
    let endOffset: Int
    init(start: Int, end: Int) {
        self.startOffset = start
        self.endOffset = end
    }
    override var start: UITextPosition { TerminalMetalTextPosition(offset: startOffset) }
    override var end: UITextPosition { TerminalMetalTextPosition(offset: endOffset) }
    override var isEmpty: Bool { startOffset == endOffset }
}

/// Storage for the IME preedit. Held on the view via an associated object so the
/// extension can stay self-contained.
private final class IMEState {
    var markedText = ""
    var selectedRange = NSRange(location: 0, length: 0)
    weak var inputDelegate: UITextInputDelegate?
    var overlay: IMEPreeditOverlay?
}

nonisolated(unsafe) private var imeStateKey: UInt8 = 0

extension TerminalMetalUIView {
    private var imeState: IMEState {
        if let existing = objc_getAssociatedObject(self, &imeStateKey) as? IMEState {
            return existing
        }
        let fresh = IMEState()
        objc_setAssociatedObject(self, &imeStateKey, fresh, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
        return fresh
    }
}

// UIKeyInput + UITextInputTraits — inherited via UITextInput. Kept in this
// file so `TerminalMetalUIView.swift` stays focused on the renderer plumbing.
extension TerminalMetalUIView {
    var hasText: Bool { false }

    func insertText(_ text: String) {
        // Clear any IME preedit before forwarding the committed bytes.
        clearMarkedTextOnCommit()
        if text == "\n" {
            onSend(Data([0x0D]))
        } else {
            onSend(Data(text.utf8))
        }
    }

    func deleteBackward() {
        onSend(Data([0x7F]))  // DEL — xterm-256color expects 0x7F.
    }

    // UITextInputTraits — terminal-friendly defaults. Required for the
    // soft keyboard, and to keep IME corrections from mangling commands.
    var autocorrectionType: UITextAutocorrectionType {
        get { .no }
        set { _ = newValue }
    }
    var autocapitalizationType: UITextAutocapitalizationType {
        get { .none }
        set { _ = newValue }
    }
    var spellCheckingType: UITextSpellCheckingType {
        get { .no }
        set { _ = newValue }
    }
    var smartQuotesType: UITextSmartQuotesType {
        get { .no }
        set { _ = newValue }
    }
    var smartDashesType: UITextSmartDashesType {
        get { .no }
        set { _ = newValue }
    }
    var smartInsertDeleteType: UITextSmartInsertDeleteType {
        get { .no }
        set { _ = newValue }
    }
    // `.default`, not `.asciiCapable` — `.asciiCapable` hides every non-Latin
    // keyboard, blocking the globe key from cycling to CJK IMEs.
    var keyboardType: UIKeyboardType {
        get { .default }
        set { _ = newValue }
    }
    var keyboardAppearance: UIKeyboardAppearance {
        get { .dark }
        set { _ = newValue }
    }
    var returnKeyType: UIReturnKeyType {
        get { .default }
        set { _ = newValue }
    }
    var enablesReturnKeyAutomatically: Bool {
        get { false }
        set { _ = newValue }
    }
}

extension TerminalMetalUIView: UITextInput {

    // MARK: Marked text

    var markedTextRange: UITextRange? {
        let state = imeState
        guard !state.markedText.isEmpty else { return nil }
        return TerminalMetalTextRange(start: 0, end: state.markedText.utf16.count)
    }

    var markedTextStyle: [NSAttributedString.Key: Any]? {
        get { nil }
        set { _ = newValue }
    }

    func setMarkedText(_ markedText: String?, selectedRange: NSRange) {
        let state = imeState
        state.markedText = markedText ?? ""
        state.selectedRange = selectedRange
        updatePreeditOverlay()
    }

    func unmarkText() {
        let state = imeState
        let pending = state.markedText
        state.markedText = ""
        state.selectedRange = NSRange(location: 0, length: 0)
        updatePreeditOverlay()
        if !pending.isEmpty {
            onSend(Data(pending.utf8))
        }
    }

    // MARK: Document model — backed by the marked-text string only.

    var selectedTextRange: UITextRange? {
        get {
            let len = imeState.markedText.utf16.count
            return TerminalMetalTextRange(start: len, end: len)
        }
        set { _ = newValue }
    }

    var beginningOfDocument: UITextPosition {
        TerminalMetalTextPosition(offset: 0)
    }

    var endOfDocument: UITextPosition {
        TerminalMetalTextPosition(offset: imeState.markedText.utf16.count)
    }

    var inputDelegate: UITextInputDelegate? {
        get { imeState.inputDelegate }
        set { imeState.inputDelegate = newValue }
    }

    var tokenizer: UITextInputTokenizer {
        UITextInputStringTokenizer(textInput: self)
    }

    func text(in range: UITextRange) -> String? {
        guard let textRange = range as? TerminalMetalTextRange else { return nil }
        let source = imeState.markedText
        let utf16 = source.utf16
        let count = utf16.count
        let lo = max(0, min(textRange.startOffset, count))
        let hi = max(lo, min(textRange.endOffset, count))
        guard let startIdx = utf16.index(utf16.startIndex, offsetBy: lo, limitedBy: utf16.endIndex),
            let endIdx = utf16.index(utf16.startIndex, offsetBy: hi, limitedBy: utf16.endIndex)
        else { return "" }
        return String(utf16[startIdx..<endIdx]) ?? ""
    }

    func replace(_ range: UITextRange, withText text: String) {
        // The terminal grid is owned by the Rust core; replacements into the
        // grid are not honoured. Marked-text edits go via setMarkedText.
        _ = range
        _ = text
    }

    func position(from position: UITextPosition, offset: Int) -> UITextPosition? {
        guard let pos = position as? TerminalMetalTextPosition else { return nil }
        let target = pos.offset + offset
        let count = imeState.markedText.utf16.count
        guard target >= 0, target <= count else { return nil }
        return TerminalMetalTextPosition(offset: target)
    }

    func position(
        from position: UITextPosition,
        in direction: UITextLayoutDirection,
        offset: Int
    ) -> UITextPosition? {
        guard let pos = position as? TerminalMetalTextPosition else { return nil }
        let delta: Int
        switch direction {
        case .right, .down: delta = offset
        case .left, .up: delta = -offset
        @unknown default: delta = offset
        }
        let target = pos.offset + delta
        let count = imeState.markedText.utf16.count
        guard target >= 0, target <= count else { return nil }
        return TerminalMetalTextPosition(offset: target)
    }

    func compare(_ position: UITextPosition, to other: UITextPosition) -> ComparisonResult {
        let lhs = (position as? TerminalMetalTextPosition)?.offset ?? 0
        let rhs = (other as? TerminalMetalTextPosition)?.offset ?? 0
        if lhs < rhs { return .orderedAscending }
        if lhs > rhs { return .orderedDescending }
        return .orderedSame
    }

    func offset(from: UITextPosition, to toPosition: UITextPosition) -> Int {
        let lhs = (from as? TerminalMetalTextPosition)?.offset ?? 0
        let rhs = (toPosition as? TerminalMetalTextPosition)?.offset ?? 0
        return rhs - lhs
    }

    func textRange(from fromPosition: UITextPosition, to toPosition: UITextPosition) -> UITextRange? {
        let lhs = (fromPosition as? TerminalMetalTextPosition)?.offset ?? 0
        let rhs = (toPosition as? TerminalMetalTextPosition)?.offset ?? 0
        return TerminalMetalTextRange(start: min(lhs, rhs), end: max(lhs, rhs))
    }

    func position(
        within range: UITextRange,
        farthestIn direction: UITextLayoutDirection
    ) -> UITextPosition? {
        guard let textRange = range as? TerminalMetalTextRange else { return nil }
        switch direction {
        case .left, .up: return TerminalMetalTextPosition(offset: textRange.startOffset)
        case .right, .down: return TerminalMetalTextPosition(offset: textRange.endOffset)
        @unknown default: return TerminalMetalTextPosition(offset: textRange.endOffset)
        }
    }

    func characterRange(
        byExtending position: UITextPosition,
        in direction: UITextLayoutDirection
    ) -> UITextRange? {
        guard let pos = position as? TerminalMetalTextPosition else { return nil }
        let count = imeState.markedText.utf16.count
        switch direction {
        case .left, .up:
            return TerminalMetalTextRange(start: 0, end: pos.offset)
        case .right, .down:
            return TerminalMetalTextRange(start: pos.offset, end: count)
        @unknown default:
            return TerminalMetalTextRange(start: pos.offset, end: count)
        }
    }

    func baseWritingDirection(
        for position: UITextPosition,
        in direction: UITextStorageDirection
    ) -> NSWritingDirection {
        _ = position
        _ = direction
        return .leftToRight
    }

    func setBaseWritingDirection(
        _ writingDirection: NSWritingDirection,
        for range: UITextRange
    ) {
        _ = writingDirection
        _ = range
    }

    // MARK: Geometry — used by the IME to anchor the candidate bar.

    func firstRect(for range: UITextRange) -> CGRect {
        _ = range
        return cursorRectInViewCoordinates()
    }

    func caretRect(for position: UITextPosition) -> CGRect {
        _ = position
        return cursorRectInViewCoordinates()
    }

    func selectionRects(for range: UITextRange) -> [UITextSelectionRect] {
        _ = range
        return []
    }

    func closestPosition(to point: CGPoint) -> UITextPosition? {
        _ = point
        return TerminalMetalTextPosition(offset: imeState.markedText.utf16.count)
    }

    func closestPosition(to point: CGPoint, within range: UITextRange) -> UITextPosition? {
        _ = range
        return closestPosition(to: point)
    }

    func characterRange(at point: CGPoint) -> UITextRange? {
        _ = point
        let count = imeState.markedText.utf16.count
        return TerminalMetalTextRange(start: count, end: count)
    }

    // MARK: - Overlay positioning + commit hook

    /// Cursor rect in this view's coordinate space, derived from the Rust
    /// core's grid snapshot. Matches the `MetalCursorLayer` placement.
    func cursorRectInViewCoordinates() -> CGRect {
        let snapshot = terminalCore.snapshot()
        let col = Int(snapshot.cursorCol)
        let row = Int(snapshot.cursorRow)
        return CGRect(
            x: CGFloat(col) * cellSize.width,
            y: CGFloat(row) * cellSize.height,
            width: cellSize.width,
            height: cellSize.height
        )
    }

    /// Show / hide / refresh the floating preedit overlay near the cursor.
    func updatePreeditOverlay() {
        let state = imeState
        if state.markedText.isEmpty {
            state.overlay?.removeFromSuperview()
            state.overlay = nil
            return
        }
        let overlay = state.overlay ?? IMEPreeditOverlay()
        if state.overlay == nil {
            addSubview(overlay)
            state.overlay = overlay
        }
        overlay.setText(state.markedText, font: preeditFont())
        let cursorRect = cursorRectInViewCoordinates()
        let size = overlay.sizeThatFits(bounds.size)
        let yBelow = cursorRect.maxY + 2
        // If the overlay would clip the bottom of the view, draw above instead.
        let originY: CGFloat
        if yBelow + size.height <= bounds.height {
            originY = yBelow
        } else {
            originY = max(0, cursorRect.minY - size.height - 2)
        }
        let originX = min(max(0, cursorRect.minX), max(0, bounds.width - size.width))
        overlay.frame = CGRect(origin: CGPoint(x: originX, y: originY), size: size)
    }

    /// Clear any marked text — called from `insertText` once the IME commits.
    func clearMarkedTextOnCommit() {
        let state = imeState
        guard !state.markedText.isEmpty else { return }
        state.markedText = ""
        state.selectedRange = NSRange(location: 0, length: 0)
        state.overlay?.removeFromSuperview()
        state.overlay = nil
    }

    private func preeditFont() -> UIFont {
        UIFontMetrics.default.scaledFont(
            for: .monospacedSystemFont(ofSize: 14, weight: .regular)
        )
    }
}
