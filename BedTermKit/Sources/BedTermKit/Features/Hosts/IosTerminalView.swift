import BedTermCoreC
import Foundation
import UIKit

/// Owns the Rust UIViewController returned by `bt_ios_create_vc`, the
/// inbound feed-forwarding `Task`, and the `on_send` / `on_resize`
/// trampoline allocations.
///
/// Architecture
/// ------------
/// Swift `TerminalSession` remains the SSH driver (Citadel + BlockStore).
/// The Rust VC is hosted by `TerminalScreenViewController` (a UIKit
/// container pushed directly onto the app's `UINavigationController`) and
/// rendering is forwarded:
///
///   - Inbound: `TerminalSession.feed` (`AsyncStream<Data>`) → the Rust
///     metal view's owned `BtTerm` via `bt_ios_view_feed_bytes`. The Rust
///     glyph renderer paints in `drawRect:`.
///   - Outbound (hardware-keyboard / IME commit): the metal view's
///     `on_send` callback (installed via `bt_ios_view_set_on_send`) →
///     `TerminalSession.send`.
///
/// Deferrals
/// ---------
/// The Rust VC's composer / keybar / dpad chips call `session.send(...)`
/// on an attached **Rust** `BtIosTerminalSession`; we don't attach one
/// here (it would require bridging the live Citadel client into Rust and
/// duplicating block-store ownership). Hardware-keyboard text entry
/// works end-to-end via `on_send`; the on-screen composer / keybar taps
/// are visual-only until the Rust session is wired into the Swift flow.
/// See `BedTerm/docs/specs/swift-rust-ssh-bridge.md`.
@MainActor
@Observable
final class IosTerminalHost {
    let session: TerminalSession
    /// Retained Rust VC pointer (`+1`); released in `teardown()`.
    @ObservationIgnored fileprivate var vcPtr: UnsafeMutableRawPointer?
    /// Feed-forwarding pump.
    @ObservationIgnored private var feedTask: Task<Void, Never>?
    /// Heap-allocated trampoline box keeping the `send` closure alive
    /// while Rust holds a raw pointer to it.
    @ObservationIgnored private var sendTrampoline: SendTrampoline?
    /// Trampoline for the Rust → Swift resize callback.
    @ObservationIgnored private var resizeTrampoline: ResizeTrampoline?
    /// Most recent `(cols, rows)` reported by the Rust metal view's
    /// `layoutSubviews`, in grid units.
    @ObservationIgnored fileprivate var latestGridDim: PTYDimensions?

    init(session: TerminalSession) {
        self.session = session
    }

    /// Construct the Rust VC + wire the inbound + outbound bridges. Idempotent.
    func makeVC(onBack: @escaping () -> Void) -> UIViewController {
        if let vcPtr {
            return Unmanaged<UIViewController>.fromOpaque(vcPtr).takeUnretainedValue()
        }
        let trampoline = SendTrampoline { [weak self] data in
            self?.session.send(data)
        }
        self.sendTrampoline = trampoline
        let backBox = BackTrampoline(onBack: onBack)
        let backCtx = Unmanaged.passRetained(backBox).toOpaque()
        guard
            let raw = bt_ios_create_vc(
                { rawCtx in
                    guard let rawCtx else { return }
                    let box = Unmanaged<BackTrampoline>.fromOpaque(rawCtx).takeUnretainedValue()
                    box.onBack()
                }, backCtx)
        else {
            // Should never happen in practice. Fall back to plain VC.
            return UIViewController()
        }
        vcPtr = raw
        // Force `viewDidLoad` on the Rust VC so the embedded metal view
        // exists before we query for it. Without this, `bt_ios_vc_metal_view`
        // returns null (view1 ivar still nil), and both the on_send and
        // on_resize installs below silently no-op — typed bytes then drop on
        // the floor and resize events never propagate to the SSH PTY.
        let vc = Unmanaged<UIViewController>.fromOpaque(raw).takeUnretainedValue()
        vc.loadViewIfNeeded()
        // Install the on_send callback on the metal view so hardware-key
        // / IME-commit bytes route into our Swift session.
        if let mv = bt_ios_vc_metal_view(raw) {
            let ctx = Unmanaged.passUnretained(trampoline).toOpaque()
            bt_ios_view_set_on_send(
                mv,
                { ctx, bytes, len in
                    guard let ctx, let bytes, len > 0 else { return }
                    let tramp = Unmanaged<SendTrampoline>.fromOpaque(ctx).takeUnretainedValue()
                    let data = Data(bytes: bytes, count: Int(len))
                    MainActor.assumeIsolated { tramp.fire(data) }
                },
                ctx
            )
            installResizeCallback(metalView: mv)
        }
        // Start the inbound feed pump — every `TerminalSession.feed`
        // chunk is also pushed into the Rust metal view's owned BtTerm
        // so the Rust glyph renderer paints what the SSH bridge sees.
        let vcRef = raw
        feedTask = Task { @MainActor [session, weak self] in
            for await chunk in session.feed {
                guard self != nil else { return }
                guard let mv = bt_ios_vc_metal_view(vcRef) else { continue }
                chunk.withUnsafeBytes { raw in
                    guard let base = raw.baseAddress?.assumingMemoryBound(to: UInt8.self)
                    else { return }
                    bt_ios_view_feed_bytes(mv, base, UInt(raw.count))
                }
            }
        }
        return Unmanaged<UIViewController>.fromOpaque(raw).takeUnretainedValue()
    }

    /// Install the on_resize callback on `metalView`. Extracted so
    /// `makeVC` stays under the cyclomatic-complexity limit.
    private func installResizeCallback(metalView: UnsafeMutableRawPointer) {
        // Install the on_resize callback so every viewport change pushes
        // a fresh PTY size into our session (and therefore into the SSH
        // peer). Without this the shell formats for whatever cols/rows
        // the session was opened with and wraps oddly when the device's
        // actual grid is narrower.
        let resizeTramp = ResizeTrampoline { [weak self] cols, rows in
            guard let self else { return }
            let dims = PTYDimensions(cols: Int(cols), rows: Int(rows))
            self.latestGridDim = dims
            self.session.resize(cols: Int(cols), rows: Int(rows))
        }
        self.resizeTrampoline = resizeTramp
        let resizeCtx = Unmanaged.passUnretained(resizeTramp).toOpaque()
        bt_ios_view_set_on_resize(
            metalView,
            { ctx, cols, rows in
                guard let ctx else { return }
                let box = Unmanaged<ResizeTrampoline>.fromOpaque(ctx).takeUnretainedValue()
                MainActor.assumeIsolated { box.fire(cols, rows) }
            },
            resizeCtx
        )
    }

    /// Release the Rust VC + cancel the feed pump. Safe to call twice.
    func teardown() {
        feedTask?.cancel()
        feedTask = nil
        if let vcPtr {
            if let mv = bt_ios_vc_metal_view(vcPtr) {
                bt_ios_view_set_on_send(mv, nil, nil)
                bt_ios_view_set_on_resize(mv, nil, nil)
            }
            bt_ios_release_vc(vcPtr)
        }
        vcPtr = nil
        sendTrampoline = nil
        resizeTrampoline = nil
    }

    /// Wait (up to `timeoutMs`) for the Rust metal view to publish its
    /// renderer-derived `(cols, rows)` so we can open the SSH PTY at the
    /// actual viewport. Returns `nil` if the deadline elapses without a
    /// callback — the caller falls back to a default.
    func awaitInitialGridDim(timeoutMs: UInt64) async -> PTYDimensions? {
        if let cached = latestGridDim { return cached }
        let deadline = Date().addingTimeInterval(Double(timeoutMs) / 1000.0)
        while Date() < deadline {
            // Also probe the view directly — some test paths inject the
            // VC pointer without a SwiftUI representable, so the resize
            // callback may not be wired yet.
            if let vcPtr, let mv = bt_ios_vc_metal_view(vcPtr) {
                var cols: UInt16 = 0
                var rows: UInt16 = 0
                bt_ios_view_grid_dim(mv, &cols, &rows)
                if cols > 0 && rows > 0 {
                    let dims = PTYDimensions(cols: Int(cols), rows: Int(rows))
                    latestGridDim = dims
                    return dims
                }
            }
            if let cached = latestGridDim { return cached }
            try? await Task.sleep(nanoseconds: 16_000_000)  // ~1 frame
        }
        return latestGridDim
    }
}

/// Small heap box so a stable `Unmanaged` pointer can be handed to the
/// Rust on_send sink without retain-cycling the host.
@MainActor
private final class SendTrampoline {
    let send: (Data) -> Void
    init(send: @escaping (Data) -> Void) { self.send = send }
    func fire(_ data: Data) { send(data) }
}

private final class BackTrampoline {
    let onBack: () -> Void
    init(onBack: @escaping () -> Void) { self.onBack = onBack }
}

/// Heap box for the Rust on_resize trampoline.
@MainActor
private final class ResizeTrampoline {
    let resize: (UInt16, UInt16) -> Void
    init(resize: @escaping (UInt16, UInt16) -> Void) { self.resize = resize }
    func fire(_ cols: UInt16, _ rows: UInt16) { resize(cols, rows) }
}
