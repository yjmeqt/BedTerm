#![cfg(feature = "mock-tty")]
use std::sync::Mutex;

use bedterm_core::mock_tty::ffi::{
    bt_mock_tty_create, bt_mock_tty_free, bt_mock_tty_set_output_callback, bt_mock_tty_write,
};

static SINK: Mutex<Vec<u8>> = Mutex::new(Vec::new());

unsafe extern "C" fn record(bytes: *const u8, len: usize, _ud: *mut std::ffi::c_void) {
    let slice = std::slice::from_raw_parts(bytes, len);
    SINK.lock().unwrap().extend_from_slice(slice);
}

#[test]
fn raw_sink_echoes_every_input_byte() {
    SINK.lock().unwrap().clear();
    let h = unsafe { bt_mock_tty_create(3, std::ptr::null()) };
    unsafe {
        bt_mock_tty_set_output_callback(h, Some(record), std::ptr::null_mut());
    }
    let bytes = b"\x1B[A\x03hi\r\n";
    unsafe {
        bt_mock_tty_write(h, bytes.as_ptr(), bytes.len());
    }
    unsafe { bt_mock_tty_free(h) };
    assert_eq!(SINK.lock().unwrap().as_slice(), bytes);
}
