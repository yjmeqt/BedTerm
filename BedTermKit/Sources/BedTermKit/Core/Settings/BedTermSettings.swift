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
        static let installShellIntegrationOnConnect = "settings.installShellIntegrationOnConnect"
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

    /// Push the bundled OSC 133 shell-integration snippet into every new SSH
    /// session immediately after the channel opens. The snippet adds prompt
    /// + command markers (and a few extension attrs like `cmd`, `dur`, `cwd`)
    /// that future Block-style views consume. Off by default — opt-in,
    /// because writing bytes into the user's shell at connect time is a
    /// surprising side effect.
    public var installShellIntegrationOnConnect: Bool {
        didSet {
            if installShellIntegrationOnConnect != oldValue {
                defaults.set(
                    installShellIntegrationOnConnect,
                    forKey: Key.installShellIntegrationOnConnect)
            }
        }
    }

    /// Group command output into collapsible blocks (Warp-style). When on,
    /// the terminal session shows the Block list instead of the Classic
    /// Metal grid (except in alt-screen mode, which always falls back to
    /// Classic). Needs OSC 133 markers from the remote shell — pair with
    /// `installShellIntegrationOnConnect`, or run a host that already has
    /// iTerm2 / kitty / VSCode shell integration installed. Default: off.
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
        self.installShellIntegrationOnConnect =
            defaults.object(forKey: Key.installShellIntegrationOnConnect) as? Bool ?? false
        self.showCommandBlocks =
            defaults.object(forKey: Key.showCommandBlocks) as? Bool ?? true
        // Sweep obsolete debug-toggle keys from earlier builds so they don't
        // linger in users' Defaults. Add new entries to `obsoleteKeys` when
        // a setting is removed; never remove from this list.
        for key in ["debug.useMetalRenderer"] {
            defaults.removeObject(forKey: key)
        }
    }
}
