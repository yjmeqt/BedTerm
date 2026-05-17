//! Mock-TTY program trait. Each program is a self-contained byte source.

pub trait Program: Send + std::any::Any {
    fn as_any(&self) -> &dyn std::any::Any;

    fn on_byte(&mut self, _byte: u8, _out: &mut Vec<u8>) {}

    fn on_line(&mut self, line: &[u8], out: &mut Vec<u8>) {
        for &b in line {
            self.on_byte(b, out);
        }
    }

    fn on_resize(&mut self, _cols: u16, _rows: u16, _out: &mut Vec<u8>) {}
    fn on_tick(&mut self, _now_ms: u64, _out: &mut Vec<u8>) {}
    fn on_signal(&mut self, _sig: Signal, _out: &mut Vec<u8>) {}

    fn boot(&mut self, _out: &mut Vec<u8>) {}
    fn wants_raw(&self) -> bool {
        false
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
    Int,
    Eof,
}
