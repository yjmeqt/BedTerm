import Foundation
import NIOCore

enum SSHErrorMapping {
    static func map(_ error: Error) -> SSHError {
        if let io = error as? IOError {
            switch io.errnoCode {
            case ECONNREFUSED: return .tcpRefused
            case ETIMEDOUT: return .timeout
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
