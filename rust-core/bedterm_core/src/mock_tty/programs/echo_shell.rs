use crate::mock_tty::program::{Program, Signal, TermiosMode};

const PROMPT: &[u8] = b"bedterm-debug$ ";

pub struct EchoShell {
    keys_mode: bool,
    alt_until_ms: Option<u64>,
    stress_until_ms: Option<u64>,
    stress_rng: u32,
    pending_mode: Option<TermiosMode>,
}

impl EchoShell {
    pub fn new() -> Self {
        Self {
            keys_mode: false,
            alt_until_ms: None,
            stress_until_ms: None,
            stress_rng: 0x1234_5678,
            pending_mode: None,
        }
    }

    fn prompt(out: &mut Vec<u8>) {
        out.extend_from_slice(PROMPT);
    }

    fn run_colors(out: &mut Vec<u8>) {
        // 16-color row
        for c in 30..=37u8 {
            let _ = std::io::Write::write_fmt(&mut *out, format_args!("\x1B[{c}m  X  "));
        }
        out.extend_from_slice(b"\x1B[0m\r\n");
        // 256-color block
        for n in 16..32u8 {
            let _ = std::io::Write::write_fmt(&mut *out, format_args!("\x1B[38;5;{n}m##"));
        }
        out.extend_from_slice(b"\x1B[0m\r\n");
        // truecolor gradient
        for i in 0..32u8 {
            let r = i * 8;
            let g = 255 - r;
            let _ = std::io::Write::write_fmt(&mut *out, format_args!("\x1B[38;2;{r};{g};0m**"));
        }
        out.extend_from_slice(b"\x1B[0m\r\n");
    }

    fn run_cursor(out: &mut Vec<u8>) {
        out.extend_from_slice(b"\x1B[2J\x1B[H");
        for row in 1..=8u8 {
            for col in 1..=16u8 {
                let ch = if (row + col) % 2 == 0 { "#" } else { "." };
                let _ = std::io::Write::write_fmt(&mut *out, format_args!("\x1B[{row};{col}H{ch}"));
            }
        }
        out.extend_from_slice(b"\x1B[10;1H");
    }

    fn enter_altscreen(&mut self, out: &mut Vec<u8>) {
        out.extend_from_slice(b"\x1B[?1049h\x1B[H\x1B[2JAlt-screen demo. Returning in 1.5s.\r\n");
        self.alt_until_ms = Some(u64::MAX);
    }

    fn stress_step(&mut self, out: &mut Vec<u8>) {
        let mut x = self.stress_rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.stress_rng = x;
        let row = ((x >> 8) % 24) as u8 + 1;
        let col = ((x >> 16) % 80) as u8 + 1;
        let r = (x & 0xFF) as u8;
        let g = ((x >> 8) & 0xFF) as u8;
        let b = ((x >> 16) & 0xFF) as u8;
        let _ = std::io::Write::write_fmt(
            &mut *out,
            format_args!("\x1B[{row};{col}H\x1B[38;2;{r};{g};{b}m#"),
        );
    }
}

impl Default for EchoShell {
    fn default() -> Self {
        Self::new()
    }
}

impl Program for EchoShell {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn wants_raw(&self) -> bool {
        false
    }

    fn boot(&mut self, out: &mut Vec<u8>) {
        out.extend_from_slice(b"BedTerm debug shell. Type a command.\r\n");
        Self::prompt(out);
    }

    fn on_byte(&mut self, b: u8, out: &mut Vec<u8>) {
        if self.keys_mode {
            if b == b'q' {
                self.keys_mode = false;
                self.pending_mode = Some(TermiosMode::Cooked);
                out.extend_from_slice(b"\r\n[keys] exit\r\n");
                Self::prompt(out);
            } else {
                let mnem = mnemonic(b);
                let _ = std::io::Write::write_fmt(&mut *out, format_args!("0x{b:02X}  {mnem}\r\n"));
            }
        }
    }

    fn on_line(&mut self, line: &[u8], out: &mut Vec<u8>) {
        if self.keys_mode {
            return;
        }
        let cmd = std::str::from_utf8(line).unwrap_or("").trim();
        match cmd {
            "" => {}
            "clear" => out.extend_from_slice(b"\x1B[2J\x1B[H"),
            "exit" => out.extend_from_slice(b"bye\r\n"),
            "colors" => Self::run_colors(out),
            "cursor" => Self::run_cursor(out),
            "altscreen" => self.enter_altscreen(out),
            "stress" => self.stress_until_ms = Some(u64::MAX),
            "keys" => {
                self.keys_mode = true;
                self.pending_mode = Some(TermiosMode::Raw);
                out.extend_from_slice(b"[keys] press keys; 'q' to exit\r\n");
                return;
            }
            other => {
                let _ =
                    std::io::Write::write_fmt(&mut *out, format_args!("{other}: not found\r\n"));
            }
        }
        Self::prompt(out);
    }

    fn on_tick(&mut self, now_ms: u64, out: &mut Vec<u8>) {
        if let Some(deadline) = self.alt_until_ms {
            if deadline == u64::MAX {
                self.alt_until_ms = Some(now_ms.saturating_add(1500));
            } else if now_ms >= deadline {
                out.extend_from_slice(b"\x1B[?1049l");
                self.alt_until_ms = None;
                Self::prompt(out);
            }
        }
        if let Some(deadline) = self.stress_until_ms {
            if deadline == u64::MAX {
                self.stress_until_ms = Some(now_ms.saturating_add(3000));
            } else if now_ms >= deadline {
                self.stress_until_ms = None;
                out.extend_from_slice(b"\x1B[0m\r\n");
                Self::prompt(out);
            } else {
                for _ in 0..16 {
                    self.stress_step(out);
                }
            }
        }
    }

    fn on_signal(&mut self, sig: Signal, out: &mut Vec<u8>) {
        match sig {
            Signal::Int => {
                self.alt_until_ms = None;
                self.stress_until_ms = None;
                if self.keys_mode {
                    self.keys_mode = false;
                    self.pending_mode = Some(TermiosMode::Cooked);
                }
                out.extend_from_slice(b"^C\r\n");
                Self::prompt(out);
            }
            Signal::Eof => {
                out.extend_from_slice(b"\r\n");
            }
        }
    }

    fn mode_request(&mut self) -> Option<TermiosMode> {
        self.pending_mode.take()
    }
}

fn mnemonic(b: u8) -> &'static str {
    match b {
        0x1B => "ESC",
        0x0D => "CR",
        0x0A => "LF",
        0x09 => "TAB",
        0x7F => "DEL",
        0x08 => "BS",
        0x03 => "^C",
        0x04 => "^D",
        0x20 => "SP",
        _ => "",
    }
}
