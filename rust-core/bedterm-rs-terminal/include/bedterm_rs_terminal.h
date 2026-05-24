// bedterm-rs-terminal FFI declarations.
// Appended to bedterm_core.h by build-rust-xcframework.sh after cbindgen
// regenerates the core header. Do not place include guards here — this
// fragment is spliced inside the existing BEDTERM_CORE_H guard.

// ── bedterm-rs-terminal ─────────────────────────────────────────────────────

/// Callback invoked on the main thread when the Rust terminal's back button
/// is tapped.
typedef void (*BtRsBackCallback)(void *ctx);

/// Create a Rust-backed `UIViewController *` (returned as opaque `void *`).
/// The returned pointer is +1 retained; release with `bt_rs_terminal_release_vc`.
/// The navigation title is set by the caller via SwiftUI `.navigationTitle()`.
///
/// @param on_back    Back-tap callback (may be NULL).
/// @param ctx        Context pointer passed through to `on_back` (may be NULL).
void *bt_rs_terminal_create_vc(BtRsBackCallback on_back,
                                void *ctx);

/// Release a `UIViewController *` previously returned by `bt_rs_terminal_create_vc`.
/// Safe to call with NULL.
void bt_rs_terminal_release_vc(void *vc_ptr);
