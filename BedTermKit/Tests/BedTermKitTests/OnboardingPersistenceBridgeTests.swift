import BedTermIOS
import Foundation
import Testing

@testable import BedTermKit

/// Round-trips the persisted "onboarding completed" flag through the
/// Rust-owned `bt_ios_settings_*` C ABI. The flag lives in the same
/// `NSUserDefaults` key the Swift `OnboardingPersistenceBridge` used
/// to write — guards against accidental key renames.
@Suite("OnboardingPersistenceBridge")
struct OnboardingPersistenceBridgeTests {
    private static let key = "com.applovin.yi.bedterm.onboardingCompleted"

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
            #expect(bt_ios_settings_onboarding_completed() == false)
        }
    }

    @Test("set persists under the documented key")
    func setPersistsValue() throws {
        try withCleanDefaults {
            bt_ios_settings_set_onboarding_completed(true)
            #expect(UserDefaults.standard.bool(forKey: Self.key) == true)
            #expect(bt_ios_settings_onboarding_completed() == true)
        }
    }

    @Test("round-trip true → false")
    func roundTripTrueFalse() throws {
        try withCleanDefaults {
            bt_ios_settings_set_onboarding_completed(true)
            #expect(bt_ios_settings_onboarding_completed() == true)
            bt_ios_settings_set_onboarding_completed(false)
            #expect(bt_ios_settings_onboarding_completed() == false)
        }
    }
}
