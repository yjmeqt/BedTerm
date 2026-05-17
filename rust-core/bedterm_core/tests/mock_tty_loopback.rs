#![cfg(feature = "mock-tty")]
//! End-to-end: pipe MockTty output through alacritty_terminal::Term and assert
//! the resulting grid. This is the first end-to-end coverage of term.rs +
//! alacritty integration.

use std::sync::Mutex;

use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::Config;
use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};
use alacritty_terminal::Term;

use bedterm_core::mock_tty::ffi::{
    bt_mock_tty_create, bt_mock_tty_free, bt_mock_tty_set_output_callback, bt_mock_tty_write,
};

#[derive(Clone, Copy)]
struct Dims {
    cols: u16,
    rows: u16,
}
impl Dimensions for Dims {
    fn columns(&self) -> usize {
        self.cols as usize
    }
    fn screen_lines(&self) -> usize {
        self.rows as usize
    }
    fn total_lines(&self) -> usize {
        self.rows as usize
    }
}

static SINK: Mutex<Vec<u8>> = Mutex::new(Vec::new());
unsafe extern "C" fn cb(b: *const u8, n: usize, _u: *mut std::ffi::c_void) {
    SINK.lock()
        .unwrap()
        .extend_from_slice(std::slice::from_raw_parts(b, n));
}

/// Drive a program with the given input, return the alacritty Term that has
/// consumed the program's output stream.
fn drive(program: u32, scripted_input: &[u8]) -> Term<VoidListener> {
    SINK.lock().unwrap().clear();
    let h = unsafe { bt_mock_tty_create(program, std::ptr::null()) };
    unsafe {
        bt_mock_tty_set_output_callback(h, Some(cb), std::ptr::null_mut());
    }
    if !scripted_input.is_empty() {
        unsafe {
            bt_mock_tty_write(h, scripted_input.as_ptr(), scripted_input.len());
        }
    }
    let bytes = std::mem::take(&mut *SINK.lock().unwrap());
    unsafe {
        bt_mock_tty_free(h);
    }

    let dim = Dims { cols: 80, rows: 24 };
    let mut term = Term::new(Config::default(), &dim, VoidListener);
    let mut parser: Processor<StdSyncHandler> = Processor::new();
    parser.advance(&mut term, &bytes);
    term
}

/// Read a row of the grid as a String.
fn row(term: &Term<VoidListener>, line: usize, cols: usize) -> String {
    (0..cols)
        .map(|c| term.grid()[Line(line as i32)][Column(c)].c)
        .collect()
}

#[test]
fn loopback_echo_shell_prompt_lands_on_row_one() {
    let term = drive(0, b"");
    let line0 = row(&term, 0, 80);
    assert!(line0.contains("BedTerm debug shell"), "got `{line0}`");
    let line1 = row(&term, 1, 16);
    assert!(line1.starts_with("bedterm-debug$"), "got `{line1}`");
}

#[test]
fn loopback_clear_resets_grid_to_prompt_at_top() {
    let term = drive(0, b"clear\r");
    let line0 = row(&term, 0, 16);
    assert!(line0.starts_with("bedterm-debug$"), "got `{line0}`");
}

#[test]
fn loopback_colors_paints_truecolor_cells() {
    let term = drive(0, b"colors\r");
    use alacritty_terminal::vte::ansi::Color;
    let mut any_truecolor = false;
    'outer: for r in 0..5 {
        for c in 0..80 {
            let cell = &term.grid()[Line(r)][Column(c)];
            if matches!(cell.fg, Color::Spec(_)) {
                any_truecolor = true;
                break 'outer;
            }
        }
    }
    assert!(
        any_truecolor,
        "expected at least one truecolor cell in colors output"
    );
}

#[test]
fn loopback_vim_lite_welcome_buffer_is_visible() {
    let term = drive(1, b"");
    // Inside alt-screen: line 0 should have "BedTerm vim-lite".
    let line0 = row(&term, 0, 16);
    assert!(line0.contains("BedTerm vim-lite"), "got `{line0}`");
}
