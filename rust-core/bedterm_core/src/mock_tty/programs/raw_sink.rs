use crate::mock_tty::program::Program;

pub struct RawSink;

impl RawSink {
    pub fn new() -> Self {
        Self
    }
}

impl Default for RawSink {
    fn default() -> Self {
        Self::new()
    }
}

impl Program for RawSink {
    fn on_byte(&mut self, byte: u8, out: &mut Vec<u8>) {
        out.push(byte);
    }
    fn wants_raw(&self) -> bool {
        true
    }
}
