//! System font registration for offline rendering.
//!
//! Loads SF Mono (or Menlo fallback) from macOS system fonts and registers
//! them with the Rust renderer, so CLI text rendering uses the same font
//! pipeline as iOS — identical glyph metrics and rasterization.

use std::fs;

/// Register the system monospace font + italic variant.
/// Must be called once before any rendering.
pub(crate) fn register_system_font() {
    // Preferred: SF Mono Regular → fallback: Menlo
    for path in [
        "/System/Library/Fonts/SFNSMono.ttf",
        "/System/Library/Fonts/Menlo.ttc",
    ] {
        if let Ok(data) = fs::read(path) {
            let ret = unsafe {
                bedterm_core::renderer::ffi::bt_font_register_terminal_face(
                    data.as_ptr(),
                    data.len(),
                    std::ptr::null_mut(),
                    0,
                )
            };
            if ret >= 0 {
                eprintln!("[bedterm-render] registered terminal font: {path}");
                break;
            }
        }
    }

    // Italic companion
    if let Ok(data) = fs::read("/System/Library/Fonts/SFNSMonoItalic.ttf") {
        unsafe {
            bedterm_core::renderer::ffi::bt_font_register_aux_face(data.as_ptr(), data.len());
        }
    }
}
