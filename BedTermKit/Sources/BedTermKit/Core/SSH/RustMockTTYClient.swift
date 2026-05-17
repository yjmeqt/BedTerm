//
// RustMockTTYClient.swift
//
// Debug-only `SSHClient` backed by the Rust in-process mock TTY
// (`bt_mock_tty_*` symbols, gated behind the `mock-tty` Cargo feature which is
// enabled only for Debug xcframework builds — see
// `scripts/build-rust-xcframework.sh`).
//
// Release builds strip both the FFI symbols and this file (`#if DEBUG`).
//

#if DEBUG
    import BedTermCoreC
    import Foundation
    import os.lock

    /// C function pointer signature of the mock TTY output callback. Mirrors
    /// `bt_mock_tty_set_output_callback`'s second parameter in the generated
    /// header (a nullable function pointer; `uintptr_t` length maps to `UInt`).
    private typealias OutputCallbackC =
        @convention(c) (
            UnsafePointer<UInt8>?, UInt, UnsafeMutableRawPointer?
        ) -> Void

    /// Which debug TTY program the Rust mock should run. Mirrors the `u32`
    /// dispatch values understood by `bt_mock_tty_create`.
    public enum DebugTTYProgram: UInt32, Sendable, CaseIterable {
        case echoShell = 0
        case vimLite = 1
        case replay = 2
        case rawSink = 3
    }

    /// `SSHClient` implementation that talks to the Rust mock TTY via FFI.
    ///
    /// Concurrency: `@unchecked Sendable` because FFI calls are serialised by an
    /// `NSLock`, matching the project's convention (see `CitadelSSHClient`).
    public final class RustMockTTYClient: SSHClient, @unchecked Sendable {
        private let program: DebugTTYProgram
        private let opts: String?
        // The handle is an opaque C pointer (non-Sendable). We store it as a UInt
        // bit pattern inside an `OSAllocatedUnfairLock` (Sendable-safe) and
        // reconstruct the OpaquePointer at each FFI call site. Zero means "no
        // handle". The lock + `@unchecked Sendable` conformance on the enclosing
        // class is the project's convention for FFI handles.
        private let handleBits = OSAllocatedUnfairLock<UInt>(initialState: 0)

        private func loadHandle() -> OpaquePointer? {
            let bits = handleBits.withLock { $0 }
            return bits == 0 ? nil : OpaquePointer(bitPattern: Int(bitPattern: bits))
        }

        private func storeHandle(_ ptr: OpaquePointer?) {
            let bits: UInt
            if let ptr {
                bits = UInt(bitPattern: Int(bitPattern: UnsafeRawPointer(ptr)))
            } else {
                bits = 0
            }
            handleBits.withLock { $0 = bits }
        }

        private func takeHandle() -> OpaquePointer? {
            let bits = handleBits.withLock { current -> UInt in
                let old = current
                current = 0
                return old
            }
            return bits == 0 ? nil : OpaquePointer(bitPattern: Int(bitPattern: bits))
        }
        private let outputStream: AsyncStream<Data>
        private let outputContinuation: AsyncStream<Data>.Continuation
        private var replayTimer: Timer?
        private let replayStart: Date = .init()

        public var output: AsyncStream<Data> { outputStream }

        public init(program: DebugTTYProgram, opts: String? = nil) {
            self.program = program
            self.opts = opts
            var cont: AsyncStream<Data>.Continuation!
            self.outputStream = AsyncStream<Data> { cont = $0 }
            self.outputContinuation = cont
        }

        public func connect(_ request: SSHConnectionRequest) async throws {
            _ = request  // credential ignored by the mock
            let created: OpaquePointer? = opts.withCStringOrNil { ptr in
                bt_mock_tty_create(program.rawValue, ptr)
            }
            guard let created else {
                throw SSHError.handshakeFailed("mock-tty create failed")
            }
            storeHandle(created)

            let cb: OutputCallbackC = { bytes, len, userData in
                guard let bytes, len > 0, let userData else { return }
                let client = Unmanaged<RustMockTTYClient>.fromOpaque(userData).takeUnretainedValue()
                let data = Data(bytes: bytes, count: Int(len))
                client.outputContinuation.yield(data)
            }
            let userData = UnsafeMutableRawPointer(Unmanaged.passUnretained(self).toOpaque())
            bt_mock_tty_set_output_callback(created, cb, userData)

            if program == .replay {
                await MainActor.run {
                    let interval = 1.0 / 60.0
                    self.replayTimer = Timer.scheduledTimer(
                        withTimeInterval: interval, repeats: true
                    ) { [weak self] _ in
                        guard let self else { return }
                        guard let handle = self.loadHandle() else { return }
                        let elapsed = UInt64(Date().timeIntervalSince(self.replayStart) * 1000)
                        bt_mock_tty_tick(handle, elapsed)
                    }
                }
            }
        }

        public func write(_ data: Data) async throws {
            guard let handle = loadHandle() else { return }
            _ = data.withUnsafeBytes { raw -> Int32 in
                guard let base = raw.bindMemory(to: UInt8.self).baseAddress else { return 0 }
                return bt_mock_tty_write(handle, base, UInt(data.count))
            }
        }

        public func resize(_ dims: PTYDimensions) async throws {
            guard let handle = loadHandle() else { return }
            let cols = UInt16(max(1, min(dims.cols, Int(UInt16.max))))
            let rows = UInt16(max(1, min(dims.rows, Int(UInt16.max))))
            bt_mock_tty_resize(handle, cols, rows)
        }

        public func disconnect() async {
            await MainActor.run {
                self.replayTimer?.invalidate()
                self.replayTimer = nil
            }
            // Drop the callback before freeing so any in-flight Rust-side flush
            // cannot reach back into a half-torn-down Swift object.
            let toFree = takeHandle()
            if let toFree {
                bt_mock_tty_set_output_callback(toFree, nil, nil)
                bt_mock_tty_free(toFree)
            }
            outputContinuation.finish()
        }
    }

    extension Optional where Wrapped == String {
        fileprivate func withCStringOrNil<R>(_ body: (UnsafePointer<CChar>?) -> R) -> R {
            switch self {
            case .none: return body(nil)
            case .some(let str): return str.withCString { body($0) }
            }
        }
    }
#endif
