import Foundation
import Observation

/// Drives the first-launch onboarding flow (R10). Captures the user's host kind
/// and network location, persists the answers to `UserDefaults`, and proactively
/// triggers the iOS local-network permission prompt for same-Wi-Fi targets.
@MainActor
@Observable
final class OnboardingViewModel {
    enum HostKind: String, Codable, Hashable {
        case macOS
        case other
    }

    enum Location: String, Codable, Hashable {
        case sameWifi
        case remote
    }

    /// Navigation steps after the host-kind picker. Used as `Hashable` path
    /// values in a `NavigationStack`.
    enum Step: Hashable {
        case location
        case macTutorial
        case localPermission
    }

    private let completedKey = "com.applovin.yi.bedterm.onboardingCompleted"
    private let hostKindKey = "com.applovin.yi.bedterm.onboardingHostKind"
    private let locationKey = "com.applovin.yi.bedterm.onboardingLocation"

    var hostKind: HostKind?
    var location: Location?
    var isRequestingPermission = false
    var permissionResult: LocalNetworkPrewarmer.Result?

    static var hasCompleted: Bool {
        UserDefaults.standard.bool(forKey: "com.applovin.yi.bedterm.onboardingCompleted")
    }

    func selectHostKind(_ kind: HostKind) {
        hostKind = kind
    }

    func selectLocation(_ loc: Location) {
        location = loc
    }

    /// The step to push after the user picks a location.
    var nextStepAfterLocation: Step {
        if hostKind == .macOS { return .macTutorial }
        if location == .sameWifi { return .localPermission }
        // Other host + remote → no tutorial, no permission step; finish directly.
        // The localPermission step is the unified terminator; it skips the
        // prompt itself when location == .remote and just shows a "Done" copy.
        return .localPermission
    }

    /// Trigger the iOS Local Network prompt. No-op for remote targets.
    func requestLocalNetworkIfNeeded() async {
        guard location == .sameWifi else {
            permissionResult = .granted
            return
        }
        isRequestingPermission = true
        permissionResult = await LocalNetworkPrewarmer.shared.requestPermission()
        isRequestingPermission = false
    }

    func finish() {
        let defaults = UserDefaults.standard
        defaults.set(true, forKey: completedKey)
        if let kind = hostKind { defaults.set(kind.rawValue, forKey: hostKindKey) }
        if let loc = location { defaults.set(loc.rawValue, forKey: locationKey) }
    }
}
