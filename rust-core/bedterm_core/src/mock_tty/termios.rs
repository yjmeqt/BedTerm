//! Minimal termios line discipline: ICANON / ECHO / ECHOE / ISIG / ICRNL / ONLCR.

use crate::mock_tty::program::{Program, Signal};

pub const VINTR: u8 = 0x03;
pub const VEOF: u8 = 0x04;
pub const VERASE: u8 = 0x7F;

#[derive(Clone, Copy, Debug)]
pub struct Flags {
    pub icanon: bool,
    pub echo: bool,
    pub echoe: bool,
    pub isig: bool,
    pub icrnl: bool,
    pub onlcr: bool,
}

impl Flags {
    pub const COOKED: Flags = Flags {
        icanon: true,
        echo: true,
        echoe: true,
        isig: true,
        icrnl: true,
        onlcr: true,
    };
    pub const RAW: Flags = Flags {
        icanon: false,
        echo: false,
        echoe: false,
        isig: false,
        icrnl: false,
        onlcr: false,
    };
}

pub struct Termios {
    flags: Flags,
    line: Vec<u8>,
}

impl Termios {
    pub fn default_cooked() -> Self {
        Self {
            flags: Flags::COOKED,
            line: Vec::with_capacity(128),
        }
    }
    pub fn set_raw(&mut self) {
        self.flags = Flags::RAW;
        self.line.clear();
    }
    pub fn set_cooked(&mut self) {
        self.flags = Flags::COOKED;
    }

    pub fn input_byte(&mut self, b: u8, program: &mut Box<dyn Program>, out: &mut Vec<u8>) {
        if self.flags.isig && b == VINTR {
            self.line.clear();
            program.on_signal(Signal::Int, out);
            return;
        }

        if !self.flags.icanon {
            program.on_byte(b, out);
            return;
        }

        match b {
            VEOF if self.line.is_empty() => program.on_signal(Signal::Eof, out),
            VERASE => {
                if self.line.pop().is_some() && self.flags.echo && self.flags.echoe {
                    out.extend_from_slice(b"\x08 \x08");
                }
            }
            b'\r' | b'\n' => {
                if self.flags.echo {
                    if self.flags.onlcr {
                        out.extend_from_slice(b"\r\n");
                    } else {
                        out.push(b);
                    }
                }
                let line = std::mem::take(&mut self.line);
                program.on_line(&line, out);
            }
            // Ctrl-U — kill line
            0x15 => {
                if self.flags.echo {
                    for _ in 0..self.line.len() {
                        out.extend_from_slice(b"\x08 \x08");
                    }
                }
                self.line.clear();
            }
            _ => {
                self.line.push(b);
                if self.flags.echo {
                    out.push(b);
                }
            }
        }
    }
}
