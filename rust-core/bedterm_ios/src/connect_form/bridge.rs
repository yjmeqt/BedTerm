//! Rust → Swift bridge declarations for the W24c connect-form VC.
//!
//! All marshalling uses UTF-8, nul-terminated C strings + JSON blobs.
//! Swift implements every symbol below via `@_cdecl` in
//! `ConnectFormBridge.swift`.

#![cfg(target_os = "ios")]

use std::ffi::c_char;

extern "C" {
    /// Return a +1-retained C string JSON snapshot of the existing draft
    /// (label / host / port / username / authMode / passwordSet /
    /// keySet / keyLabel) for the given UUID-string. NULL when `id` is
    /// missing or unparseable. Caller (Rust) frees via
    /// [`bt_swift_connect_form_free_snapshot`].
    pub fn bt_swift_connect_form_prefill_json(id: *const c_char) -> *mut c_char;

    /// Free a snapshot pointer previously returned by
    /// [`bt_swift_connect_form_prefill_json`]. NULL-safe.
    pub fn bt_swift_connect_form_free_snapshot(ptr: *mut c_char);

    /// Persist the draft. `draft_json` is the JSON-encoded
    /// `ConnectFormDraft`. `new_password` is the new password value when
    /// the user typed one (or NULL when preserving the stored secret).
    /// `passphrase` is the new key passphrase or NULL. Returns `true` on
    /// success. On `false`, Rust may call
    /// [`bt_swift_connect_form_last_error`] to retrieve the validation
    /// message. Returns the UUID-string of the saved entry via
    /// `out_id` when non-NULL; caller frees with
    /// [`bt_swift_connect_form_free_snapshot`].
    pub fn bt_swift_connect_form_save(
        draft_json: *const c_char,
        new_password: *const c_char,
        passphrase: *const c_char,
        out_id: *mut *mut c_char,
    ) -> bool;

    /// Return the last validation error message (or NULL when none).
    /// Caller (Rust) frees via [`bt_swift_connect_form_free_snapshot`].
    pub fn bt_swift_connect_form_last_error() -> *mut c_char;

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
    ///
    /// Currently unused by the Rust VC — `save(...)` consults the parked
    /// bytes Swift-side. Kept for Swift-side test parity + a future
    /// Rust-side pre-save validation path.
    #[allow(dead_code)]
    pub fn bt_swift_connect_form_take_pending_key_bytes(out_len: *mut usize) -> *mut u8;

    /// Free a buffer returned by
    /// [`bt_swift_connect_form_take_pending_key_bytes`]. NULL-safe.
    #[allow(dead_code)]
    pub fn bt_swift_connect_form_free_key_bytes(ptr: *mut u8);
}
