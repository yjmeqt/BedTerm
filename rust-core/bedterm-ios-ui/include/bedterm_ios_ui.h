// bedterm-ios-ui FFI declarations.
// Appended to bedterm_core.h by build-rust-xcframework.sh after cbindgen
// regenerates the core header. Do not place include guards here — this
// fragment is spliced inside the existing BEDTERM_CORE_H guard.

// ── bedterm-ios-ui ──────────────────────────────────────────────────────────

/// Callback invoked on the main thread when the iOS terminal's back button
/// is tapped.
typedef void (*BtIosBackCallback)(void *ctx);

/// Create the iOS terminal `UIViewController *` (returned as opaque `void *`).
/// The returned pointer is +1 retained; release with `bt_ios_release_vc`.
/// The navigation title is set by the caller via SwiftUI `.navigationTitle()`.
///
/// @param on_back    Back-tap callback (may be NULL).
/// @param ctx        Context pointer passed through to `on_back` (may be NULL).
void *bt_ios_create_vc(BtIosBackCallback on_back,
                       void *ctx);

/// Release a `UIViewController *` previously returned by `bt_ios_create_vc`.
/// Safe to call with NULL.
void bt_ios_release_vc(void *vc_ptr);
