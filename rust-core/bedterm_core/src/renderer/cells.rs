//! Vertex layout for the cell-quad shader. Layout must match the Metal
//! `CellVertex` struct in `shaders.rs`.

#[repr(C)]
#[derive(Copy, Clone)]
pub struct CellVertex {
    pub pos_x: f32,
    pub pos_y: f32,
    pub uv_x: f32,
    pub uv_y: f32,
    pub fg: [f32; 4],
    pub bg: [f32; 4],
}

/// Two triangles per cell.
pub const VERTICES_PER_CELL: usize = 6;
