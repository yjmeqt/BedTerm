import Foundation
import Testing

@testable import BedTermKit

@Suite("SSH error mapping")
struct SSHErrorMappingTests {
    @Test("POSIX ECONNREFUSED maps to .tcpRefused")
    func tcpRefused() {
        let err = POSIXError(.ECONNREFUSED)
        #expect(SSHErrorMapping.map(err) == .tcpRefused)
    }

    @Test("POSIX ETIMEDOUT maps to .timeout")
    func timedOut() {
        let err = POSIXError(.ETIMEDOUT)
        #expect(SSHErrorMapping.map(err) == .timeout)
    }

    @Test("DNS resolution failure maps to .dnsResolution")
    func dns() {
        struct FakeDNS: Error, CustomStringConvertible {
            var description: String { "Domain name not found" }
        }
        #expect(SSHErrorMapping.map(FakeDNS()) == .dnsResolution)
    }

    @Test("unknown error falls back to .disconnected(description)")
    func fallback() {
        struct Boom: Error, CustomStringConvertible { var description: String { "boom" } }
        #expect(SSHErrorMapping.map(Boom()) == .disconnected("boom"))
    }
}
