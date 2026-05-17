use crate::mock_tty::program::{Program, Signal};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Normal,
    Insert,
    Command,
}

pub struct VimLite {
    mode: Mode,
    buffer: Vec<String>,
    cursor_row: usize,
    cursor_col: usize,
    cmd_buf: String,
    status: String,
    last_normal_key: Option<u8>,
    cols: u16,
    rows: u16,
    exit: bool,
}

const WELCOME: &[&str] = &[
    "BedTerm vim-lite",
    "----------------",
    "NORMAL: h j k l 0 $ x dd gg G i a o :",
    "INSERT: <typed text>  ESC to NORMAL",
    "COMMAND: :q :w :wq  ESC to abort",
    "",
];

impl VimLite {
    pub fn new() -> Self {
        Self {
            mode: Mode::Normal,
            buffer: WELCOME.iter().map(|s| s.to_string()).collect(),
            cursor_row: 0,
            cursor_col: 0,
            cmd_buf: String::new(),
            status: String::new(),
            last_normal_key: None,
            cols: 80,
            rows: 24,
            exit: false,
        }
    }

    fn redraw(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(b"\x1B[2J\x1B[H");
        for (i, line) in self.buffer.iter().enumerate() {
            if i + 1 >= self.rows as usize {
                break;
            }
            let _ = std::io::Write::write_fmt(&mut *out, format_args!("{line}\r\n"));
        }
        let status = self.status_line();
        let _ = std::io::Write::write_fmt(
            &mut *out,
            format_args!(
                "\x1B[{};1H\x1B[7m{status:<width$}\x1B[0m",
                self.rows,
                width = self.cols as usize
            ),
        );
        let _ = std::io::Write::write_fmt(
            &mut *out,
            format_args!("\x1B[{};{}H", self.cursor_row + 1, self.cursor_col + 1),
        );
        out.extend_from_slice(match self.mode {
            Mode::Insert => b"\x1B[6 q",
            _ => b"\x1B[2 q",
        });
    }

    fn status_line(&self) -> String {
        match self.mode {
            Mode::Normal => format!("-- NORMAL --  {}", self.status),
            Mode::Insert => "-- INSERT --".to_string(),
            Mode::Command => format!(":{}", self.cmd_buf),
        }
    }

    fn enter_alt_screen(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(b"\x1B[?1049h\x1B[?25h");
    }

    fn leave_alt_screen(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(b"\x1B[?1049l\x1B[0 q");
    }

    fn line_len(&self, row: usize) -> usize {
        self.buffer.get(row).map(|s| s.len()).unwrap_or(0)
    }

    fn clamp_cursor(&mut self) {
        if self.buffer.is_empty() {
            self.buffer.push(String::new());
        }
        self.cursor_row = self.cursor_row.min(self.buffer.len() - 1);
        let max = self.line_len(self.cursor_row);
        if self.cursor_col >= max {
            self.cursor_col = max.saturating_sub(1);
        }
    }

    fn on_normal(&mut self, b: u8) {
        self.status.clear();
        let pending = self.last_normal_key.take();
        match (pending, b) {
            (Some(b'g'), b'g') => {
                self.cursor_row = 0;
                self.cursor_col = 0;
            }
            (Some(b'd'), b'd') => {
                if !self.buffer.is_empty() {
                    self.buffer.remove(self.cursor_row);
                }
                if self.buffer.is_empty() {
                    self.buffer.push(String::new());
                }
                self.clamp_cursor();
            }
            (_, b'g') => self.last_normal_key = Some(b'g'),
            (_, b'd') => self.last_normal_key = Some(b'd'),
            (_, b'h') => {
                if self.cursor_col > 0 {
                    self.cursor_col -= 1;
                }
            }
            (_, b'l') => {
                let max = self.line_len(self.cursor_row);
                if self.cursor_col + 1 < max {
                    self.cursor_col += 1;
                }
            }
            (_, b'j') => {
                if self.cursor_row + 1 < self.buffer.len() {
                    self.cursor_row += 1;
                    self.clamp_cursor();
                }
            }
            (_, b'k') => {
                if self.cursor_row > 0 {
                    self.cursor_row -= 1;
                    self.clamp_cursor();
                }
            }
            (_, b'0') => self.cursor_col = 0,
            (_, b'$') => self.cursor_col = self.line_len(self.cursor_row).saturating_sub(1),
            (_, b'G') => {
                self.cursor_row = self.buffer.len().saturating_sub(1);
                self.cursor_col = 0;
            }
            (_, b'x') => {
                if self.cursor_col < self.line_len(self.cursor_row) {
                    self.buffer[self.cursor_row].remove(self.cursor_col);
                    self.clamp_cursor();
                }
            }
            (_, b'i') => self.mode = Mode::Insert,
            (_, b'a') => {
                self.cursor_col = (self.cursor_col + 1).min(self.line_len(self.cursor_row));
                self.mode = Mode::Insert;
            }
            (_, b'o') => {
                self.buffer.insert(self.cursor_row + 1, String::new());
                self.cursor_row += 1;
                self.cursor_col = 0;
                self.mode = Mode::Insert;
            }
            (_, b':') => {
                self.cmd_buf.clear();
                self.mode = Mode::Command;
            }
            _ => {}
        }
    }

    fn on_insert(&mut self, b: u8) {
        match b {
            0x1B => self.mode = Mode::Normal,
            0x7F | 0x08 => {
                if self.cursor_col > 0 {
                    self.buffer[self.cursor_row].remove(self.cursor_col - 1);
                    self.cursor_col -= 1;
                }
            }
            b'\r' | b'\n' => {
                let tail = self.buffer[self.cursor_row].split_off(self.cursor_col);
                self.buffer.insert(self.cursor_row + 1, tail);
                self.cursor_row += 1;
                self.cursor_col = 0;
            }
            0x20..=0x7E => {
                let row = &mut self.buffer[self.cursor_row];
                row.insert(self.cursor_col, b as char);
                self.cursor_col += 1;
            }
            _ => {}
        }
    }

    fn on_command(&mut self, b: u8, out: &mut Vec<u8>) {
        match b {
            0x1B => {
                self.mode = Mode::Normal;
                self.cmd_buf.clear();
            }
            b'\r' | b'\n' => {
                match self.cmd_buf.as_str() {
                    "q" | "q!" => {
                        self.leave_alt_screen(out);
                        self.exit = true;
                    }
                    "w" => {
                        self.status = "written".into();
                        self.mode = Mode::Normal;
                    }
                    "wq" => {
                        self.status = "written".into();
                        self.leave_alt_screen(out);
                        self.exit = true;
                    }
                    other => {
                        self.status = format!("E:{other}");
                        self.mode = Mode::Normal;
                    }
                }
                self.cmd_buf.clear();
            }
            0x7F | 0x08 => {
                self.cmd_buf.pop();
            }
            0x20..=0x7E => self.cmd_buf.push(b as char),
            _ => {}
        }
    }
}

impl Default for VimLite {
    fn default() -> Self {
        Self::new()
    }
}

impl Program for VimLite {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn wants_raw(&self) -> bool {
        true
    }

    fn boot(&mut self, out: &mut Vec<u8>) {
        self.enter_alt_screen(out);
        self.redraw(out);
    }

    fn on_byte(&mut self, b: u8, out: &mut Vec<u8>) {
        if self.exit {
            return;
        }
        match self.mode {
            Mode::Normal => self.on_normal(b),
            Mode::Insert => self.on_insert(b),
            Mode::Command => self.on_command(b, out),
        }
        if self.exit {
            return;
        }
        self.redraw(out);
    }

    fn on_resize(&mut self, cols: u16, rows: u16, out: &mut Vec<u8>) {
        self.cols = cols.max(20);
        self.rows = rows.max(5);
        self.clamp_cursor();
        self.redraw(out);
    }

    fn on_signal(&mut self, _sig: Signal, _out: &mut Vec<u8>) {}
}
