//! Metal renderer.

pub mod atlas;
pub mod block_list_ffi;
pub mod cells;
pub mod ffi;
pub(crate) mod font_system;
pub(crate) mod glyph_raster;
pub mod pipeline;
pub mod shaders;

use atlas::GlyphAtlas;
use cells::{CellVertex, VERTICES_PER_CELL};

use crate::snapshot::CellSnapshot;
use metal::foreign_types::ForeignType;
use metal::{
    CommandQueue, Device, MTLClearColor, MTLLoadAction, MTLPrimitiveType, MTLResourceOptions,
    MTLStoreAction, RenderPassDescriptor, Texture,
};
use pipeline::Pipelines;

pub struct Renderer {
    pub(crate) device: Device,
    pub(crate) queue: CommandQueue,
    pub(crate) pipelines: Pipelines,
    pub(crate) atlas: GlyphAtlas,
    pub(crate) pixel_size: f32,
    pub(crate) dpr: f32,
    pub(crate) clear_color: [f32; 4],
}

impl Renderer {
    /// Build a renderer from raw `id<MTLDevice>` / `id<MTLCommandQueue>` pointers.
    ///
    /// # Safety
    /// `device_ptr` and `queue_ptr` must be non-null and point to live ObjC
    /// objects of the respective Metal protocols. Ownership of one retain
    /// each is transferred to the renderer — Swift must pass +1 retained
    /// pointers (e.g. via `Unmanaged.passRetained(...).toOpaque()`).
    pub unsafe fn from_ptrs(
        device_ptr: *const std::ffi::c_void,
        queue_ptr: *const std::ffi::c_void,
    ) -> Option<Self> {
        if device_ptr.is_null() || queue_ptr.is_null() {
            return None;
        }
        let device = Device::from_ptr(device_ptr as *mut _);
        let queue = CommandQueue::from_ptr(queue_ptr as *mut _);
        let pipelines = Pipelines::build(&device).ok()?;
        // Catch panics from the cosmic-text/swash atlas init so the FFI
        // boundary never unwinds into Swift (which would abort the
        // process). Returning None lets Swift surface a sensible error
        // instead of dying mid-view-creation.
        let atlas = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            GlyphAtlas::new(&device, 14.0, 3.0)
        })) {
            Ok(a) => a,
            Err(panic) => {
                let msg = panic
                    .downcast_ref::<&'static str>()
                    .copied()
                    .or_else(|| panic.downcast_ref::<String>().map(|s| s.as_str()))
                    .unwrap_or("<non-string panic payload>");
                eprintln!("[bedterm] GlyphAtlas::new panicked: {msg}");
                return None;
            }
        };
        Some(Self {
            device,
            queue,
            pipelines,
            atlas,
            pixel_size: 14.0,
            dpr: 3.0,
            clear_color: [0.0, 0.0, 0.0, 1.0],
        })
    }

    pub fn set_clear_color(&mut self, r: f32, g: f32, b: f32, a: f32) {
        self.clear_color = [r, g, b, a];
    }

    pub fn set_font(&mut self, pixel_size: f32, dpr: f32) {
        self.pixel_size = pixel_size.max(1.0);
        self.dpr = dpr.max(1.0);
        // Rebuild the atlas at the new scale. The old `Texture` and `CTFont`
        // drop here, releasing their underlying ObjC / CF objects.
        self.atlas = GlyphAtlas::new(&self.device, self.pixel_size, self.dpr);
    }

    /// # Safety
    /// `texture_ptr` must be a live `id<MTLTexture>` borrowed for the
    /// duration of this call. `term_ptr` may be null (empty draw).
    pub unsafe fn draw(
        &mut self,
        term_ptr: *const crate::ffi::BtTerm,
        texture_ptr: *const std::ffi::c_void,
        viewport_w: u32,
        viewport_h: u32,
        _time: f64,
    ) -> i32 {
        if texture_ptr.is_null() {
            return -1;
        }

        // Take a fresh snapshot. The cast from *const to *mut is safe
        // because BtTerm internally needs mutable access for snapshot
        // caching, and the renderer holds the only reference at this point.
        let snapshot = if term_ptr.is_null() {
            None
        } else {
            let term = &mut *(term_ptr as *mut crate::ffi::BtTerm);
            Some(term.snapshot_for_renderer().clone())
        };
        let (cells_slice, cols, rows): (&[CellSnapshot], u16, u16) = match snapshot {
            Some(ref s) => (s.cells.as_slice(), s.cols, s.rows),
            None => (&[], 0, 0),
        };
        self.draw_cells_inner(cells_slice, cols, rows, texture_ptr, viewport_w, viewport_h)
    }

    /// Same as `draw` but the cells come from a caller-provided slice
    /// instead of being snapshotted from a `BtTerm`. Used by Block view to
    /// render frozen row ranges (sealed blocks) and live row ranges
    /// (running blocks) through the same Metal pipeline.
    ///
    /// # Safety
    /// `texture_ptr` must be a live `id<MTLTexture>` borrowed for the
    /// duration of this call. `cells_ptr` may be null or `cells_len` zero
    /// for an empty draw. When non-empty, `cells_len` must equal
    /// `cols as usize * rows as usize`.
    // 9-arg signature predates the CT atlas swap; refactor tracked separately.
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn draw_cells(
        &mut self,
        cells_ptr: *const CellSnapshot,
        cells_len: usize,
        cols: u16,
        rows: u16,
        texture_ptr: *const std::ffi::c_void,
        viewport_w: u32,
        viewport_h: u32,
        _time: f64,
    ) -> i32 {
        if texture_ptr.is_null() {
            return -1;
        }
        let cells: &[CellSnapshot] = if cells_ptr.is_null() || cells_len == 0 {
            &[]
        } else {
            std::slice::from_raw_parts(cells_ptr, cells_len)
        };
        self.draw_cells_inner(cells, cols, rows, texture_ptr, viewport_w, viewport_h)
    }

    /// Single internal cell→vertex pipeline shared by `draw` and
    /// `draw_cells`. Forwards through `draw_cells_into_subregion` with
    /// `dest_y_px=0.0` and `clear_first=true` for zero-behaviour-change.
    unsafe fn draw_cells_inner(
        &mut self,
        cells: &[CellSnapshot],
        cols: u16,
        rows: u16,
        texture_ptr: *const std::ffi::c_void,
        viewport_w: u32,
        viewport_h: u32,
    ) -> i32 {
        self.draw_cells_into_subregion(
            cells,
            cols,
            rows,
            texture_ptr,
            viewport_w,
            viewport_h,
            0.0,
            true,
        )
    }

    /// pub(crate) forwarder so `block_list` FFI (Task 3) can paint a
    /// cell range at an arbitrary Y offset without cracking open the
    /// private helper. Same semantics as `draw_cells_into_subregion`.
    ///
    /// # Safety
    /// `texture_ptr` must be a live `id<MTLTexture>` borrowed for the
    /// duration of this call.
    #[allow(clippy::too_many_arguments)]
    pub(crate) unsafe fn draw_cells_subregion(
        &mut self,
        cells: &[CellSnapshot],
        cols: u16,
        rows: u16,
        texture_ptr: *const std::ffi::c_void,
        viewport_w: u32,
        viewport_h: u32,
        dest_y_px: f32,
        clear_first: bool,
    ) -> i32 {
        self.draw_cells_into_subregion(
            cells,
            cols,
            rows,
            texture_ptr,
            viewport_w,
            viewport_h,
            dest_y_px,
            clear_first,
        )
    }

    /// Encode a clear-only pass into `texture_ptr`. Used by the block
    /// view when no blocks are visible (e.g. empty session) so the
    /// drawable still gets cleared to `self.clear_color` and presented.
    ///
    /// # Safety
    /// `texture_ptr` must be a live `id<MTLTexture>` borrowed for the
    /// duration of this call.
    pub(crate) unsafe fn clear_viewport(
        &mut self,
        texture_ptr: *const std::ffi::c_void,
        _viewport_w: u32,
        _viewport_h: u32,
    ) -> i32 {
        self.draw_cells_into_subregion(&[], 0, 0, texture_ptr, _viewport_w, _viewport_h, 0.0, true)
    }

    /// Paint `cells` into a Y-offset region of `texture_ptr`. When
    /// `clear_first` is true the pass uses `MTLLoadAction::Clear`;
    /// otherwise `MTLLoadAction::Load` so prior content per-frame is
    /// preserved (multi-block per-drawable rendering).
    ///
    /// Returns 0 on success (including an empty `cells` slice — still
    /// encodes the clear/load pass), -1 on missing texture.
    #[allow(clippy::too_many_arguments)]
    unsafe fn draw_cells_into_subregion(
        &mut self,
        cells: &[CellSnapshot],
        cols: u16,
        rows: u16,
        texture_ptr: *const std::ffi::c_void,
        viewport_w: u32,
        viewport_h: u32,
        dest_y_px: f32,
        clear_first: bool,
    ) -> i32 {
        if texture_ptr.is_null() {
            return -1;
        }
        // Early-layout guard: viewport_w/h == 0 is legitimate during the
        // first frame before the MTKView gets a drawable size. The
        // vertex shader's `pos / viewportPx` would divide by zero;
        // skip the encode entirely.
        if viewport_w == 0 || viewport_h == 0 {
            return 0;
        }
        let mut verts: Vec<CellVertex> = Vec::new();
        if !cells.is_empty() {
            let (cell_w, cell_h) = self.atlas.cell_px;
            let cell_wf = cell_w as f32;
            let cell_hf = cell_h as f32;
            let cols = cols as usize;
            let rows = rows as usize;
            verts.reserve(cols * rows * VERTICES_PER_CELL);
            // Flag bits — must match `term.rs` snapshot encoding.
            const FLAG_WIDE_LEADING: u16 = 16;
            const FLAG_WIDE_TRAILING: u16 = 32;
            // Pre-ensure all needed glyphs (one-pass; the HashMap dedups).
            // Wide (CJK) glyphs get rasterised into a 2-cell-wide slot.
            for cell in cells {
                if cell.ch != 0 {
                    let wide = (cell.flags & FLAG_WIDE_LEADING) != 0;
                    self.atlas.ensure(cell.ch, wide);
                }
            }
            for r in 0..rows {
                for c in 0..cols {
                    let cell = cells[r * cols + c];
                    // Wide-trailing spacer cells contribute no quad — the
                    // preceding WIDE_LEADING cell's quad already spans both
                    // columns (including the spacer's background).
                    if (cell.flags & FLAG_WIDE_TRAILING) != 0 {
                        continue;
                    }
                    let glyph = self.atlas.lookup(cell.ch).copied();
                    let (uvo, uvs, glyph_wide, is_color) = match glyph {
                        Some(g) => (g.uv_origin, g.uv_size, g.wide, g.is_color),
                        None => ((0.0, 0.0), (0.0, 0.0), false, false),
                    };
                    // Wide (CJK) glyphs draw across two cell columns. A wide
                    // cell whose glyph couldn't be rasterised still claims
                    // both columns so the trailing spacer isn't drawn over by
                    // a neighbour and the bg colour stays consistent. Color
                    // emoji glyphs are auto-promoted to wide by the atlas
                    // even when the terminal didn't tag them WIDE_CHAR; honour
                    // that so the bitmap renders at its natural aspect ratio.
                    let wide = (cell.flags & FLAG_WIDE_LEADING) != 0 || glyph_wide;
                    let span = if wide { 2.0 } else { 1.0 };
                    let cell_span_w = cell_wf * span;
                    let x = c as f32 * cell_wf;
                    let y = r as f32 * cell_hf + dest_y_px;
                    let fg = rgba_to_float(cell.fg_rgba);
                    let bg = rgba_to_float(cell.bg_rgba);
                    let is_color_f = if is_color { 1.0 } else { 0.0 };
                    let v = |dx: f32, dy: f32, du: f32, dv: f32| CellVertex {
                        pos_x: x + dx * cell_span_w,
                        pos_y: y + dy * cell_hf,
                        uv_x: uvo.0 + du * uvs.0,
                        uv_y: uvo.1 + dv * uvs.1,
                        fg,
                        bg,
                        is_color: is_color_f,
                        _pad: [0.0; 3],
                    };
                    // Triangle 1: TL, TR, BL
                    verts.push(v(0.0, 0.0, 0.0, 0.0));
                    verts.push(v(1.0, 0.0, 1.0, 0.0));
                    verts.push(v(0.0, 1.0, 0.0, 1.0));
                    // Triangle 2: TR, BR, BL
                    verts.push(v(1.0, 0.0, 1.0, 0.0));
                    verts.push(v(1.0, 1.0, 1.0, 1.0));
                    verts.push(v(0.0, 1.0, 0.0, 1.0));
                }
            }
        }

        let vbuf = if verts.is_empty() {
            None
        } else {
            Some(self.device.new_buffer_with_data(
                verts.as_ptr() as *const _,
                (verts.len() * std::mem::size_of::<CellVertex>()) as u64,
                MTLResourceOptions::StorageModeShared,
            ))
        };

        #[repr(C)]
        struct Uniforms {
            vp_x: f32,
            vp_y: f32,
        }
        let uniforms = Uniforms {
            vp_x: viewport_w as f32,
            vp_y: viewport_h as f32,
        };

        // Texture is borrowed — wrap in ManuallyDrop to suppress the
        // release-on-drop behaviour of the owned Texture. Swift owns the
        // drawable's retain count.
        let texture = Texture::from_ptr(texture_ptr as *mut _);
        let texture = std::mem::ManuallyDrop::new(texture);

        let pass = RenderPassDescriptor::new();
        let att = pass.color_attachments().object_at(0).unwrap();
        att.set_texture(Some(&*texture));
        if clear_first {
            att.set_load_action(MTLLoadAction::Clear);
            let [cr, cg, cb, ca] = self.clear_color;
            att.set_clear_color(MTLClearColor::new(
                cr as f64, cg as f64, cb as f64, ca as f64,
            ));
        } else {
            att.set_load_action(MTLLoadAction::Load);
        }
        att.set_store_action(MTLStoreAction::Store);

        let cmd = self.queue.new_command_buffer();
        let enc = cmd.new_render_command_encoder(pass);
        enc.set_render_pipeline_state(&self.pipelines.cell_pso);
        if let Some(ref b) = vbuf {
            enc.set_vertex_buffer(0, Some(b), 0);
            enc.set_vertex_bytes(
                1,
                std::mem::size_of::<Uniforms>() as u64,
                &uniforms as *const Uniforms as *const _,
            );
            enc.set_fragment_texture(0, Some(&self.atlas.texture));
            enc.draw_primitives(MTLPrimitiveType::Triangle, 0, verts.len() as u64);
        }
        enc.end_encoding();
        cmd.commit();
        // NOT cmd.wait_until_completed — Swift's MTKView present happens on
        // its own schedule after this returns.
        0
    }

    /// Render visible block bodies in one frame. Called by the FFI; see
    /// `block_list_ffi::bt_renderer_draw_block_list` for the contract.
    ///
    /// # Safety
    /// `term` must be a valid `&mut BtTerm`. `texture_ptr` must be a live
    /// `id<MTLTexture>` borrowed for the call.
    pub(crate) unsafe fn draw_block_list(
        &mut self,
        term: &mut crate::ffi::BtTerm,
        texture_ptr: *const std::ffi::c_void,
        viewport_w: u32,
        viewport_h: u32,
        scroll_y_px: f32,
        entries: &[crate::renderer::block_list_ffi::BtBlockLayoutEntry],
    ) -> i32 {
        let viewport_h_f = viewport_h as f32;
        let mut painted_any = false;

        for entry in entries {
            let body_top_in_view = entry.body_y_top_px - scroll_y_px;
            let body_bot_in_view = body_top_in_view + entry.body_height_px;
            // Cull blocks entirely outside the viewport.
            if body_bot_in_view <= 0.0 || body_top_in_view >= viewport_h_f {
                continue;
            }
            // Resolve the snapshot: sealed -> frozen; running -> live range.
            // Linear scan over blocks: typical visible block count is < 20.
            let resolved: Option<crate::snapshot::GridSnapshot> = {
                let inner = term.inner_ref();
                let blocks = inner.blocks();
                let block = blocks.iter().find(|b| b.id == entry.block_id);
                match block {
                    Some(b) if b.frozen_snapshot.is_some() => b.frozen_snapshot.clone(),
                    Some(b) => {
                        let start = b.start_line;
                        let end = inner.current_line() + 1;
                        if end > start {
                            Some(inner.snapshot_range(start, end))
                        } else {
                            None
                        }
                    }
                    None => None,
                }
            };
            let Some(snap) = resolved else { continue };
            self.draw_cells_subregion(
                &snap.cells,
                snap.cols,
                snap.rows,
                texture_ptr,
                viewport_w,
                viewport_h,
                body_top_in_view,
                !painted_any, // clear_first only on the very first block
            );
            painted_any = true;
        }

        if !painted_any {
            self.clear_viewport(texture_ptr, viewport_w, viewport_h);
        }
        0
    }
}

fn rgba_to_float(rgba: u32) -> [f32; 4] {
    [
        ((rgba >> 24) & 0xFF) as f32 / 255.0,
        ((rgba >> 16) & 0xFF) as f32 / 255.0,
        ((rgba >> 8) & 0xFF) as f32 / 255.0,
        (rgba & 0xFF) as f32 / 255.0,
    ]
}
