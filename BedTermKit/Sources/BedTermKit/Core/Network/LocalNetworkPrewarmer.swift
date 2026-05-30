import Foundation
import Network

/// Triggers the iOS "Local Network" permission prompt during onboarding,
/// so the first SSH connection to a LAN host does not fail with a
/// permission-shaped error before the user can tap Allow.
///
/// iOS has no public API to query the local-network permission state. The
/// trick: start an `NWBrowser` for a Bonjour service. Until the user responds
/// the browser stays in `.setup`; once permission is granted the browser
/// transitions to `.ready`; if denied it transitions to `.failed` /
/// `.waiting` with a policy-denied error. We watch state changes and resolve.
@MainActor
public final class LocalNetworkPrewarmer {
    public enum Result {
        case granted
        case denied
        case unknown
    }

    public static let shared = LocalNetworkPrewarmer()

    private let bonjourType = "_bedterm._tcp"
    private let timeout: Duration = .seconds(10)

    private init() {}

    public func requestPermission() async -> Result {
        let descriptor = NWBrowser.Descriptor.bonjour(type: bonjourType, domain: nil)
        let params = NWParameters()
        params.includePeerToPeer = true
        let browser = NWBrowser(for: descriptor, using: params)

        let outcome = await withCheckedContinuation { (continuation: CheckedContinuation<Result, Never>) in
            let resolved = ResolvedFlag()
            browser.stateUpdateHandler = { state in
                switch state {
                case .ready:
                    guard resolved.set() else { return }
                    continuation.resume(returning: .granted)
                case .failed(let error):
                    guard resolved.set() else { return }
                    continuation.resume(returning: Self.isPolicyDenied(error) ? .denied : .granted)
                case .waiting(let error):
                    // `.waiting` with policyDenied means the user tapped Don't Allow.
                    // Any other `.waiting` (e.g. no peers on the LAN, no network)
                    // means the browser is permitted to run — treat as granted.
                    guard Self.isPolicyDenied(error) else { return }
                    guard resolved.set() else { return }
                    continuation.resume(returning: .denied)
                case .cancelled:
                    guard resolved.set() else { return }
                    continuation.resume(returning: .unknown)
                case .setup:
                    break
                @unknown default:
                    break
                }
            }
            browser.start(queue: .main)

            Task { @MainActor in
                try? await Task.sleep(for: timeout)
                guard resolved.set() else { return }
                continuation.resume(returning: .unknown)
            }
        }
        browser.cancel()
        return outcome
    }

    /// True when the `NWError` indicates the user denied the Local Network
    /// permission. iOS reports this as a DNS error with code -65555
    /// (`kDNSServiceErr_PolicyDenied`) on both `.waiting` and `.failed`.
    private nonisolated static func isPolicyDenied(_ error: NWError) -> Bool {
        if case .dns(let code) = error, code == -65555 { return true }
        return false
    }
}

private final class ResolvedFlag: @unchecked Sendable {
    private var done = false
    private let lock = NSLock()
    func set() -> Bool {
        lock.lock(); defer { lock.unlock() }
        if done { return false }
        done = true
        return true
    }
    func reset() {
        lock.lock(); defer { lock.unlock() }
        done = false
    }
}
