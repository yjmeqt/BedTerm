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
    /// 1.0 if the sampled glyph is a color bitmap (Apple Color Emoji), 0.0
    /// otherwise. The fragment shader uses this to either composite the
    /// glyph RGBA directly (color path) or tint the alpha mask by `fg`
    /// (monochrome path). A float (not bool) so the layout stays plain
    /// `repr(C)` floats and matches the Metal struct layout.
    pub is_color: f32,
    /// Pad to 64 bytes (16-byte alignment). Metal rounds the struct
    /// stride up to the alignment of its largest member (`float4` = 16
    /// bytes), so the Rust packed layout (52 bytes) would otherwise be
    /// read at the wrong stride on the GPU side. Three trailing floats
    /// of explicit padding match the implicit Metal padding.
    pub _pad: [f32; 3],
}

/// Two triangles per cell.
pub const VERTICES_PER_CELL: usize = 6;
