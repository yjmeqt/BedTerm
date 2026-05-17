//! Debug-only mock TTY. Feature-gated; absent in Release builds.

pub mod ffi;
pub mod program;
pub mod programs;
pub mod termios;

use crate::mock_tty::program::Program;
use crate::mock_tty::termios::Termios;

pub struct MockTty {
    pub(crate) program: Box<dyn Program>,
    termios: Termios,
    pub(crate) output: Vec<u8>,
    callback: Option<OutputCallback>,
    #[cfg(debug_assertions)]
    in_callback: std::cell::Cell<bool>,
}

#[derive(Clone, Copy)]
pub(crate) struct OutputCallback {
    pub func: unsafe extern "C" fn(*const u8, usize, *mut std::ffi::c_void),
    pub user_data: *mut std::ffi::c_void,
}

unsafe impl Send for OutputCallback {}

impl MockTty {
    pub fn new(program: Box<dyn Program>) -> Self {
        let termios = Termios::default_cooked();
        Self {
            program,
            termios,
            output: Vec::with_capacity(4096),
            callback: None,
            #[cfg(debug_assertions)]
            in_callback: std::cell::Cell::new(false),
        }
    }

    pub fn write_input(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.termios.input_byte(b, &mut self.program, &mut self.output);
        }
        self.flush();
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.program.on_resize(cols, rows, &mut self.output);
        self.flush();
    }

    pub fn tick(&mut self, now_ms: u64) {
        self.program.on_tick(now_ms, &mut self.output);
        self.flush();
    }

    pub(crate) fn set_callback(&mut self, cb: Option<OutputCallback>) {
        self.callback = cb;
    }

    pub(crate) fn flush(&mut self) {
        if self.output.is_empty() {
            return;
        }
        if let Some(cb) = self.callback {
            #[cfg(debug_assertions)]
            {
                assert!(
                    !self.in_callback.get(),
                    "bt_mock_tty: reentered handle from inside output callback"
                );
                self.in_callback.set(true);
            }
            unsafe { (cb.func)(self.output.as_ptr(), self.output.len(), cb.user_data); }
            #[cfg(debug_assertions)]
            self.in_callback.set(false);
        }
        self.output.clear();
    }
}
