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
    let mut s = SINK.lock().unwrap();
    std::mem::take(&mut *s)
}

fn make() -> *mut bedterm_core::mock_tty::ffi::BtMockTty {
    drain();
    let h = unsafe { bt_mock_tty_create(0, std::ptr::null()) };
    unsafe {
        bt_mock_tty_set_output_callback(h, Some(record), std::ptr::null_mut());
    }
    h
}

#[test]
fn echo_shell_emits_prompt_on_boot() {
    let h = make();
    let out = drain();
    assert!(
        out.windows(b"bedterm-debug$ ".len())
            .any(|w| w == b"bedterm-debug$ "),
        "expected prompt, got {:?}",
        String::from_utf8_lossy(&out)
    );
    unsafe {
        bt_mock_tty_free(h);
    }
}

#[test]
fn echo_shell_unknown_command_prints_not_found_and_reprompts() {
    let h = make();
    drain();
    let cmd = b"frobnicate\r";
    unsafe {
        bt_mock_tty_write(h, cmd.as_ptr(), cmd.len());
    }
    let out = drain();
    let s = String::from_utf8_lossy(&out);
    assert!(s.contains("frobnicate: not found"), "got {s}");
    assert!(s.contains("bedterm-debug$ "), "no reprompt: {s}");
    unsafe {
        bt_mock_tty_free(h);
    }
}

#[test]
fn echo_shell_clear_emits_2j_and_cursor_home() {
    let h = make();
    drain();
    let cmd = b"clear\r";
    unsafe {
        bt_mock_tty_write(h, cmd.as_ptr(), cmd.len());
    }
    let out = drain();
    assert!(out.windows(4).any(|w| w == b"\x1B[2J"), "no ED2: {out:?}");
    assert!(
        out.windows(3).any(|w| w == b"\x1B[H"),
        "no CUP home: {out:?}"
    );
    unsafe {
        bt_mock_tty_free(h);
    }
}

#[test]
fn echo_shell_colors_emits_truecolor_sgr() {
    let h = make();
    drain();
    let cmd = b"colors\r";
    unsafe {
        bt_mock_tty_write(h, cmd.as_ptr(), cmd.len());
    }
    let s = String::from_utf8_lossy(&drain()).to_string();
    assert!(s.contains("\x1B[38;2;"), "missing truecolor SGR");
    assert!(s.contains("\x1B[38;5;"), "missing 256-color SGR");
    assert!(
        s.contains("\x1B[31m") || s.contains("\x1B[41m") || s.contains("\x1B[32m"),
        "missing 16-color SGR"
    );
    unsafe {
        bt_mock_tty_free(h);
    }
}

#[test]
fn echo_shell_altscreen_enters_1049() {
    let h = make();
    drain();
    let cmd = b"altscreen\r";
    unsafe {
        bt_mock_tty_write(h, cmd.as_ptr(), cmd.len());
    }
    let s = String::from_utf8_lossy(&drain()).to_string();
    assert!(s.contains("\x1B[?1049h"), "no alt-screen enter");
    unsafe {
        bt_mock_tty_free(h);
    }
}

#[test]
fn echo_shell_keys_sub_mode_prints_hex_until_q() {
    let h = make();
    drain();
    let enter = b"keys\r";
    unsafe {
        bt_mock_tty_write(h, enter.as_ptr(), enter.len());
    }
    drain();
    let probe = b"\x1B[A"; // up arrow: ESC '[' 'A'
    unsafe {
        bt_mock_tty_write(h, probe.as_ptr(), probe.len());
    }
    let s = String::from_utf8_lossy(&drain()).to_string();
    assert!(s.contains("0x1B"), "missing ESC dump: {s}");
    assert!(s.contains("0x5B"), "missing [ dump: {s}");
    assert!(s.contains("0x41"), "missing A dump: {s}");
    let exit = b"q";
    unsafe {
        bt_mock_tty_write(h, exit.as_ptr(), exit.len());
    }
    let s = String::from_utf8_lossy(&drain()).to_string();
    assert!(
        s.contains("bedterm-debug$ "),
        "no reprompt after keys exit: {s}"
    );
    unsafe {
        bt_mock_tty_free(h);
    }
}

#[test]
fn echo_shell_demo_vim_switches_to_replay() {
    use bedterm_core::mock_tty::ffi::bt_mock_tty_tick;
    let h = make();
    drain();
    let cmd = b"demo vim\r";
    unsafe {
        bt_mock_tty_write(h, cmd.as_ptr(), cmd.len());
    }
    drain();
    unsafe {
        bt_mock_tty_tick(h, 10_000);
    }
    let s = drain();
    assert!(
        !s.is_empty(),
        "expected replay output after demo vim + tick"
    );
    assert!(
        s.windows(8).any(|w| w == b"\x1B[?1049h"),
        "expected alt-screen enter from vim cast: {s:?}"
    );
    unsafe {
        bt_mock_tty_free(h);
    }
}
