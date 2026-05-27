import Foundation

/// Bridge between the Rust-built `BtIosSettingsViewController` and the
/// Swift-owned `BedTermSettings` store.
///
/// The Rust VC does not link Swift modules; it reaches into the settings
/// store through the C ABI exported by the `@_cdecl` functions below. The
/// app sets `observableHandle` at launch (after constructing the shared
/// `BedTermSettings` instance and before any Rust VC is created); each
/// `@_cdecl` accessor reads from / writes to that handle.
///
/// All accessors are main-actor — UIKit's contract enforces main-thread
/// access from selector handlers, and the `BedTermSettings` store is
/// itself `@MainActor`.
@MainActor
public enum SettingsBridge {
    /// Strong handle to the shared settings store. Installed once by the
    /// app delegate / root coordinator at startup. Reads return defaults
    /// (matching the documented `BedTermSettings` first-launch values)
    /// when the handle is unset — this lets Rust unit / harness tests
    /// load the VC without a backing store.
    public static var observableHandle: BedTermSettings?
}

// MARK: - C ABI accessors
//
// The function names match the symbol patterns the Rust VC's
// `extern "C"` declarations reach for via `bt_swift_settings_*`. Keep
// the two sides in sync — adding a new property here requires adding
// the matching extern in `rust-core/bedterm_ios/src/settings_vc.rs`.

@_cdecl("bt_swift_settings_get_reserve_top_safe_area")
public func btSwiftSettingsGetReserveTopSafeArea() -> Bool {
    MainActor.assumeIsolated {
        SettingsBridge.observableHandle?.reserveTopSafeAreaInAltScreen ?? true
    }
}

@_cdecl("bt_swift_settings_set_reserve_top_safe_area")
public func btSwiftSettingsSetReserveTopSafeArea(_ value: Bool) {
    MainActor.assumeIsolated {
        SettingsBridge.observableHandle?.reserveTopSafeAreaInAltScreen = value
    }
}

@_cdecl("bt_swift_settings_get_show_command_blocks")
public func btSwiftSettingsGetShowCommandBlocks() -> Bool {
    MainActor.assumeIsolated {
        SettingsBridge.observableHandle?.showCommandBlocks ?? false
    }
}

@_cdecl("bt_swift_settings_set_show_command_blocks")
public func btSwiftSettingsSetShowCommandBlocks(_ value: Bool) {
    MainActor.assumeIsolated {
        SettingsBridge.observableHandle?.showCommandBlocks = value
    }
}

@_cdecl("bt_swift_settings_get_use_rust_hosts_list")
public func btSwiftSettingsGetUseRustHostsList() -> Bool {
    MainActor.assumeIsolated {
        SettingsBridge.observableHandle?.useRustHostsList ?? false
    }
}

@_cdecl("bt_swift_settings_set_use_rust_hosts_list")
public func btSwiftSettingsSetUseRustHostsList(_ value: Bool) {
    MainActor.assumeIsolated {
        SettingsBridge.observableHandle?.useRustHostsList = value
    }
}

@_cdecl("bt_swift_settings_get_use_rust_connect_form")
public func btSwiftSettingsGetUseRustConnectForm() -> Bool {
    MainActor.assumeIsolated {
        SettingsBridge.observableHandle?.useRustConnectForm ?? false
    }
}

@_cdecl("bt_swift_settings_set_use_rust_connect_form")
public func btSwiftSettingsSetUseRustConnectForm(_ value: Bool) {
    MainActor.assumeIsolated {
        SettingsBridge.observableHandle?.useRustConnectForm = value
    }
}
