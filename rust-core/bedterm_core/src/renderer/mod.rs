//! Metal renderer.

pub mod atlas;
pub mod block_list_ffi;
pub mod cells;
#[cfg(any(target_os = "ios", target_os = "macos"))]
pub(crate) mod coretext_raster;
pub mod ffi;
pub(crate) mod font_system;
pub mod glyph_raster;
pub(crate) mod header_band;
pub mod icon_atlas;
pub mod pipeline;
pub mod shaders;
pub(crate) mod ui_text;

use atlas::GlyphAtlas;
use cells::{CellVertex, PanelVertex, VERTICES_PER_CELL, VERTICES_PER_PANEL};

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
    /// Subheadline font size in **pixels** (point × scale). Pushed
    /// from Swift via `bt_renderer_set_ui_font_sizes_px`. Used for
    /// per-block header command text.
    pub(crate) ui_subheadline_px: f32,
    /// Caption2 font size in pixels — used for header subtitle.
    pub(crate) ui_caption2_px: f32,
    /// Current UI scale (UIScreen.scale). Fallback 3.0 covers initial
    /// frames before Swift has a window.
    pub(crate) ui_scale: f32,
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
            ui_subheadline_px: 0.0,
            ui_caption2_px: 0.0,
            ui_scale: 3.0,
        })
    }

    /// Push UI font sizes resolved from `UIFont.preferredFont(forTextStyle:)`.
    /// Values arrive in pixels (Swift multiplies point × screen scale).
    /// Invalidates the UI-glyph atlas cache whenever the sizes actually
    /// change so a Dynamic Type bump re-rasterizes at the new pixel
    /// size on the next frame.
    pub fn set_ui_font_sizes(&mut self, sub: f32, cap: f32, scale: f32) {
        let new_sub = sub.max(1.0);
        let new_cap = cap.max(1.0);
        let new_scale = scale.max(1.0);
        // Float compare via hundredths-of-px equality — same precision the
        // atlas's hash key uses, so we invalidate IFF a different cache
        // key would result for any glyph at the same codepoint.
        let changed = ((self.ui_subheadline_px - new_sub).abs() > 0.005)
            || ((self.ui_caption2_px - new_cap).abs() > 0.005)
            || ((self.ui_scale - new_scale).abs() > 0.005);
        self.ui_subheadline_px = new_sub;
        self.ui_caption2_px = new_cap;
        self.ui_scale = new_scale;
        if changed {
            self.atlas.reset_ui_caches();
        }
    }

    pub fn set_clear_color(&mut self, r: f32, g: f32, b: f32, a: f32) {
        self.clear_color = [r, g, b, a];
    }

    /// Actual cell pixel dimensions after font registration.
    pub fn cell_pixel_size(&self) -> (u32, u32) {
        self.atlas.cell_px
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

    /// Paint `cells` into a Y-offset region of `texture_ptr`. When
    /// `clear_first` is true the pass uses `MTLLoadAction::Clear`;
    /// otherwise `MTLLoadAction::Load`.
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
        // first frame before the MTKView gets a drawable size.
        if viewport_w == 0 || viewport_h == 0 {
            return 0;
        }
        let mut verts: Vec<CellVertex> = Vec::new();
        self.append_cells_to_verts(cells, cols, rows, dest_y_px, &mut verts);
        self.encode_cell_pass(texture_ptr, viewport_w, viewport_h, &verts, clear_first);
        0
    }

    /// Render visible block bodies in one frame. One render pass with
    /// one Clear, one draw call covering the concatenated vertex buffer
    /// of every visible block. Called by the FFI; see
    /// `block_list_ffi::bt_renderer_draw_block_list` for the contract.
    ///
    /// # Safety
    /// `term` must be a valid `&mut BtTerm`. `texture_ptr` must be a live
    /// `id<MTLTexture>` borrowed for the call.
    #[allow(clippy::too_many_arguments)]
    pub(crate) unsafe fn draw_block_list(
        &mut self,
        term: &mut crate::ffi::BtTerm,
        texture_ptr: *const std::ffi::c_void,
        viewport_w: u32,
        viewport_h: u32,
        scroll_y_px: f32,
        entries: &[crate::renderer::block_list_ffi::BtBlockLayoutEntry],
        headers: &[crate::renderer::block_list_ffi::BtBlockHeaderEntry],
    ) -> i32 {
        if texture_ptr.is_null() {
            return -1;
        }
        if viewport_w == 0 || viewport_h == 0 {
            return 0;
        }

        let viewport_h_f = viewport_h as f32;
        let mut cell_verts: Vec<CellVertex> = Vec::new();
        let mut panel_verts: Vec<PanelVertex> = Vec::new();

        for entry in entries {
            let body_top_in_view = entry.body_y_top_px - scroll_y_px;
            let body_bot_in_view = body_top_in_view + entry.body_height_px;
            // Cull blocks entirely outside the viewport.
            if body_bot_in_view <= 0.0 || body_top_in_view >= viewport_h_f {
                continue;
            }

            // Panel chrome — only if Swift supplied a non-zero RGBA.
            if entry.panel_bg_rgba != 0 {
                let panel_top_in_view = entry.panel_y_top_px - scroll_y_px;
                Self::append_panel_to_verts(
                    entry.panel_x_left_px,
                    panel_top_in_view,
                    entry.panel_width_px,
                    entry.panel_height_px,
                    entry.panel_corner_radius_px,
                    entry.panel_bg_rgba,
                    &mut panel_verts,
                );
            }

            // Cells. Resolution order:
            //   1. `block.grid` — live, running command. The block owns
            //      its own VTE + grid; render whatever cells are
            //      currently there. Mirrors Warp's `output_grid` model
            //      so claude / fzf / gum cursor-positioning redraws
            //      stay scoped to the block's body.
            //   2. `block.frozen_snapshot` — sealed block. Snapshot
            //      captured at `CommandFinished` from the (now-dropped)
            //      block grid.
            //   3. Fallback: global terminal row range — legacy path
            //      for blocks that predate the per-block grid (e.g.
            //      Ctrl-C path that synthesises a block without ever
            //      hitting `Preexec`).
            let resolved: Option<crate::snapshot::GridSnapshot> = {
                let inner = term.inner_ref();
                let palette = inner.palette();
                let blocks = inner.blocks();
                let block = blocks.iter().find(|b| b.id == entry.block_id);
                match block {
                    Some(b) if b.grid.is_some() => b.grid.as_ref().map(|g| g.snapshot(palette)),
                    // Frozen blocks store palette-agnostic colours;
                    // re-resolve every frame so the body adopts the
                    // live light↔dark palette instead of the colours
                    // baked at seal-time.
                    Some(b) if b.frozen_snapshot.is_some() => {
                        b.frozen_snapshot.as_ref().map(|raw| raw.resolve(palette))
                    }
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
            self.append_cells_to_verts(
                &snap.cells,
                snap.cols,
                snap.rows,
                body_top_in_view,
                &mut cell_verts,
            );
        }

        // Header bands. Collected into separate vertex buffers so the
        // sticky band's opaque rectangle paints AFTER all body cells —
        // otherwise the body cells under the pinned header would be
        // drawn last and the band looks transparent. Non-sticky and
        // sticky headers share the buffers; sticky is appended last so
        // it z-sorts above its sibling on hand-off.
        let mut header_panel_verts: Vec<PanelVertex> = Vec::new();
        let mut header_cell_verts: Vec<CellVertex> = Vec::new();
        if !headers.is_empty() {
            let (cell_w_px, cell_h_px) = self.atlas.cell_px;
            let mut ctx = crate::renderer::header_band::HeaderDrawContext {
                subheadline_px: self.ui_subheadline_px,
                caption2_px: self.ui_caption2_px,
                scale: self.ui_scale,
                viewport_w: viewport_w as f32,
                viewport_h: viewport_h as f32,
                scroll_y_px,
                surface_bg_rgba: rgba_f32_to_u32(self.clear_color),
                atlas: &mut self.atlas,
                cell_w_px: cell_w_px as f32,
                cell_h_px: cell_h_px as f32,
            };
            for h in headers.iter().filter(|h| h.is_sticky == 0) {
                crate::renderer::header_band::emit_header(
                    &mut ctx,
                    h,
                    &mut header_panel_verts,
                    &mut header_cell_verts,
                );
            }
            for h in headers.iter().filter(|h| h.is_sticky != 0) {
                crate::renderer::header_band::emit_header(
                    &mut ctx,
                    h,
                    &mut header_panel_verts,
                    &mut header_cell_verts,
                );
            }
        }

        // One render pass, four phases:
        //   1. body panels  (currently always empty — Swift passes
        //      panel_bg_rgba=0, so the renderer skips them)
        //   2. body cells   (block output text)
        //   3. header panels (opaque band — must overpaint body cells
        //      so the sticky pin actually occludes content underneath)
        //   4. header cells (command + subtitle glyphs, badge icons)
        self.encode_block_pass(
            texture_ptr,
            viewport_w,
            viewport_h,
            &panel_verts,
            &cell_verts,
            &header_panel_verts,
            &header_cell_verts,
        );
        0
    }

    fn append_panel_to_verts(
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        corner_radius: f32,
        rgba: u32,
        verts: &mut Vec<PanelVertex>,
    ) {
        if width <= 0.0 || height <= 0.0 {
            return;
        }
        verts.reserve(VERTICES_PER_PANEL);
        let color = rgba_to_premultiplied(rgba);
        let v = |dx: f32, dy: f32| PanelVertex {
            pos_x: x + dx * width,
            pos_y: y + dy * height,
            local_x: dx,
            local_y: dy,
            size_x: width,
            size_y: height,
            color,
            corner_radius,
            _pad: [0.0; 3],
        };
        // TL, TR, BL  /  TR, BR, BL
        verts.push(v(0.0, 0.0));
        verts.push(v(1.0, 0.0));
        verts.push(v(0.0, 1.0));
        verts.push(v(1.0, 0.0));
        verts.push(v(1.0, 1.0));
        verts.push(v(0.0, 1.0));
    }

    /// Single render pass, four ordered phases: body panels → body cells →
    /// header panels → header cells.
    ///
    /// The split exists so the sticky header band's opaque rectangle paints
    /// AFTER all body cells — otherwise body cells in the pinned band's
    /// Y-range would draw last and the header would look transparent.
    /// Always clears with `self.clear_color`.
    #[allow(clippy::too_many_arguments)]
    unsafe fn encode_block_pass(
        &mut self,
        texture_ptr: *const std::ffi::c_void,
        viewport_w: u32,
        viewport_h: u32,
        body_panel_verts: &[PanelVertex],
        body_cell_verts: &[CellVertex],
        header_panel_verts: &[PanelVertex],
        header_cell_verts: &[CellVertex],
    ) {
        #[repr(C)]
        struct Uniforms {
            vp_x: f32,
            vp_y: f32,
        }
        let uniforms = Uniforms {
            vp_x: viewport_w as f32,
            vp_y: viewport_h as f32,
        };

        let make_panel_buf = |verts: &[PanelVertex]| {
            (!verts.is_empty()).then(|| {
                self.device.new_buffer_with_data(
                    verts.as_ptr() as *const _,
                    std::mem::size_of_val(verts) as u64,
                    MTLResourceOptions::StorageModeShared,
                )
            })
        };
        let make_cell_buf = |verts: &[CellVertex]| {
            (!verts.is_empty()).then(|| {
                self.device.new_buffer_with_data(
                    verts.as_ptr() as *const _,
                    std::mem::size_of_val(verts) as u64,
                    MTLResourceOptions::StorageModeShared,
                )
            })
        };
        let body_panel_buf = make_panel_buf(body_panel_verts);
        let body_cell_buf = make_cell_buf(body_cell_verts);
        let header_panel_buf = make_panel_buf(header_panel_verts);
        let header_cell_buf = make_cell_buf(header_cell_verts);

        let texture = Texture::from_ptr(texture_ptr as *mut _);
        let texture = std::mem::ManuallyDrop::new(texture);

        let pass = RenderPassDescriptor::new();
        let att = pass.color_attachments().object_at(0).unwrap();
        att.set_texture(Some(&*texture));
        att.set_load_action(MTLLoadAction::Clear);
        let [cr, cg, cb, ca] = self.clear_color;
        att.set_clear_color(MTLClearColor::new(
            cr as f64, cg as f64, cb as f64, ca as f64,
        ));
        att.set_store_action(MTLStoreAction::Store);

        let cmd = self.queue.new_command_buffer();
        let enc = cmd.new_render_command_encoder(pass);

        let draw_panels = |verts: &[PanelVertex], buf: &Option<metal::Buffer>| {
            if let Some(ref b) = buf {
                enc.set_render_pipeline_state(&self.pipelines.panel_pso);
                enc.set_vertex_buffer(0, Some(b), 0);
                enc.set_vertex_bytes(
                    1,
                    std::mem::size_of::<Uniforms>() as u64,
                    &uniforms as *const Uniforms as *const _,
                );
                enc.draw_primitives(MTLPrimitiveType::Triangle, 0, verts.len() as u64);
            }
        };
        let draw_cells = |verts: &[CellVertex], buf: &Option<metal::Buffer>| {
            if let Some(ref b) = buf {
                enc.set_render_pipeline_state(&self.pipelines.cell_pso);
                enc.set_vertex_buffer(0, Some(b), 0);
                enc.set_vertex_bytes(
                    1,
                    std::mem::size_of::<Uniforms>() as u64,
                    &uniforms as *const Uniforms as *const _,
                );
                enc.set_fragment_texture(0, Some(&self.atlas.texture));
                enc.draw_primitives(MTLPrimitiveType::Triangle, 0, verts.len() as u64);
            }
        };

        draw_panels(body_panel_verts, &body_panel_buf);
        draw_cells(body_cell_verts, &body_cell_buf);
        draw_panels(header_panel_verts, &header_panel_buf);
        draw_cells(header_cell_verts, &header_cell_buf);

        enc.end_encoding();
        cmd.commit();
    }

    /// Build per-cell quad vertices for one block's grid, appending into
    /// the caller's vertex buffer. Shared between the per-block
    /// `draw_cells_into_subregion` path and the one-pass `draw_block_list`
    /// path so glyph-atlas state, wide-glyph handling, and color-emoji
    /// promotion stay in one place.
    fn append_cells_to_verts(
        &mut self,
        cells: &[CellSnapshot],
        cols: u16,
        rows: u16,
        dest_y_px: f32,
        verts: &mut Vec<CellVertex>,
    ) {
        if cells.is_empty() {
            return;
        }
        let (cell_w, cell_h) = self.atlas.cell_px;
        let cell_wf = cell_w as f32;
        let cell_hf = cell_h as f32;
        let cols = cols as usize;
        let rows = rows as usize;
        verts.reserve(cols * rows * VERTICES_PER_CELL);
        // Flag bits — must match `term.rs` snapshot encoding
        // (see `snapshot.rs:25`).
        const FLAG_BOLD: u16 = 1;
        const FLAG_UNDERLINE: u16 = 2;
        const FLAG_INVERSE: u16 = 4;
        const FLAG_ITALIC: u16 = 8;
        const FLAG_WIDE_LEADING: u16 = 16;
        const FLAG_WIDE_TRAILING: u16 = 32;
        const FLAG_STRIKETHROUGH: u16 = 64;
        for cell in cells {
            if cell.ch != 0 {
                let wide = (cell.flags & FLAG_WIDE_LEADING) != 0;
                let key = crate::renderer::atlas::GlyphKey {
                    codepoint: cell.ch,
                    bold: (cell.flags & FLAG_BOLD) != 0,
                    italic: (cell.flags & FLAG_ITALIC) != 0,
                };
                self.atlas.ensure(key, wide);
            }
        }
        for r in 0..rows {
            for c in 0..cols {
                let cell = cells[r * cols + c];
                if (cell.flags & FLAG_WIDE_TRAILING) != 0 {
                    continue;
                }
                let glyph_key = crate::renderer::atlas::GlyphKey {
                    codepoint: cell.ch,
                    bold: (cell.flags & FLAG_BOLD) != 0,
                    italic: (cell.flags & FLAG_ITALIC) != 0,
                };
                let glyph = self.atlas.lookup(glyph_key).copied();
                let (uvo, uvs, glyph_wide, is_color) = match glyph {
                    Some(g) => (g.uv_origin, g.uv_size, g.wide, g.is_color),
                    None => ((0.0, 0.0), (0.0, 0.0), false, false),
                };
                let wide = (cell.flags & FLAG_WIDE_LEADING) != 0 || glyph_wide;
                let span = if wide { 2.0 } else { 1.0 };
                let cell_span_w = cell_wf * span;
                let x = c as f32 * cell_wf;
                let y = r as f32 * cell_hf + dest_y_px;
                // SGR 7 inverse: swap fg / bg at vertex emission. The
                // cell shader's `mix(bg, fg, sample.a)` then paints the
                // glyph in the original bg over the original fg — what
                // vim's visual selection / fzf's highlighted row / less's
                // search hit are after. Underline below picks up the
                // post-swap `fg` so the hairline still reads against the
                // inverted background.
                let (fg, bg) = if (cell.flags & FLAG_INVERSE) != 0 {
                    (rgba_to_float(cell.bg_rgba), rgba_to_float(cell.fg_rgba))
                } else {
                    (rgba_to_float(cell.fg_rgba), rgba_to_float(cell.bg_rgba))
                };
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
                verts.push(v(0.0, 0.0, 0.0, 0.0));
                verts.push(v(1.0, 0.0, 1.0, 0.0));
                verts.push(v(0.0, 1.0, 0.0, 1.0));
                verts.push(v(1.0, 0.0, 1.0, 0.0));
                verts.push(v(1.0, 1.0, 1.0, 1.0));
                verts.push(v(0.0, 1.0, 0.0, 1.0));

                // SGR 4 underline + SGR 9 strikethrough. Both reuse the
                // cell pipeline by sampling the atlas's reserved
                // solid-alpha texel — the fragment shader's
                // `mix(bg, fg, sample.a)` with sample.a == 1 yields fg,
                // so a quad with `fg = cell.fg_rgba` becomes a flat
                // fg-coloured rectangle of the requested thickness.
                // Underline sits below the baseline; strikethrough sits
                // at roughly the x-height midline so it visually
                // strikes through lowercase letters.
                let needs_underline = (cell.flags & FLAG_UNDERLINE) != 0;
                let needs_strike = (cell.flags & FLAG_STRIKETHROUGH) != 0;
                if needs_underline || needs_strike {
                    let thickness = (self.dpr.round() as u32).max(1) as f32;
                    let (su, sv) = self.atlas.solid_uv;
                    let mut emit_decoration = |line_y: f32| {
                        let solid = |dx: f32, dy: f32| CellVertex {
                            pos_x: x + dx * cell_span_w,
                            pos_y: line_y + dy * thickness,
                            uv_x: su,
                            uv_y: sv,
                            fg,
                            bg,
                            is_color: 0.0,
                            _pad: [0.0; 3],
                        };
                        verts.push(solid(0.0, 0.0));
                        verts.push(solid(1.0, 0.0));
                        verts.push(solid(0.0, 1.0));
                        verts.push(solid(1.0, 0.0));
                        verts.push(solid(1.0, 1.0));
                        verts.push(solid(0.0, 1.0));
                    };
                    if needs_underline {
                        emit_decoration(y + self.atlas.ascent_px as f32 + thickness);
                    }
                    if needs_strike {
                        // x-height midline ≈ baseline − 0.30 × ascent.
                        // Empirical for Menlo / SF: matches where the
                        // cross-bar of `e` / `a` sits, so the line
                        // visually bisects lowercase letters rather
                        // than sitting on top of them.
                        let ascent = self.atlas.ascent_px as f32;
                        let strike_y = y + ascent - ascent * 0.30;
                        emit_decoration(strike_y);
                    }
                }
            }
        }
    }

    /// Emit one render pass: Clear or Load → optional vertex draw → Store
    /// → commit. Shared between the per-block subregion path and the
    /// one-pass block-list path.
    unsafe fn encode_cell_pass(
        &mut self,
        texture_ptr: *const std::ffi::c_void,
        viewport_w: u32,
        viewport_h: u32,
        verts: &[CellVertex],
        clear: bool,
    ) {
        #[repr(C)]
        struct Uniforms {
            vp_x: f32,
            vp_y: f32,
        }
        let uniforms = Uniforms {
            vp_x: viewport_w as f32,
            vp_y: viewport_h as f32,
        };

        let vbuf = if verts.is_empty() {
            None
        } else {
            Some(self.device.new_buffer_with_data(
                verts.as_ptr() as *const _,
                std::mem::size_of_val(verts) as u64,
                MTLResourceOptions::StorageModeShared,
            ))
        };

        let texture = Texture::from_ptr(texture_ptr as *mut _);
        let texture = std::mem::ManuallyDrop::new(texture);

        let pass = RenderPassDescriptor::new();
        let att = pass.color_attachments().object_at(0).unwrap();
        att.set_texture(Some(&*texture));
        if clear {
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

/// Pack a [r,g,b,a] f32 tuple (the renderer's `clear_color` storage
/// format) back into the 0xRRGGBBAA word the header pipeline expects.
fn rgba_f32_to_u32(rgba: [f32; 4]) -> u32 {
    let to_u8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    (to_u8(rgba[0]) << 24) | (to_u8(rgba[1]) << 16) | (to_u8(rgba[2]) << 8) | to_u8(rgba[3])
}

/// Same as `rgba_to_float` but multiplies RGB by alpha so the panel
/// pipeline (premultiplied source-over blending) doesn't double-dip
/// on the alpha.
fn rgba_to_premultiplied(rgba: u32) -> [f32; 4] {
    let a = (rgba & 0xFF) as f32 / 255.0;
    [
        ((rgba >> 24) & 0xFF) as f32 / 255.0 * a,
        ((rgba >> 16) & 0xFF) as f32 / 255.0 * a,
        ((rgba >> 8) & 0xFF) as f32 / 255.0 * a,
        a,
    ]
}
