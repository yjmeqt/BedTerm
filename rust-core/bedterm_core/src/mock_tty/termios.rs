//! Termios state machine. Filled in by Task 2; placeholder for now.

use crate::mock_tty::program::Program;

pub struct Termios {
    raw: bool,
}

impl Termios {
    pub fn default_cooked() -> Self {
        Self { raw: false }
    }

    pub fn input_byte(&mut self, b: u8, program: &mut Box<dyn Program>, out: &mut Vec<u8>) {
        program.on_byte(b, out);
        let _ = self.raw;
    }
}
