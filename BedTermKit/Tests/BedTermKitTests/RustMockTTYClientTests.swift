#if DEBUG
    import Foundation
    import Testing

    @testable import BedTermKit

    @Suite("RustMockTTYClient")
    struct RustMockTTYClientTests {
        @Test("rawSink program echoes every input byte back through the output stream")
        func rawSinkEchoesEveryByte() async throws {
            let client = RustMockTTYClient(program: .rawSink)
            let placeholder = HostCredential(
                host: "debug", port: 0, username: "debug", auth: .password(""))
            try await client.connect(
                .init(credential: placeholder, initialPTY: .init(cols: 80, rows: 24)))

            let payload: [UInt8] = [0x1B, 0x5B, 0x41, 0x03, 0x68, 0x69, 0x0D, 0x0A]

            let collector = Task<Data, Never> {
                var accum = Data()
                for await chunk in client.output {
                    accum.append(chunk)
                    if accum.count >= payload.count { break }
                }
                return accum
            }

            try await client.write(Data(payload))

            let received = await collector.value
            await client.disconnect()
            #expect(Array(received) == payload)
        }
    }
#endif
