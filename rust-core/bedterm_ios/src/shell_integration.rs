//! Compile-time-embedded shell-integration script.
//!
//! The DCS shell-integration body (`bedterm-integration.sh`) used to ship
//! as a SwiftPM `.copy` resource and was read at runtime from
//! `Bundle.module`. We now embed it directly in the staticlib via
//! `include_bytes!` so the bytes are in `.rodata`, callers don't pay a
//! bundle lookup, and there's only one place on disk where the script
//! lives (the Rust crate's `assets/` dir).
//!
//! Swift's `ShellIntegrationScript` is a one-line shim over
//! [`bt_ios_shell_integration_payload`]. The host CLI tool
//! `bedterm-record` opens the file directly from this same path on disk —
//! it isn't linked against the staticlib.

/// Raw UTF-8 bytes of `bedterm-integration.sh`. Static lifetime; no
/// allocation, no caller-side free.
pub const PAYLOAD_BYTES: &[u8] = include_bytes!("../assets/bedterm-integration.sh");

/// Return a pointer to the embedded `bedterm-integration.sh` payload
/// and write its length through `out_len`.
///
/// The returned pointer is **static** — it lives in the staticlib's
/// `.rodata` for the lifetime of the process. Callers must not free it
/// and must not mutate the bytes. The payload is not nul-terminated;
/// always use the returned length.
///
/// # Safety
/// `out_len` must be a valid, writable pointer to a `usize`. Pass NULL
/// to skip the length write (only useful as a liveness check).
#[no_mangle]
pub unsafe extern "C" fn bt_ios_shell_integration_payload(out_len: *mut usize) -> *const u8 {
    if !out_len.is_null() {
        unsafe { *out_len = PAYLOAD_BYTES.len() };
    }
    PAYLOAD_BYTES.as_ptr()
}

#[cfg(test)]
mod tests {
    use super::PAYLOAD_BYTES;

    /// The embedded bytes must match the on-disk source verbatim — any
    /// drift would mean a stale build cache or a script edit that didn't
    /// recompile. Compare against a fresh read of the same file via the
    /// `CARGO_MANIFEST_DIR`-relative path.
    #[test]
    fn embedded_bytes_match_source_file() {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let path = std::path::Path::new(manifest_dir).join("assets/bedterm-integration.sh");
        let on_disk = std::fs::read(&path).expect("read assets/bedterm-integration.sh");
        assert_eq!(on_disk.len(), PAYLOAD_BYTES.len());
        assert_eq!(on_disk, PAYLOAD_BYTES);
    }

    /// Spot-check that the payload looks like the integration script:
    /// it must carry the DCS opener and the hook entry points. Catches
    /// the case where the file was replaced with an empty stub.
    #[test]
    fn embedded_bytes_look_like_integration_script() {
        let text = std::str::from_utf8(PAYLOAD_BYTES).expect("utf-8");
        assert!(text.contains("__BEDTERM_INTEGRATION_INSTALLED"));
        assert!(text.contains("__bedterm_precmd"));
        assert!(text.contains("__bedterm_preexec"));
        assert!(text.contains(r"\033P$d"));
    }
}
