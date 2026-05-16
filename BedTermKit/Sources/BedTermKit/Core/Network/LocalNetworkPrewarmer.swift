import Foundation
import Network

/// Triggers the iOS "Local Network" permission prompt before opening an SSH
/// connection to a LAN host, so the first connect does not fail with a
/// permission-shaped error before the user can tap Allow.
///
/// iOS has no public API to query the local-network permission state. The
/// trick: start an `NWBrowser` for a Bonjour service. Until the user responds
/// the browser stays in `.setup`; once permission is granted the browser
/// transitions to `.ready`; if denied it transitions to `.failed` /
/// `.waiting` with a policy-denied error. We watch state changes and resolve.
@MainActor
final class LocalNetworkPrewarmer {
    enum Result {
        case granted
        case denied
        case unknown
    }

    static let shared = LocalNetworkPrewarmer()

    private let grantedKey = "com.applovin.yi.bedterm.localNetworkGranted"
    private let bonjourType = "_bedterm._tcp"
    private let timeout: Duration = .seconds(10)

    private init() {}

    /// True when we have previously seen the browser become `.ready` —
    /// i.e. the user has at some point granted permission.
    var hasGrantedBefore: Bool {
        UserDefaults.standard.bool(forKey: grantedKey)
    }

    /// Returns `.notNeeded` when the host is loopback or not a LAN address;
    /// otherwise triggers the prompt and waits for the user's response.
    func ensurePermission(forHost host: String) async -> Result {
        guard Self.isLAN(host: host) else { return .granted }
        if hasGrantedBefore { return .granted }
        return await requestPermission()
    }

    // swiftlint:disable cyclomatic_complexity
    func requestPermission() async -> Result {
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

        if outcome == .granted {
            UserDefaults.standard.set(true, forKey: grantedKey)
        }
        return outcome
    }
    // swiftlint:enable cyclomatic_complexity

    /// True when the `NWError` indicates the user denied the Local Network
    /// permission. iOS reports this as a DNS error with code -65555
    /// (`kDNSServiceErr_PolicyDenied`) on both `.waiting` and `.failed`.
    private nonisolated static func isPolicyDenied(_ error: NWError) -> Bool {
        if case .dns(let code) = error, code == -65555 { return true }
        return false
    }

    // swiftlint:disable cyclomatic_complexity
    /// True when `host` is on the local network in the sense iOS gates with the
    /// permission prompt: RFC1918 IPv4, IPv6 unique-local / link-local, or an
    /// mDNS `.local` name. Loopback and public addresses return false.
    static func isLAN(host: String) -> Bool {
        let trimmed = host.trimmingCharacters(in: .whitespaces)
        guard !trimmed.isEmpty else { return false }
        if trimmed == "localhost" { return false }
        if trimmed.hasSuffix(".local") { return true }

        if let v4 = IPv4Address(trimmed) {
            let bytes = v4.rawValue
            guard bytes.count == 4 else { return false }
            let b0 = bytes[0], b1 = bytes[1]
            if b0 == 127 { return false }  // loopback
            if b0 == 10 { return true }  // 10.0.0.0/8
            if b0 == 192 && b1 == 168 { return true }  // 192.168.0.0/16
            if b0 == 172 && (16...31).contains(b1) { return true }  // 172.16.0.0/12
            if b0 == 169 && b1 == 254 { return true }  // link-local
            return false
        }

        if let v6 = IPv6Address(trimmed) {
            let bytes = v6.rawValue
            guard bytes.count == 16 else { return false }
            let b0 = bytes[0]
            if b0 == 0xFE && (bytes[1] & 0xC0) == 0x80 { return true }  // fe80::/10 link-local
            if (b0 & 0xFE) == 0xFC { return true }  // fc00::/7 unique-local
            return false
        }
        return false
    }
    // swiftlint:enable cyclomatic_complexity
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
