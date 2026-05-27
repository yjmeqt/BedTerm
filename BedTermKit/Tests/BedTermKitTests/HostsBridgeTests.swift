import BedTermCoreC
import Foundation
import Testing

@testable import BedTermKit

/// Exercises the W24b `@_cdecl` shims that the Rust hosts list VC reaches
/// for via `bt_swift_hosts_*`. The bridge has been slimmed down: only
/// `bt_swift_hosts_connect` remains (snapshot + delete now go through
/// `crate::hosts_store` directly from the Rust VC).
@Suite("HostsBridge")
@MainActor
struct HostsBridgeTests {
    @Test("connect handler receives the parsed UUID")
    func connectHandlerReceivesUUID() throws {
        let prior = HostsBridge.connectHandler
        defer { HostsBridge.connectHandler = prior }

        var received: UUID?
        HostsBridge.connectHandler = { id in received = id }

        let uuid = UUID()
        uuid.uuidString.withCString { ptr in
            btSwiftHostsConnect(ptr)
        }
        #expect(received == uuid)
    }

    @Test("connect handler ignores invalid UUIDs")
    func connectHandlerIgnoresInvalidUUID() throws {
        let prior = HostsBridge.connectHandler
        defer { HostsBridge.connectHandler = prior }

        var fired = false
        HostsBridge.connectHandler = { _ in fired = true }
        "not-a-uuid".withCString { ptr in
            btSwiftHostsConnect(ptr)
        }
        #expect(!fired)
    }

    @Test("C symbol is reachable via dlsym")
    func cdeclSymbolsReachable() throws {
        let handle = UnsafeMutableRawPointer(bitPattern: -2)
        #expect(dlsym(handle, "bt_swift_hosts_connect") != nil)
    }
}
