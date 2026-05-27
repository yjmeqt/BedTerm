import Foundation

/// Persistent-flag bridge for the Rust-owned onboarding state machine.
///
/// After W23d the Rust `BtIosOnboardingFlowVC` owns the entire onboarding
/// flow (step transitions, state, final completion signal). Swift retains
/// only the `UserDefaults`-backed "has the user finished onboarding"
/// boolean, exposed here via two `@_cdecl` shims with no business logic.
///
/// The key string is kept byte-identical to the one
/// `OnboardingViewModel.swift` used so an in-place upgrade preserves the
/// per-device "skip onboarding next launch" flag.
public enum OnboardingPersistenceBridge {
    /// `UserDefaults` key the previous Swift `OnboardingViewModel` wrote.
    /// Held public so `RootCoordinator.hasCompletedOnboarding` can read
    /// it without going through the C ABI.
    public static let completedKey = "com.applovin.yi.bedterm.onboardingCompleted"

    /// Read the persisted flag. Same default (`false`) as the Swift VM.
    public static var hasCompleted: Bool {
        UserDefaults.standard.bool(forKey: completedKey)
    }

    /// Write the persisted flag.
    public static func setCompleted(_ value: Bool) {
        UserDefaults.standard.set(value, forKey: completedKey)
    }
}

// MARK: - C ABI

@_cdecl("bt_swift_onboarding_get_completed")
public func btSwiftOnboardingGetCompleted() -> Bool {
    OnboardingPersistenceBridge.hasCompleted
}

@_cdecl("bt_swift_onboarding_set_completed")
public func btSwiftOnboardingSetCompleted(_ value: Bool) {
    OnboardingPersistenceBridge.setCompleted(value)
}
