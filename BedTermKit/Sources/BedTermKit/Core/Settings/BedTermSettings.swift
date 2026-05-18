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
        static let reserveTopSafeAreaInAltScreen = "settings.reserveTopSafeAreaInAltScreen"
        static let showCommandBlocks = "settings.showCommandBlocks"
    }

    private let defaults: UserDefaults

    /// When the remote app enters alt-screen mode (vim / htop / claude / fzf),
    /// reserve the top safe-area inset so the Dynamic Island, notch, or status
    /// bar no longer overlaps the TUI's first row. The bottom toolbar stays
    /// visible regardless — users still need Esc / Ctrl inside vim.
    /// Default: on.
    public var reserveTopSafeAreaInAltScreen: Bool {
        didSet {
            if reserveTopSafeAreaInAltScreen != oldValue {
                defaults.set(
                    reserveTopSafeAreaInAltScreen, forKey: Key.reserveTopSafeAreaInAltScreen)
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
        self.reserveTopSafeAreaInAltScreen =
            defaults.object(forKey: Key.reserveTopSafeAreaInAltScreen) as? Bool ?? true
        self.showCommandBlocks =
            defaults.object(forKey: Key.showCommandBlocks) as? Bool ?? false
    }
}
