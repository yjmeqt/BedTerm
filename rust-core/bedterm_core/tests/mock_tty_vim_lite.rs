#![cfg(feature = "mock-tty")]
use std::sync::Mutex;

use bedterm_core::mock_tty::ffi::{
    bt_mock_tty_create, bt_mock_tty_free, bt_mock_tty_set_output_callback, bt_mock_tty_write,
};

static SINK: Mutex<Vec<u8>> = Mutex::new(Vec::new());

unsafe extern "C" fn record(b: *const u8, n: usize, _u: *mut std::ffi::c_void) {
    SINK.lock()
        .unwrap()
        .extend_from_slice(std::slice::from_raw_parts(b, n));
}

fn drain() -> Vec<u8> {
    std::mem::take(&mut *SINK.lock().unwrap())
}

fn make() -> *mut bedterm_core::mock_tty::ffi::BtMockTty {
    drain();
    let h = unsafe { bt_mock_tty_create(1, std::ptr::null()) };
    unsafe {
        bt_mock_tty_set_output_callback(h, Some(record), std::ptr::null_mut());
    }
    h
}

#[test]
fn vim_lite_enters_alt_screen_and_sets_block_cursor() {
    let h = make();
    let s = String::from_utf8_lossy(&drain()).to_string();
    assert!(s.contains("\x1B[?1049h"), "no alt-screen enter: {s:?}");
    assert!(s.contains("\x1B[2 q"), "no block cursor: {s:?}");
    unsafe {
        bt_mock_tty_free(h);
    }
}

#[test]
fn vim_lite_i_switches_to_insert_and_sets_bar_cursor() {
    let h = make();
    drain();
    unsafe {
        bt_mock_tty_write(h, b"i".as_ptr(), 1);
    }
    let s = String::from_utf8_lossy(&drain()).to_string();
    assert!(s.contains("\x1B[6 q"), "expected bar cursor: {s:?}");
    unsafe {
        bt_mock_tty_free(h);
    }
}

#[test]
fn vim_lite_colon_q_exits_alt_screen() {
    let h = make();
    drain();
    unsafe {
        bt_mock_tty_write(h, b":q\r".as_ptr(), 3);
    }
    let s = String::from_utf8_lossy(&drain()).to_string();
    assert!(s.contains("\x1B[?1049l"), "no alt-screen exit: {s:?}");
    unsafe {
        bt_mock_tty_free(h);
    }
}

#[test]
fn vim_lite_esc_from_insert_returns_to_normal_block_cursor() {
    let h = make();
    drain();
    unsafe {
        bt_mock_tty_write(h, b"i".as_ptr(), 1);
    }
    drain();
    unsafe {
        bt_mock_tty_write(h, b"\x1B".as_ptr(), 1);
    }
    let s = String::from_utf8_lossy(&drain()).to_string();
    assert!(
        s.contains("\x1B[2 q"),
        "expected block cursor after ESC: {s:?}"
    );
    unsafe {
        bt_mock_tty_free(h);
    }
}
