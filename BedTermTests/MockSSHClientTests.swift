@testable import BedTerm
import Foundation
import Testing

@Suite("MockSSHClient")
struct MockSSHClientTests {
    @Test("scripted output is delivered to the output stream after connect")
    func scriptedOutput() async throws {
        let mock = MockSSHClient()
        mock.script(output: [Data("hello\n".utf8), Data("world\n".utf8)])
        try await mock.connect(.init(
            credential: HostCredential(host: "h", port: 22, username: "u", auth: .password("p")),
            initialPTY: .init(cols: 80, rows: 24)
        ))
        var iter = mock.output.makeAsyncIterator()
        #expect(await iter.next() == Data("hello\n".utf8))
        #expect(await iter.next() == Data("world\n".utf8))
        await mock.disconnect()
        #expect(await iter.next() == nil)
    }

    @Test("write records bytes sent")
    func writeRecords() async throws {
        let mock = MockSSHClient()
        try await mock.connect(.init(
            credential: HostCredential(host: "h", port: 22, username: "u", auth: .password("p")),
            initialPTY: .init(cols: 80, rows: 24)
        ))
        try await mock.write(Data([0x03]))
        try await mock.write(Data("ls\n".utf8))
        #expect(mock.written == [Data([0x03]), Data("ls\n".utf8)])
    }

    @Test("resize records the latest dimensions")
    func resizeRecords() async throws {
        let mock = MockSSHClient()
        try await mock.connect(.init(
            credential: HostCredential(host: "h", port: 22, username: "u", auth: .password("p")),
            initialPTY: .init(cols: 80, rows: 24)
        ))
        try await mock.resize(.init(cols: 120, rows: 40))
        #expect(mock.lastResize == .init(cols: 120, rows: 40))
    }

    @Test("connect rethrows a scripted error")
    func connectErrors() async {
        let mock = MockSSHClient()
        mock.scriptConnectError(.authenticationFailed)
        await #expect(throws: SSHError.authenticationFailed) {
            try await mock.connect(.init(
                credential: HostCredential(host: "h", port: 22, username: "u", auth: .password("p")),
                initialPTY: .init(cols: 80, rows: 24)
            ))
        }
    }
}
