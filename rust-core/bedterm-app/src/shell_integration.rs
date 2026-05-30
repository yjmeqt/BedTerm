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
//! [`bt_ios_shell_integration_payload`] or
//! [`bt_ios_shell_integration_script`]. The host CLI tool
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

/// Convenience: return the payload as a nul-terminated `&CStr`. The
/// embedded bytes are UTF-8 and verified to contain no interior NULs at
/// test time, so this is infallible.
pub fn payload_cstr() -> &'static std::ffi::CStr {
    // The payload is guaranteed to have no interior NUL (verified by
    // `tests::no_interior_nul`), so appending a NUL byte is safe.
    static PAYLOAD_CSTR: std::sync::LazyLock<std::ffi::CString> = std::sync::LazyLock::new(|| {
        std::ffi::CString::new(PAYLOAD_BYTES)
            .expect("shell integration script must not contain interior NUL")
    });
    PAYLOAD_CSTR.as_c_str()
}

/// Legacy C-ABI entry point returning a nul-terminated pointer to the
/// embedded shell-integration script. Deprecated in favour of
/// [`bt_ios_shell_integration_payload`] which avoids the implicit NUL
/// terminator assumption.
///
/// The returned pointer is **static** and must not be freed.
#[no_mangle]
pub unsafe extern "C" fn bt_ios_shell_integration_script() -> *const std::ffi::c_char {
    payload_cstr().as_ptr()
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
    /// it must carry the DCS opener, the hook entry points, and the
    /// Warp-tagged JSON payload shapes the parser expects. Catches the
    /// case where the file was replaced with an empty stub.
    #[test]
    fn embedded_bytes_look_like_integration_script() {
        let text = std::str::from_utf8(PAYLOAD_BYTES).expect("utf-8");
        assert!(text.contains("__BEDTERM_INTEGRATION_INSTALLED"));
        assert!(text.contains("__bedterm_precmd"));
        assert!(text.contains("__bedterm_preexec"));
        assert!(text.contains(r"\033P$d"));
    }

    /// Confirm the payload carries the Warp-tagged JSON hook shapes the
    /// block parser's DCS decoder turns into `IntegPayload`. Missing one
    /// would break the entire block-list view.
    #[test]
    fn embedded_bytes_have_warp_json_hooks() {
        let text = std::str::from_utf8(PAYLOAD_BYTES).expect("utf-8");
        assert!(
            text.contains(r#"{"hook":"Precmd","value":{"pwd":""#),
            "missing Precmd hook JSON\nscript length = {}",
            PAYLOAD_BYTES.len()
        );
        assert!(
            text.contains(r#"{"hook":"Preexec","value":{"command":""#),
            "missing Preexec hook JSON"
        );
        // CommandFinished uses %d (integer exit code), not "%s".
        assert!(
            text.contains(r#"{"hook":"CommandFinished","value":{"exit_code":"#),
            "missing CommandFinished hook JSON"
        );
    }

    /// The C-string convenience accessor must produce valid UTF-8 that
    /// matches the raw byte payload. Catches a mismatch between the
    /// `payload_cstr` lazy init and the `PAYLOAD_BYTES` const.
    #[test]
    fn cstr_matches_bytes() {
        let cstr = super::payload_cstr();
        assert_eq!(
            cstr.to_bytes(),
            PAYLOAD_BYTES,
            "payload_cstr must match PAYLOAD_BYTES (no missing/extra NUL)"
        );
        // Also verify a round-trip via the FFI entry point.
        let ffi_ptr = unsafe { super::bt_ios_shell_integration_script() };
        assert!(!ffi_ptr.is_null());
        let ffi_str = unsafe { std::ffi::CStr::from_ptr(ffi_ptr) };
        assert_eq!(ffi_str.to_bytes(), PAYLOAD_BYTES);
    }

    /// PASTE_DETECTION: if the script ever gains an interior NUL byte,
    /// `payload_cstr` would panic at runtime. This is extremely unlikely
    /// for a hand-written shell script, but worth an explicit assertion so
    /// the failure is caught by unit tests rather than at app launch.
    #[test]
    fn no_interior_nul() {
        assert!(
            !PAYLOAD_BYTES.contains(&b'\0'),
            "embedded script must not contain interior NUL bytes"
        );
    }

    // -----------------------------------------------------------------------
    // Comprehensive shell-integration script content verification
    // -----------------------------------------------------------------------

    /// The embedded bytes must be non-empty — a zero-length payload would
    /// break shell integration completely.
    #[test]
    fn embedded_bytes_are_non_empty() {
        assert!(
            !PAYLOAD_BYTES.is_empty(),
            "script payload must not be empty"
        );
        assert!(
            PAYLOAD_BYTES.len() > 100,
            "script payload must be a substantial shell script, got {} bytes",
            PAYLOAD_BYTES.len()
        );
    }

    /// Confirm the payload carries the Warp-compatible DCS wire-format
    /// markers: the opener `ESC P $ d` and the ST trailer `ESC \`.
    #[test]
    fn embedded_bytes_contain_dcs_markers() {
        let text = std::str::from_utf8(PAYLOAD_BYTES).expect("utf-8");
        // DCS opener: \033P$d (hex-encoded JSON marker).
        assert!(text.contains(r"\033P$d"), "missing DCS opener ESC P $ d");
        // DCS trailer: \033\\ (ESC backslash = 7-bit ST).
        assert!(
            text.contains(r"\033\\"),
            "missing DCS trailer ESC BACKSLASH (7-bit ST)"
        );
    }

    /// The script must detect which shell it is running under — zsh and
    /// bash get different hook mechanisms. Both detection paths must be
    /// present.
    #[test]
    fn embedded_bytes_contain_shell_type_detection() {
        let text = std::str::from_utf8(PAYLOAD_BYTES).expect("utf-8");
        assert!(text.contains("ZSH_VERSION"), "missing zsh detection");
        assert!(text.contains("BASH_VERSION"), "missing bash detection");
    }

    /// The zsh branch must install hooks via `add-zsh-hook` for both
    /// precmd and preexec.
    #[test]
    fn embedded_bytes_contain_zsh_hooks() {
        let text = std::str::from_utf8(PAYLOAD_BYTES).expect("utf-8");
        assert!(text.contains("add-zsh-hook"), "missing add-zsh-hook");
        assert!(
            text.contains("add-zsh-hook precmd"),
            "missing precmd hook registration"
        );
        assert!(
            text.contains("add-zsh-hook preexec"),
            "missing preexec hook registration"
        );
    }

    /// The bash branch must install hooks via `PROMPT_COMMAND` (precmd)
    /// and a `DEBUG` trap (preexec).
    #[test]
    fn embedded_bytes_contain_bash_hooks() {
        let text = std::str::from_utf8(PAYLOAD_BYTES).expect("utf-8");
        assert!(
            text.contains("PROMPT_COMMAND"),
            "missing PROMPT_COMMAND for bash precmd"
        );
        assert!(text.contains("trap"), "missing trap for bash preexec");
        assert!(
            text.contains("__bedterm_preexec"),
            "missing __bedterm_preexec reference"
        );
    }

    /// The guard variable `__BEDTERM_INTEGRATION_INSTALLED` ensures
    /// idempotency — sourcing the script twice must be a no-op.
    #[test]
    fn embedded_bytes_contain_idempotency_guard() {
        let text = std::str::from_utf8(PAYLOAD_BYTES).expect("utf-8");
        assert!(
            text.contains("__BEDTERM_INTEGRATION_INSTALLED"),
            "missing idempotency guard variable"
        );
        // The guard must be checked at the top and set at the bottom.
        assert!(
            text.contains(r#"${__BEDTERM_INTEGRATION_INSTALLED:-}"#),
            "missing guard check at script entry"
        );
        assert!(
            text.contains("__BEDTERM_INTEGRATION_INSTALLED=1"),
            "missing guard set at script exit"
        );
    }

    /// All three emit helper functions must be present: precmd, preexec,
    /// and command-finished.
    #[test]
    fn embedded_bytes_contain_emit_helpers() {
        let text = std::str::from_utf8(PAYLOAD_BYTES).expect("utf-8");
        assert!(text.contains("__bedterm_emit_precmd"));
        assert!(text.contains("__bedterm_emit_preexec"));
        assert!(text.contains("__bedterm_emit_finished"));
    }

    /// The JSON-escaping and hex-encoding helper functions must be present
    /// since they are called by the emit helpers.
    #[test]
    fn embedded_bytes_contain_format_helpers() {
        let text = std::str::from_utf8(PAYLOAD_BYTES).expect("utf-8");
        assert!(
            text.contains("__bedterm_json_escape"),
            "missing JSON escape helper"
        );
        assert!(text.contains("__bedterm_hex"), "missing hex encoder helper");
        assert!(
            text.contains("__bedterm_emit_dcs"),
            "missing DCS wrapper helper"
        );
    }

    /// The git-branch helper is used by the precmd hook for the
    /// `git_branch` field — it must be present.
    #[test]
    fn embedded_bytes_contain_git_helper() {
        let text = std::str::from_utf8(PAYLOAD_BYTES).expect("utf-8");
        assert!(
            text.contains("__bedterm_git_branch"),
            "missing git branch helper"
        );
        assert!(
            text.contains("git symbolic-ref"),
            "missing git symbolic-ref call"
        );
    }

    /// A reasonable line count guards against accidental truncation or
    /// replacement with a tiny stub. The script must be at least 50 lines
    /// of substantive shell code.
    #[test]
    fn embedded_bytes_line_count_reasonable() {
        let text = std::str::from_utf8(PAYLOAD_BYTES).expect("utf-8");
        let line_count = text.lines().count();
        assert!(
            line_count >= 50,
            "script has only {line_count} lines — expected at least 50"
        );
        assert!(
            line_count <= 500,
            "script has {line_count} lines — expected at most 500"
        );
    }

    /// The entire payload must be valid UTF-8 — the Swift side reads it
    /// as a String and any non-UTF-8 bytes would crash at runtime.
    #[test]
    fn embedded_bytes_valid_utf8() {
        assert!(
            std::str::from_utf8(PAYLOAD_BYTES).is_ok(),
            "embedded script is not valid UTF-8"
        );
    }

    /// The first line must be a shell comment (hashbang or descriptive
    /// header) — a valid shell script always starts with a non-executable
    /// line for the shebang or file-level doc comment.
    #[test]
    fn embedded_bytes_starts_with_hash_header() {
        let text = std::str::from_utf8(PAYLOAD_BYTES).expect("utf-8");
        let first_line = text.lines().next().expect("script has no lines");
        assert!(
            first_line.starts_with('#'),
            "first line must be a comment/header, got: {first_line:?}"
        );
    }

    /// The script must handle the case where the remote shell is neither
    /// bash nor zsh — the else branch should be a silent no-op.
    #[test]
    fn embedded_bytes_contains_unknown_shell_fallback() {
        let text = std::str::from_utf8(PAYLOAD_BYTES).expect("utf-8");
        assert!(
            text.contains("Unknown shell"),
            "missing unknown shell fallback comment"
        );
        assert!(
            text.contains("quietly skip"),
            "missing 'quietly skip' in fallback"
        );
    }

    /// The JSON body wrapped in DCS must include the pwd field in the
    /// Precmd payload (with git_branch).
    #[test]
    fn embedded_bytes_precmd_has_git_branch() {
        let text = std::str::from_utf8(PAYLOAD_BYTES).expect("utf-8");
        assert!(
            text.contains("\"git_branch\""),
            "Precmd JSON must include git_branch field"
        );
    }

    /// The script should not contain any bare `\r` (carriage return)
    /// bytes that would cause line-ending issues when sent over SSH.
    #[test]
    fn embedded_bytes_no_windows_line_endings() {
        assert!(
            !PAYLOAD_BYTES.contains(&b'\r'),
            "script must use unix line endings (no CR bytes)"
        );
    }
}
