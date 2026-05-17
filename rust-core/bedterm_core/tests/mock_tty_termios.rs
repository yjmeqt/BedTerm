#![cfg(feature = "mock-tty")]
use bedterm_core::mock_tty::program::{Program, Signal};
use bedterm_core::mock_tty::termios::Termios;

#[derive(Default)]
struct Recorder {
    lines: Vec<Vec<u8>>,
    bytes: Vec<u8>,
    signals: Vec<Signal>,
}

impl Program for Recorder {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn on_byte(&mut self, b: u8, _out: &mut Vec<u8>) {
        self.bytes.push(b);
    }
    fn on_line(&mut self, line: &[u8], _out: &mut Vec<u8>) {
        self.lines.push(line.to_vec());
    }
    fn on_signal(&mut self, s: Signal, _out: &mut Vec<u8>) {
        self.signals.push(s);
    }
}

fn feed(tm: &mut Termios, prog: &mut Box<dyn Program>, bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for &b in bytes {
        tm.input_byte(b, prog, &mut out);
    }
    out
}

#[test]
fn cooked_mode_buffers_until_cr_and_echoes_with_crlf() {
    let mut tm = Termios::default_cooked();
    let mut prog: Box<dyn Program> = Box::new(Recorder::default());
    let echo = feed(&mut tm, &mut prog, b"hi\r");
    assert_eq!(echo, b"hi\r\n");
    let rec = prog.as_any().downcast_ref::<Recorder>().unwrap();
    assert_eq!(rec.lines, vec![b"hi".to_vec()]);
}

#[test]
fn backspace_edits_line_buffer_with_echo() {
    let mut tm = Termios::default_cooked();
    let mut prog: Box<dyn Program> = Box::new(Recorder::default());
    let echo = feed(&mut tm, &mut prog, b"ab\x7Fc\r");
    assert_eq!(echo, b"ab\x08 \x08c\r\n");
    let rec = prog.as_any().downcast_ref::<Recorder>().unwrap();
    assert_eq!(rec.lines, vec![b"ac".to_vec()]);
}

#[test]
fn ctrl_c_raises_sigint_and_clears_line_buffer() {
    let mut tm = Termios::default_cooked();
    let mut prog: Box<dyn Program> = Box::new(Recorder::default());
    feed(&mut tm, &mut prog, b"foo\x03bar\r");
    let rec = prog.as_any().downcast_ref::<Recorder>().unwrap();
    assert_eq!(rec.signals, vec![Signal::Int]);
    assert_eq!(rec.lines, vec![b"bar".to_vec()]);
}

#[test]
fn raw_mode_forwards_every_byte_unchanged() {
    let mut tm = Termios::default_cooked();
    tm.set_raw();
    let mut prog: Box<dyn Program> = Box::new(Recorder::default());
    let echo = feed(&mut tm, &mut prog, b"a\r\x03b");
    assert!(echo.is_empty(), "raw mode never echoes");
    let rec = prog.as_any().downcast_ref::<Recorder>().unwrap();
    assert_eq!(rec.bytes, b"a\r\x03b");
    assert!(rec.signals.is_empty());
}

#[test]
fn eof_at_line_start_emits_eof_signal() {
    let mut tm = Termios::default_cooked();
    let mut prog: Box<dyn Program> = Box::new(Recorder::default());
    feed(&mut tm, &mut prog, b"\x04");
    let rec = prog.as_any().downcast_ref::<Recorder>().unwrap();
    assert_eq!(rec.signals, vec![Signal::Eof]);
}
