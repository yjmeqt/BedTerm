import Foundation
import Observation

/// Lightweight, app-wide settings store backed by `UserDefaults.standard`.
///
/// Reads on init; writes propagate immediately. Observable so SwiftUI views
/// re-render when a setting flips. Instantiated once in `BedTermApp` and
/// passed down via `@Environment(BedTermSettings.self)`.
@MainActor
@Observable
public final class BedTermSettings {
    private enum Key {
        static let autoHideComposerInAltScreen = "settings.autoHideComposerInAltScreen"
        static let showCommandBlocks = "settings.showCommandBlocks"
    }

    private let defaults: UserDefaults

    /// Hide the bottom composer + key bar when the remote app enters the
    /// alternate screen buffer (vim / claude / htop / fzf). When hidden, the
    /// terminal view fills the bottom area and key events flow straight to
    /// the PTY. Default: on.
    public var autoHideComposerInAltScreen: Bool {
        didSet {
            if autoHideComposerInAltScreen != oldValue {
                defaults.set(autoHideComposerInAltScreen, forKey: Key.autoHideComposerInAltScreen)
            }
        }
    }

    /// Group command output into collapsible blocks (Warp-style). Requires
    /// shell-integration injection and OSC 133 parsing — not implemented yet,
    /// so this toggle is exposed disabled in the UI as a forward signal.
    public var showCommandBlocks: Bool {
        didSet {
            if showCommandBlocks != oldValue {
                defaults.set(showCommandBlocks, forKey: Key.showCommandBlocks)
            }
        }
    }

    public init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        // `object(forKey:)` is `nil` for never-written keys; `bool(forKey:)`
        // collapses that to `false`. Use the object check to preserve the
        // documented default of `true` on first launch.
        self.autoHideComposerInAltScreen =
            defaults.object(forKey: Key.autoHideComposerInAltScreen) as? Bool ?? true
        self.showCommandBlocks =
            defaults.object(forKey: Key.showCommandBlocks) as? Bool ?? false
    }
}
