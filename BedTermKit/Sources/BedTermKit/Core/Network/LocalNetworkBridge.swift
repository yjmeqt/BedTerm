import Foundation

/// Cross-FFI bridge that lets the Rust onboarding coordinator trigger the
/// iOS Local Network permission prompt.
///
/// `LocalNetworkPrewarmer` (Network.framework wrapper) stays in Swift —
/// Rust merely calls into this shim with an opaque `ctx` pointer + a
/// C completion callback. We forward to `LocalNetworkPrewarmer.requestPermission`
/// inside a `@MainActor` task and fire the completion when the probe
/// resolves. The Rust side treats the completion as a one-shot.
@_cdecl("bt_swift_request_local_network")
public func btSwiftRequestLocalNetwork(
    _ ctx: UnsafeMutableRawPointer?,
    _ completion: @convention(c) (UnsafeMutableRawPointer?) -> Void
) {
    // `@convention(c)` function pointers + raw pointers are not `Sendable`
    // under Swift 6 strict concurrency; smuggle them through a Sendable
    // wrapper. The C ABI guarantees the function pointer + ctx remain
    // valid until `completion(ctx)` fires.
    let box = LocalNetworkBridgeBox(ctx: ctx, completion: completion)
    Task { @MainActor in
        _ = await LocalNetworkPrewarmer.shared.requestPermission()
        box.fire()
    }
}

/// Sendable wrapper that pins a C function pointer + opaque ctx so the
/// Swift 6 task-isolation checker accepts crossing them into a `Task`.
/// The wrapped values are inherently main-thread / C-ABI safe because
/// the Rust caller fires `bt_swift_request_local_network` from
/// `BtIosOnboardingFlowVC` on the main queue.
private final class LocalNetworkBridgeBox: @unchecked Sendable {
    let ctx: UnsafeMutableRawPointer?
    let completion: @convention(c) (UnsafeMutableRawPointer?) -> Void

    init(
        ctx: UnsafeMutableRawPointer?,
        completion: @convention(c) (UnsafeMutableRawPointer?) -> Void
    ) {
        self.ctx = ctx
        self.completion = completion
    }

    func fire() {
        completion(ctx)
    }
}
