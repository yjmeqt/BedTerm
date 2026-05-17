use bedterm_core::snapshot::{CellSnapshot, GridSnapshot};

#[test]
fn empty_grid_has_zero_cells() {
    let snap = GridSnapshot { cols: 0, rows: 0, cursor_col: 0, cursor_row: 0, cells: Vec::new() };
    assert_eq!(snap.cells.len(), 0);
    assert_eq!(snap.cols, 0);
}

#[test]
fn cell_packs_rgba_fg_bg() {
    let c = CellSnapshot { ch: 'A' as u32, fg_rgba: 0xFFFFFFFF, bg_rgba: 0x000000FF, flags: 0 };
    assert_eq!(c.ch, 65);
    assert_eq!(c.fg_rgba, 0xFFFFFFFF);
}
