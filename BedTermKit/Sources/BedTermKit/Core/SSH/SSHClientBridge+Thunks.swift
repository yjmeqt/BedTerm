//
// SSHClientBridge+Thunks.swift — C ABI thunks for `SSHClientBridge`.
//
// Split out of `SSHClientBridge.swift` to keep both files under the
// 400-line cap. Each `@convention(c)` thunk casts its `ctx` back to
// the `SSHClientBridge` and asserts main-actor isolation — the
// Rust-side session always invokes these from the main queue.
//
// Note: every C ABI argument that crosses into `MainActor.assumeIsolated`
// is reduced to a `UInt` raw address first so Swift's strict-concurrency
// checker doesn't trip on the `Sendable`-less pointer types.
//

import BedTermIOS
import Foundation

func bridgeFromCtx(_ ctx: UnsafeMutableRawPointer?) -> SSHClientBridge? {
    guard let ctx else { return nil }
    return Unmanaged<SSHClientBridge>.fromOpaque(ctx).takeUnretainedValue()
}

let bridgeConnect:
    @convention(c) (
        UnsafeMutableRawPointer?, BtSSHConnectRequest, BtSSHCompletion?,
        UnsafeMutableRawPointer?
    ) -> Void = { ctx, req, completion, completionCtx in
        let ctxAddr = ctx.map { UInt(bitPattern: $0) } ?? 0
        let credentialAddr = req.credential_opaque.map { UInt(bitPattern: $0) } ?? 0
        let bootstrapAddr = UInt(bitPattern: req.bootstrap_payload)
        let cols = req.cols
        let rows = req.rows
        let completionAddr = completion.map { unsafeBitCast($0, to: UInt.self) } ?? 0
        let completionCtxAddr = completionCtx.map { UInt(bitPattern: $0) } ?? 0
        MainActor.assumeIsolated {
            guard ctxAddr != 0, completionAddr != 0 else { return }
            let ctxPtr = UnsafeMutableRawPointer(bitPattern: ctxAddr)
            guard let ctxPtr,
                let bridge = bridgeFromCtx(ctxPtr)
            else { return }
            let credentialPtr: UnsafeRawPointer? =
                credentialAddr == 0 ? nil : UnsafeRawPointer(bitPattern: credentialAddr)
            let bootstrapPtr = UnsafePointer<CChar>(bitPattern: bootstrapAddr)
            let request = BtSSHConnectRequest(
                credential_opaque: credentialPtr,
                cols: cols,
                rows: rows,
                bootstrap_payload: bootstrapPtr)
            let completionFn = unsafeBitCast(completionAddr, to: BtSSHCompletion.self)
            let completionCtxPtr =
                completionCtxAddr == 0
                ? nil
                : UnsafeMutableRawPointer(bitPattern: completionCtxAddr)
            bridge.connect(
                request: request, completion: completionFn, completionCtx: completionCtxPtr)
        }
    }

let bridgeWrite:
    @convention(c) (
        UnsafeMutableRawPointer?, UnsafePointer<UInt8>?, UInt,
        BtSSHCompletion?, UnsafeMutableRawPointer?
    ) -> Void = { ctx, bytes, len, completion, completionCtx in
        let count = Int(len)
        // Copy bytes here so the buffer Rust handed us doesn't outlive the call.
        let copy: [UInt8]
        if let bytes, count > 0 {
            copy = Array(UnsafeBufferPointer(start: bytes, count: count))
        } else {
            copy = []
        }
        let ctxAddr = ctx.map { UInt(bitPattern: $0) } ?? 0
        let completionAddr = completion.map { unsafeBitCast($0, to: UInt.self) } ?? 0
        let completionCtxAddr = completionCtx.map { UInt(bitPattern: $0) } ?? 0
        MainActor.assumeIsolated {
            guard ctxAddr != 0, completionAddr != 0 else { return }
            let ctxPtr = UnsafeMutableRawPointer(bitPattern: ctxAddr)
            guard let ctxPtr,
                let bridge = bridgeFromCtx(ctxPtr)
            else { return }
            let completionFn = unsafeBitCast(completionAddr, to: BtSSHCompletion.self)
            let completionCtxPtr =
                completionCtxAddr == 0
                ? nil
                : UnsafeMutableRawPointer(bitPattern: completionCtxAddr)
            copy.withUnsafeBufferPointer { buf in
                bridge.write(
                    bytes: buf.baseAddress, length: buf.count,
                    completion: completionFn, completionCtx: completionCtxPtr)
            }
        }
    }

let bridgeResize:
    @convention(c) (
        UnsafeMutableRawPointer?, Int32, Int32,
        BtSSHCompletion?, UnsafeMutableRawPointer?
    ) -> Void = { ctx, cols, rows, completion, completionCtx in
        let ctxAddr = ctx.map { UInt(bitPattern: $0) } ?? 0
        let completionAddr = completion.map { unsafeBitCast($0, to: UInt.self) } ?? 0
        let completionCtxAddr = completionCtx.map { UInt(bitPattern: $0) } ?? 0
        MainActor.assumeIsolated {
            guard ctxAddr != 0, completionAddr != 0 else { return }
            let ctxPtr = UnsafeMutableRawPointer(bitPattern: ctxAddr)
            guard let ctxPtr,
                let bridge = bridgeFromCtx(ctxPtr)
            else { return }
            let completionFn = unsafeBitCast(completionAddr, to: BtSSHCompletion.self)
            let completionCtxPtr =
                completionCtxAddr == 0
                ? nil
                : UnsafeMutableRawPointer(bitPattern: completionCtxAddr)
            bridge.resize(
                cols: cols, rows: rows,
                completion: completionFn, completionCtx: completionCtxPtr)
        }
    }

let bridgeDisconnect:
    @convention(c) (
        UnsafeMutableRawPointer?, BtSSHCompletion?, UnsafeMutableRawPointer?
    ) -> Void = { ctx, completion, completionCtx in
        let ctxAddr = ctx.map { UInt(bitPattern: $0) } ?? 0
        let completionAddr = completion.map { unsafeBitCast($0, to: UInt.self) } ?? 0
        let completionCtxAddr = completionCtx.map { UInt(bitPattern: $0) } ?? 0
        MainActor.assumeIsolated {
            guard ctxAddr != 0, completionAddr != 0 else { return }
            let ctxPtr = UnsafeMutableRawPointer(bitPattern: ctxAddr)
            guard let ctxPtr,
                let bridge = bridgeFromCtx(ctxPtr)
            else { return }
            let completionFn = unsafeBitCast(completionAddr, to: BtSSHCompletion.self)
            let completionCtxPtr =
                completionCtxAddr == 0
                ? nil
                : UnsafeMutableRawPointer(bitPattern: completionCtxAddr)
            bridge.disconnect(completion: completionFn, completionCtx: completionCtxPtr)
        }
    }

let bridgeSetOutputSink:
    @convention(c) (
        UnsafeMutableRawPointer?, BtSSHOutputSink?, UnsafeMutableRawPointer?
    ) -> Void = { ctx, sink, sinkCtx in
        let ctxAddr = ctx.map { UInt(bitPattern: $0) } ?? 0
        let sinkAddr = sink.map { unsafeBitCast($0, to: UInt.self) } ?? 0
        let sinkCtxAddr = sinkCtx.map { UInt(bitPattern: $0) } ?? 0
        MainActor.assumeIsolated {
            guard ctxAddr != 0 else { return }
            let ctxPtr = UnsafeMutableRawPointer(bitPattern: ctxAddr)
            guard let ctxPtr,
                let bridge = bridgeFromCtx(ctxPtr)
            else { return }
            let sinkFn: BtSSHOutputSink? =
                sinkAddr == 0 ? nil : unsafeBitCast(sinkAddr, to: BtSSHOutputSink.self)
            let sinkCtxPtr =
                sinkCtxAddr == 0 ? nil : UnsafeMutableRawPointer(bitPattern: sinkCtxAddr)
            bridge.setOutputSink(sinkFn, ctx: sinkCtxPtr)
        }
    }

let bridgeRelease: @convention(c) (UnsafeMutableRawPointer?) -> Void = { ctx in
    guard let ctx else { return }
    // Balance the +1 retain from `bt_swift_ssh_bridge_create`.
    Unmanaged<SSHClientBridge>.fromOpaque(ctx).release()
}

/// Create an `SSHBridgeHandle` on the Rust side wrapping `clientCtx` (a
/// retained `Unmanaged<SSHClient>` opaque pointer). Returns the Rust
/// handle pointer; caller releases via `bt_ios_ssh_bridge_release` on
/// the Rust side.
///
/// The Swift caller is responsible for having previously stashed any
/// credential it wants `connect` to use via `registerSSHCredential(_:)`
/// and threading the returned token into `BtSSHConnectRequest`.
@_cdecl("bt_swift_ssh_bridge_create")
public func bt_swift_ssh_bridge_create(
    _ clientCtx: UnsafeMutableRawPointer
) -> OpaquePointer? {
    let rawAddress = UInt(bitPattern: clientCtx)
    let rawResult: UInt = MainActor.assumeIsolated {
        guard let clientPtr = UnsafeMutableRawPointer(bitPattern: rawAddress) else {
            return 0
        }
        let client = Unmanaged<AnyObject>.fromOpaque(clientPtr).takeRetainedValue()
        guard let ssh = client as? (any SSHClient) else {
            return 0
        }
        let bridge = SSHClientBridge(client: ssh)
        let bridgeCtx = Unmanaged.passRetained(bridge).toOpaque()

        var vtable = BtSSHClientVTable(
            connect: bridgeConnect,
            write: bridgeWrite,
            resize: bridgeResize,
            disconnect: bridgeDisconnect,
            set_output_sink: bridgeSetOutputSink,
            release: bridgeRelease
        )
        let handle: OpaquePointer? = withUnsafePointer(to: &vtable) { vtablePtr in
            bt_ios_register_ssh_bridge(
                UnsafeMutablePointer(mutating: vtablePtr), bridgeCtx)
        }
        guard let handle else { return 0 }
        return UInt(bitPattern: handle)
    }
    return rawResult == 0 ? nil : OpaquePointer(bitPattern: rawResult)
}

/// Release a detail-message `NSString *` that came back through a
/// `BtSSHCompletion`. Mirrors `Unmanaged.fromOpaque(...).release()`.
@_cdecl("bt_ssh_release_message")
public func bt_ssh_release_message(_ msg: UnsafeRawPointer?) {
    guard let msg else { return }
    Unmanaged<NSString>.fromOpaque(msg).release()
}
