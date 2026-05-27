//! Rust -> Swift bridge declarations for the W24c connect-form VC.
//!
//! After the W24e refactor the VC calls the Rust `connect_form_vm` directly
//! for all state + validation. The only functions that still cross the FFI
//! seam are:
//!
//! - **File picker** — `bt_swift_connect_form_pick_key`,
//!   `bt_swift_connect_form_take_pending_key_bytes`,
//!   `bt_swift_connect_form_free_key_bytes` — irreducibly Swift
//!   (UIDocumentPickerViewController).
//! - **Persistence** — `bt_swift_hosts_store_save_json` — the thin
//!   callback that deserialises the SaveOutcome JSON and calls
//!   `HostsStore.save()`.

#![cfg(target_os = "ios")]

use std::ffi::c_char;

extern "C" {
    /// Present the system document picker and pipe the picked file's
    /// bytes into `ConnectFormBridge.pendingKeyBytes`. Fires
    /// `on_picked(ctx, label_or_null)` once the user picks (or
    /// cancels — `label_or_null` is NULL on cancel). The label is the
    /// file's `lastPathComponent` (e.g. `id_ed25519`).
    pub fn bt_swift_connect_form_pick_key(
        on_picked: unsafe extern "C" fn(ctx: *mut std::ffi::c_void, label: *const c_char),
        ctx: *mut std::ffi::c_void,
    );

    /// Take ownership of the bytes parked by the most recent successful
    /// picker round-trip. Writes the byte count via `out_len` and
    /// returns a +1 malloc'd buffer (caller frees with
    /// [`bt_swift_connect_form_free_key_bytes`]). Returns NULL when no
    /// pending key is parked. Clears the pending slot regardless.
    pub fn bt_swift_connect_form_take_pending_key_bytes(out_len: *mut usize) -> *mut u8;

    /// Free a buffer returned by
    /// [`bt_swift_connect_form_take_pending_key_bytes`]. NULL-safe.
    pub fn bt_swift_connect_form_free_key_bytes(ptr: *mut u8);

    /// Persist a JSON-encoded [`crate::connect_form_vm::SaveOutcome`]
    /// through Swift's `HostsStore`. Swift deserialises the JSON, builds a
    /// `SavedHost`, and calls `HostsStore().save()`. The JSON pointer is
    /// borrowed for the duration of the call.
    pub fn bt_swift_hosts_store_save_json(json: *const c_char);
}
