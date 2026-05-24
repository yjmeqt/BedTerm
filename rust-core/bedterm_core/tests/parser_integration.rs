use bedterm_core::snapshot::{CellSnapshot, GridSnapshot};

#[test]
fn empty_grid_has_zero_cells() {
    let snap = GridSnapshot {
        cols: 0,
        rows: 0,
        cursor_col: 0,
        cursor_row: 0,
        display_offset: 0,
        cells: Vec::new(),
    };
    assert_eq!(snap.cells.len(), 0);
    assert_eq!(snap.cols, 0);
}

#[test]
fn cell_packs_rgba_fg_bg() {
    let c = CellSnapshot {
        ch: 'A' as u32,
        fg_rgba: 0xFFFFFFFF,
        bg_rgba: 0x000000FF,
        flags: 0,
    };
    assert_eq!(c.ch, 65);
    assert_eq!(c.fg_rgba, 0xFFFFFFFF);
}

use bedterm_core::term::Terminal;

#[test]
fn feeding_ascii_populates_cells() {
    let mut t = Terminal::new(20, 5);
    t.feed(b"hello");
    let snap = t.snapshot();
    assert_eq!(snap.cols, 20);
    assert_eq!(snap.rows, 5);
    assert_eq!(snap.cell(0, 0).unwrap().ch, 'h' as u32);
    assert_eq!(snap.cell(4, 0).unwrap().ch, 'o' as u32);
    assert_eq!(snap.cursor_col, 5);
    assert_eq!(snap.cursor_row, 0);
}

#[test]
fn ansi_color_sets_fg() {
    let mut t = Terminal::new(20, 5);
    t.feed(b"\x1b[31mR\x1b[0m");
    let cell = *t.snapshot().cell(0, 0).unwrap();
    assert_eq!(cell.ch, 'R' as u32);
    let r = (cell.fg_rgba >> 24) & 0xff;
    let g = (cell.fg_rgba >> 16) & 0xff;
    let b = (cell.fg_rgba >> 8) & 0xff;
    assert!(r > 0x80, "red channel expected > 0x80, got 0x{:x}", r);
    assert!(g < 0x40, "green should be low, got 0x{:x}", g);
    assert!(b < 0x40, "blue should be low, got 0x{:x}", b);
}

#[test]
fn resize_updates_dimensions() {
    let mut t = Terminal::new(20, 5);
    t.resize(40, 10);
    let snap = t.snapshot();
    assert_eq!(snap.cols, 40);
    assert_eq!(snap.rows, 10);
}

#[test]
fn ffi_roundtrip() {
    use bedterm_core::ffi::*;
    unsafe {
        let h = bt_term_new(20, 5);
        assert!(!h.is_null());
        let bytes = b"hi";
        bt_term_feed(h, bytes.as_ptr(), bytes.len());
        let mut out = std::mem::MaybeUninit::<BtSnapshotView>::uninit();
        let ok = bt_term_snapshot(h, out.as_mut_ptr());
        assert_eq!(ok, 0);
        let view = out.assume_init();
        assert_eq!(view.cols, 20);
        assert_eq!(view.rows, 5);
        let cell0 = *view.cells.add(0);
        assert_eq!(cell0.ch, 'h' as u32);
        bt_term_snapshot_release(h);
        bt_term_free(h);
    }
}
