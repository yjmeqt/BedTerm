import Foundation
import Testing

@testable import BedTermKit

/// Exercises the W23d `@_cdecl` shims (`bt_swift_onboarding_get_completed`
/// / `bt_swift_onboarding_set_completed`) round-trip through the same
/// `UserDefaults` key the Swift `OnboardingViewModel` used to read.
///
/// The bridge has no business logic — these tests guard against
/// accidentally renaming the key (which would silently re-run onboarding
/// for upgraded users) and confirm the C symbols resolve.
@Suite("OnboardingPersistenceBridge")
struct OnboardingPersistenceBridgeTests {
    private static let key = "com.applovin.yi.bedterm.onboardingCompleted"

    /// Reset the persisted value before and after each test so the suite
    /// is order-independent.
    private func withCleanDefaults(_ body: () throws -> Void) rethrows {
        let defaults = UserDefaults.standard
        let prior = defaults.object(forKey: Self.key)
        defaults.removeObject(forKey: Self.key)
        defer {
            if let prior {
                defaults.set(prior, forKey: Self.key)
            } else {
                defaults.removeObject(forKey: Self.key)
            }
        }
        try body()
    }

    @Test("get returns false when key is absent")
    func getDefaultsToFalse() throws {
        try withCleanDefaults {
            // Bridge enum view — same code path the @_cdecl uses.
            #expect(OnboardingPersistenceBridge.hasCompleted == false)
        }
    }

    @Test("set persists the value under the documented key")
    func setPersistsValue() throws {
        try withCleanDefaults {
            OnboardingPersistenceBridge.setCompleted(true)
            #expect(UserDefaults.standard.bool(forKey: Self.key) == true)
            #expect(OnboardingPersistenceBridge.hasCompleted == true)
        }
    }

    @Test("round-trip true → false")
    func roundTripTrueFalse() throws {
        try withCleanDefaults {
            OnboardingPersistenceBridge.setCompleted(true)
            #expect(OnboardingPersistenceBridge.hasCompleted == true)
            OnboardingPersistenceBridge.setCompleted(false)
            #expect(OnboardingPersistenceBridge.hasCompleted == false)
        }
    }

    @Test("C symbols are reachable via dlsym")
    func cdeclSymbolsReachable() throws {
        // `dlsym(RTLD_DEFAULT, …)` finds @_cdecl exports linked into the
        // test bundle. Confirms the names match what the Rust extern "C"
        // block in coordinator.rs reaches for.
        #expect(dlsym(UnsafeMutableRawPointer(bitPattern: -2), "bt_swift_onboarding_get_completed") != nil)
        #expect(dlsym(UnsafeMutableRawPointer(bitPattern: -2), "bt_swift_onboarding_set_completed") != nil)
        #expect(dlsym(UnsafeMutableRawPointer(bitPattern: -2), "bt_swift_request_local_network") != nil)
    }
}
