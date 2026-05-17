import BedTermCoreC
import Foundation

/// Terminal mode flags exposed by the Rust core. Mirrors the `BT_MODE_*`
/// constants in `bedterm_core.h`; if you add a bit there, add it here too.
public struct BedTermMode: OptionSet, Sendable, Equatable {
    public let rawValue: UInt32

    public init(rawValue: UInt32) {
        self.rawValue = rawValue
    }

    /// Alternate screen is active. Set by vim/htop/claude/codex and other
    /// full-screen TUIs. Use this to hide overlays that assume a shell prompt.
    public static let altScreen = BedTermMode(rawValue: UInt32(BT_MODE_ALT_SCREEN))
    /// Remote app requested bracketed paste (`ESC[?2004h`).
    public static let bracketedPaste = BedTermMode(rawValue: UInt32(BT_MODE_BRACKETED_PASTE))
    /// Any mouse reporting mode is on (click / motion / drag).
    public static let mouseReport = BedTermMode(rawValue: UInt32(BT_MODE_MOUSE_REPORT))
    /// Application cursor keys mode (DECCKM); arrow keys send SS3 instead of CSI.
    public static let appCursor = BedTermMode(rawValue: UInt32(BT_MODE_APP_CURSOR))
    /// Application keypad mode.
    public static let appKeypad = BedTermMode(rawValue: UInt32(BT_MODE_APP_KEYPAD))
    /// Focus in/out reporting.
    public static let focusInOut = BedTermMode(rawValue: UInt32(BT_MODE_FOCUS_IN_OUT))
}
