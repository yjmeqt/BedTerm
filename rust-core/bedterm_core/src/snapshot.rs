//! Flat, C-friendly grid snapshot. Held by Rust, pointed at from Swift via FFI.

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct CellSnapshot {
    /// Unicode scalar of the cell. 0 = blank. Wide-char trailing halves use char 0 with WIDE_TRAILING flag.
    pub ch: u32,
    /// Foreground colour, RGBA packed big-endian-style: 0xRRGGBBAA.
    pub fg_rgba: u32,
    /// Background colour, same encoding.
    pub bg_rgba: u32,
    /// Bitfield: 1=bold, 2=underline, 4=inverse, 8=italic, 16=wide_leading, 32=wide_trailing.
    pub flags: u16,
}

#[derive(Debug, Clone)]
pub struct GridSnapshot {
    pub cols: u16,
    pub rows: u16,
    pub cursor_col: u16,
    pub cursor_row: u16,
    pub cells: Vec<CellSnapshot>,
}

impl GridSnapshot {
    pub fn cell(&self, col: u16, row: u16) -> Option<&CellSnapshot> {
        if col >= self.cols || row >= self.rows {
            return None;
        }
        self.cells
            .get(row as usize * self.cols as usize + col as usize)
    }
}
