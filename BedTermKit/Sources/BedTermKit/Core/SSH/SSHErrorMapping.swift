import Foundation

enum SSHErrorMapping {
    static func map(_ error: Error) -> SSHError {
        if let posix = error as? POSIXError {
            switch posix.code {
            case .ECONNREFUSED:
                return .tcpRefused
            case .ETIMEDOUT:
                return .timeout
            case .ECONNRESET, .EPIPE:
                return .peerReset
            default: break
            }
        }
        let desc = String(describing: error).lowercased()
        if desc.contains("domain name") || desc.contains("nodename") {
            return .dnsResolution
        }
        if desc.contains("authentication") {
            return .authenticationFailed
        }
        return .disconnected(String(describing: error))
    }
}
