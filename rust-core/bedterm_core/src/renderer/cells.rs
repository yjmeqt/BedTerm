//! Vertex layouts for the renderer's two pipelines: cell quads (text)
//! and panel quads (Warp-style rounded block backgrounds). Both layouts
//! must match the Metal `struct` definitions in `shaders.rs`.

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
    /// Pad to 64 bytes (16-byte alignment).
    pub _pad: [f32; 3],
}

/// Per-vertex layout for the panel pipeline. Each panel quad is rendered
/// with a signed-distance-function fragment shader that produces an
/// anti-aliased rounded-rectangle alpha mask, giving Warp's iconic
/// rounded block backgrounds without per-pixel CPU work.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct PanelVertex {
    /// Viewport pixel position (top-left origin).
    pub pos_x: f32,
    pub pos_y: f32,
    /// 0..1 within the panel — fragment shader recovers pixel coords as
    /// `local * size` to evaluate the SDF.
    pub local_x: f32,
    pub local_y: f32,
    /// Panel size in pixels — needed by the SDF.
    pub size_x: f32,
    pub size_y: f32,
    /// Premultiplied RGBA bg color.
    pub color: [f32; 4],
    pub corner_radius: f32,
    /// Pad to 48 bytes (16-byte alignment for the float4 color).
    pub _pad: [f32; 3],
}

/// Two triangles per cell.
pub const VERTICES_PER_CELL: usize = 6;

/// Two triangles per panel.
pub const VERTICES_PER_PANEL: usize = 6;
