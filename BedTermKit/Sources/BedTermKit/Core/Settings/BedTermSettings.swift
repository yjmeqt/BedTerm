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
        static let useRustTerminal = "experiments.useRustTerminal"
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

    /// Group command output into collapsible blocks (Warp-style). When on,
    /// the terminal session shows the Block list instead of the Classic
    /// Metal grid (except in alt-screen mode, which always falls back to
    /// Classic). Also gates the per-connect bootstrap push of the shell-
    /// integration snippet — there is no separate shell-integration toggle
    /// because the markers serve no purpose without Block view. Currently in
    /// beta — defaults to off until the feature stabilises. Default: off.
    public var showCommandBlocks: Bool {
        didSet {
            if showCommandBlocks != oldValue {
                defaults.set(showCommandBlocks, forKey: Key.showCommandBlocks)
            }
        }
    }

    /// **Experimental.** Route the terminal screen through the Rust-backed
    /// `BtIosTerminalViewController`. Default: off.
    public var useRustTerminal: Bool {
        didSet {
            if useRustTerminal != oldValue {
                defaults.set(useRustTerminal, forKey: Key.useRustTerminal)
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
        self.useRustTerminal =
            defaults.object(forKey: Key.useRustTerminal) as? Bool ?? false
        // Migrate the old shell-integration key (pre-2026-05-24, when it was
        // a separate toggle) into showCommandBlocks, then delete it.
        if let oldShell = defaults.object(forKey: "settings.installShellIntegrationOnConnect") as? Bool {
            // Preserve the user's explicit prior choice across the rename,
            // even though the new key now defaults to off (beta).
            if defaults.object(forKey: Key.showCommandBlocks) == nil {
                showCommandBlocks = oldShell
            }
            defaults.removeObject(forKey: "settings.installShellIntegrationOnConnect")
        }
        // Sweep obsolete debug-toggle keys from earlier builds so they don't
        // linger in users' Defaults. Add new entries to `obsoleteKeys` when
        // a setting is removed; never remove from this list.
        for key in ["debug.useMetalRenderer"] {
            defaults.removeObject(forKey: key)
        }
    }
}
