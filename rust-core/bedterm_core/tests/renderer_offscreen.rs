//! Renders one fixture offscreen and asserts the texture is not all zeros.
//!
//! macOS-only: this test instantiates an actual MTLDevice. iOS targets
//! cannot run this test (no display server in cargo test); CI for iOS
//! relies on the XCTest layer which lives in Swift.

#![cfg(target_os = "macos")]

use bedterm_core::ffi::{
    bt_term_feed, bt_term_free, bt_term_new, bt_term_set_palette, BtPaletteView,
};
use bedterm_core::renderer::block_list_ffi::{bt_renderer_draw_block_list, BtBlockLayoutEntry};
use bedterm_core::renderer::ffi::{bt_renderer_draw, bt_renderer_free, bt_renderer_new};
use bedterm_core::term::BtRgb24;
use metal::{
    foreign_types::ForeignType, Device, MTLPixelFormat, MTLRegion, MTLTextureUsage,
    TextureDescriptor,
};

#[test]
fn renders_hello_world_into_offscreen_texture() {
    let device = Device::system_default().expect("metal device");
    let queue = device.new_command_queue();

    // bt_renderer_new takes ownership of one retain on each pointer (matches
    // Swift's Unmanaged.passRetained convention). We give it +1-retained
    // clones so the renderer can release them on drop, while `device`/`queue`
    // remain valid for our own use here in the test.
    let device_ptr = Clone::clone(&device).into_ptr() as *const _;
    let queue_ptr = Clone::clone(&queue).into_ptr() as *const _;
    let renderer = unsafe { bt_renderer_new(device_ptr, queue_ptr) };
    assert!(!renderer.is_null(), "bt_renderer_new returned null");

    let term = bt_term_new(80, 24);
    assert!(!term.is_null(), "bt_term_new returned null");

    let payload = b"Hello, terminal!\n";
    unsafe {
        bt_term_feed(term, payload.as_ptr(), payload.len());
    }

    let desc = TextureDescriptor::new();
    desc.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
    desc.set_width(1024);
    desc.set_height(768);
    desc.set_usage(MTLTextureUsage::ShaderRead | MTLTextureUsage::RenderTarget);
    let tex = device.new_texture(&desc);

    let rc = unsafe { bt_renderer_draw(renderer, term, tex.as_ptr() as *const _, 1024, 768, 0.0) };
    assert_eq!(rc, 0, "bt_renderer_draw returned {rc}");

    // Wait for the GPU work to actually finish before reading back.
    // Submit a no-op command buffer and wait on it; this drains the queue.
    let drain = queue.new_command_buffer();
    drain.commit();
    drain.wait_until_completed();

    // Read pixels back.
    let bytes_per_row = 1024 * 4;
    let mut bytes = vec![0u8; bytes_per_row * 768];
    let region = MTLRegion::new_2d(0, 0, 1024, 768);
    tex.get_bytes(
        bytes.as_mut_ptr() as *mut _,
        bytes_per_row as u64,
        region,
        0,
    );

    // Count non-zero R/G/B bytes (skip the alpha lane). Clear colour gives
    // alpha=255 for every pixel, so if we only counted any non-zero we'd
    // pass even on a completely cleared texture.
    let mut non_zero_rgb = 0usize;
    for chunk in bytes.chunks_exact(4) {
        if chunk[0] != 0 || chunk[1] != 0 || chunk[2] != 0 {
            non_zero_rgb += 1;
        }
    }
    assert!(
        non_zero_rgb > 25,
        "expected glyph pixels in render output, got {non_zero_rgb} non-zero RGB pixels",
    );
    println!("non_zero_rgb pixels = {non_zero_rgb}");

    unsafe {
        bt_term_free(term);
        bt_renderer_free(renderer);
    }
}

#[test]
fn draw_block_list_paints_first_visible_block() {
    let device = Device::system_default().expect("metal device");
    let queue = device.new_command_queue();

    let device_ptr = Clone::clone(&device).into_ptr() as *const _;
    let queue_ptr = Clone::clone(&queue).into_ptr() as *const _;
    let renderer = unsafe { bt_renderer_new(device_ptr, queue_ptr) };
    assert!(!renderer.is_null());

    let term = bt_term_new(80, 24);
    assert!(!term.is_null());

    // OSC 133;A ;C cmd=bHM= ;D;0 -> one sealed block whose live range
    // resolves via snapshot_range (frozen_snapshot is set on D).
    let stream = b"\x1b]133;A\x1b\\hi\n\x1b]133;C;cmd=bHM=\x1b\\\x1b]133;D;0\x1b\\";
    unsafe {
        bt_term_feed(term, stream.as_ptr(), stream.len());
    }

    let desc = TextureDescriptor::new();
    desc.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
    desc.set_width(256);
    desc.set_height(128);
    desc.set_usage(MTLTextureUsage::ShaderRead | MTLTextureUsage::RenderTarget);
    let tex = device.new_texture(&desc);

    // Single entry: paint block 1 (first sealed block) at body Y = 0, 60px tall.
    let entry = BtBlockLayoutEntry {
        block_id: 1,
        body_y_top_px: 0.0,
        body_height_px: 60.0,
        panel_y_top_px: 0.0,
        panel_height_px: 60.0,
        panel_x_left_px: 0.0,
        panel_width_px: 256.0,
        panel_bg_rgba: 0, // skip panel chrome in the smoke test
        panel_corner_radius_px: 0.0,
    };
    let rc = unsafe {
        bt_renderer_draw_block_list(
            renderer,
            term,
            tex.as_ptr() as *const _,
            256,
            128,
            0.0,
            &entry as *const _,
            1,
            std::ptr::null(),
            0,
        )
    };
    assert_eq!(rc, 0);

    let drain = queue.new_command_buffer();
    drain.commit();
    drain.wait_until_completed();

    // Empty-entries path: should clear the viewport without crashing.
    let rc_empty = unsafe {
        bt_renderer_draw_block_list(
            renderer,
            term,
            tex.as_ptr() as *const _,
            256,
            128,
            0.0,
            std::ptr::null(),
            0,
            std::ptr::null(),
            0,
        )
    };
    assert_eq!(rc_empty, 0);

    unsafe {
        bt_term_free(term);
        bt_renderer_free(renderer);
    }
}

/// End-to-end pipeline check for the Option-3 palette decoupling:
/// seal a block, then render it twice — once under a "light" palette
/// (white bg / black fg) and once under a "dark" palette (inverted) —
/// against the same `BtTerm`. Sampling a known body pixel both times
/// must yield different colours, proving the renderer re-resolves the
/// frozen snapshot through the current palette.
#[test]
fn frozen_block_repaints_under_palette_flip() {
    fn dcs7(json: &str) -> Vec<u8> {
        use std::fmt::Write;
        let mut v = vec![0x1B, b'P', b'$', b'd'];
        let mut hex = String::with_capacity(json.len() * 2);
        for b in json.bytes() {
            write!(hex, "{b:02x}").unwrap();
        }
        v.extend_from_slice(hex.as_bytes());
        v.extend_from_slice(b"\x1b\\");
        v
    }
    fn feed_str(term: *mut bedterm_core::ffi::BtTerm, bytes: &[u8]) {
        unsafe { bt_term_feed(term, bytes.as_ptr(), bytes.len()) }
    }
    let device = Device::system_default().expect("metal device");
    let queue = device.new_command_queue();
    let device_ptr = Clone::clone(&device).into_ptr() as *const _;
    let queue_ptr = Clone::clone(&queue).into_ptr() as *const _;
    let renderer = unsafe { bt_renderer_new(device_ptr, queue_ptr) };
    let term = bt_term_new(20, 5);

    // Seal one block: Precmd → Preexec → "hi\r\n" → CommandFinished.
    feed_str(term, &dcs7(r#"{"hook":"Precmd","value":{"pwd":"/x"}}"#));
    feed_str(
        term,
        &dcs7(r#"{"hook":"Preexec","value":{"command":"ls"}}"#),
    );
    feed_str(term, b"hi\r\n");
    feed_str(
        term,
        &dcs7(r#"{"hook":"CommandFinished","value":{"exit_code":0}}"#),
    );

    let desc = TextureDescriptor::new();
    desc.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
    desc.set_width(256);
    desc.set_height(64);
    desc.set_usage(MTLTextureUsage::ShaderRead | MTLTextureUsage::RenderTarget);
    let tex = device.new_texture(&desc);
    let entry = BtBlockLayoutEntry {
        block_id: 1,
        body_y_top_px: 0.0,
        body_height_px: 64.0,
        panel_y_top_px: 0.0,
        panel_height_px: 64.0,
        panel_x_left_px: 0.0,
        panel_width_px: 256.0,
        panel_bg_rgba: 0,
        panel_corner_radius_px: 0.0,
    };

    let sample = |palette: &BtPaletteView| -> (u8, u8, u8) {
        unsafe { bt_term_set_palette(term, palette as *const BtPaletteView) };
        let rc = unsafe {
            bt_renderer_draw_block_list(
                renderer,
                term,
                tex.as_ptr() as *const _,
                256,
                64,
                0.0,
                &entry as *const _,
                1,
                std::ptr::null(),
                0,
            )
        };
        assert_eq!(rc, 0, "draw_block_list failed");
        let drain = queue.new_command_buffer();
        drain.commit();
        drain.wait_until_completed();

        let bytes_per_row = 256 * 4;
        let mut bytes = vec![0u8; bytes_per_row * 64];
        let region = MTLRegion::new_2d(0, 0, 256, 64);
        tex.get_bytes(
            bytes.as_mut_ptr() as *mut _,
            bytes_per_row as u64,
            region,
            0,
        );
        // Sample bottom-right corner — well past "hi" (col 0–1), so the
        // pixel reads pure cell background, which is exactly what the
        // palette controls.
        let (px, py) = (240usize, 50usize);
        let i = py * bytes_per_row + px * 4;
        // BGRA8Unorm in memory: B, G, R, A in that order.
        (bytes[i + 2], bytes[i + 1], bytes[i])
    };

    let light = BtPaletteView {
        default_fg: BtRgb24 {
            r: 0x00,
            g: 0x00,
            b: 0x00,
        },
        default_bg: BtRgb24 {
            r: 0xFF,
            g: 0xFF,
            b: 0xFF,
        },
        ansi: [BtRgb24 { r: 0, g: 0, b: 0 }; 16],
    };
    let dark = BtPaletteView {
        default_fg: BtRgb24 {
            r: 0xFF,
            g: 0xFF,
            b: 0xFF,
        },
        default_bg: BtRgb24 {
            r: 0x11,
            g: 0x11,
            b: 0x11,
        },
        ansi: [BtRgb24 {
            r: 0xFF,
            g: 0xFF,
            b: 0xFF,
        }; 16],
    };

    let (lr, lg, lb) = sample(&light);
    let (dr, dg, db) = sample(&dark);
    println!("light bg pixel = ({lr},{lg},{lb}); dark bg pixel = ({dr},{dg},{db})");
    assert!(
        lr > 0xE0 && lg > 0xE0 && lb > 0xE0,
        "light palette should paint near-white at sample point, got ({lr},{lg},{lb})"
    );
    assert!(
        dr < 0x40 && dg < 0x40 && db < 0x40,
        "dark palette should paint near-black at sample point, got ({dr},{dg},{db})"
    );

    unsafe {
        bt_term_free(term);
        bt_renderer_free(renderer);
    }
}
