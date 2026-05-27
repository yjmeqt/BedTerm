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
        static let useRustHostsList = "settings.useRustHostsList"
        static let useRustConnectForm = "settings.useRustConnectForm"
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

    /// W24b experimental: route the saved-hosts root through the
    /// Rust-built `BtIosHostsListViewController` instead of the SwiftUI
    /// `HostsScreen`. Connect orchestration (toaster, mismatch dialog,
    /// terminal push) still runs through Swift's `HostsViewModel`; the
    /// Rust VC only renders the list and signals row taps / swipes back
    /// through the `bt_swift_hosts_*` bridge. Default: off until the
    /// W24c connect-form port lands.
    public var useRustHostsList: Bool {
        didSet {
            if useRustHostsList != oldValue {
                defaults.set(useRustHostsList, forKey: Key.useRustHostsList)
            }
        }
    }

    /// W24c experimental: route the saved-hosts connect form through
    /// the Rust-built `BtIosConnectFormViewController` instead of the
    /// SwiftUI `ConnectionFormScreen`. Validation + persistence still
    /// runs through Swift's `ConnectionFormViewModel.save` via the
    /// `bt_swift_connect_form_*` bridge. Default: off until the W24d
    /// flag-flip retirement lands.
    public var useRustConnectForm: Bool {
        didSet {
            if useRustConnectForm != oldValue {
                defaults.set(useRustConnectForm, forKey: Key.useRustConnectForm)
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
        self.useRustHostsList =
            defaults.object(forKey: Key.useRustHostsList) as? Bool ?? false
        self.useRustConnectForm =
            defaults.object(forKey: Key.useRustConnectForm) as? Bool ?? false
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
        for key in [
            "debug.useMetalRenderer",
            "experiments.useRustTerminal",
            "experiments.useRustSettings",
            "experiments.useRustOnboarding"
        ] {
            defaults.removeObject(forKey: key)
        }
    }
}
