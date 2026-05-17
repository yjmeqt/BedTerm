#![cfg(feature = "mock-tty")]
use std::sync::Mutex;

use bedterm_core::mock_tty::ffi::{
    bt_mock_tty_create, bt_mock_tty_free, bt_mock_tty_set_output_callback, bt_mock_tty_tick,
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

#[test]
fn replay_releases_events_in_order_at_recorded_times() {
    let cast_text = "{\"version\":2,\"width\":80,\"height\":24}\n\
        [0.0,\"o\",\"hi\"]\n\
        [0.5,\"o\",\" world\\r\\n\"]\n\
        [1.0,\"o\",\"\\u001b[?1049h\"]\n";
    // Pass cast_inline via opts_json. The inline value must be JSON-escaped
    // because it lives inside the outer JSON object.
    let json_escaped = cast_text
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n");
    let opts_str = format!("{{\"cast_inline\":\"{}\"}}", json_escaped);
    let opts = std::ffi::CString::new(opts_str).unwrap();

    let h = unsafe { bt_mock_tty_create(2, opts.as_ptr()) };
    unsafe {
        bt_mock_tty_set_output_callback(h, Some(record), std::ptr::null_mut());
    }
    drain();

    // At t=0, only the first event releases.
    unsafe {
        bt_mock_tty_tick(h, 0);
    }
    assert_eq!(drain(), b"hi");

    // At t=0.6s, second event releases.
    unsafe {
        bt_mock_tty_tick(h, 600);
    }
    assert_eq!(drain(), b" world\r\n");

    // At t=2s, third event releases.
    unsafe {
        bt_mock_tty_tick(h, 2000);
    }
    assert_eq!(drain(), b"\x1B[?1049h");
    unsafe {
        bt_mock_tty_free(h);
    }
}

#[test]
fn replay_with_no_opts_is_empty() {
    let h = unsafe { bt_mock_tty_create(2, std::ptr::null()) };
    unsafe {
        bt_mock_tty_set_output_callback(h, Some(record), std::ptr::null_mut());
    }
    drain();
    unsafe {
        bt_mock_tty_tick(h, 10_000);
    }
    assert_eq!(drain(), Vec::<u8>::new());
    unsafe {
        bt_mock_tty_free(h);
    }
}
