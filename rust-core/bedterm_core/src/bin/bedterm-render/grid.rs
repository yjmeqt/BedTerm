//! Grid mode: stdin/fixture → BtTerm → Renderer::draw → PNG.

use std::fs;
use std::io::{self, Read};

use metal::foreign_types::ForeignType;
use metal::Device;

use bedterm_core::ffi::BtPaletteView;
use bedterm_core::renderer::Renderer;
use bedterm_core::term::Palette;

use crate::context::RenderContext;
use crate::png::{self, OffscreenTarget};

pub(crate) struct GridArgs {
    pub input: Option<String>,
}

impl Default for GridArgs {
    fn default() -> Self {
        Self { input: None }
    }
}

pub(crate) fn run(args: GridArgs, ctx: &RenderContext) -> Result<(), Box<dyn std::error::Error>> {
    let raw_bytes: Vec<u8> = match &args.input {
        Some(path) => fs::read(path)?,
        None => {
            let mut buf = Vec::new();
            io::stdin().read_to_end(&mut buf)?;
            buf
        }
    };
    let bytes = crate::context::normalize_newlines(&raw_bytes);

    let device = Device::system_default().ok_or("no Metal device found (must run on macOS)")?;
    let queue = device.new_command_queue();

    let mut renderer = unsafe {
        Renderer::from_ptrs(
            device.as_ptr() as *const std::ffi::c_void,
            queue.as_ptr() as *const std::ffi::c_void,
        )
    }
    .ok_or("failed to create renderer")?;

    crate::font::register_system_font();
    renderer.set_font(ctx.font_size_pt, ctx.scale);

    let (cell_w_px, cell_h_px) = renderer.cell_pixel_size();
    let cell_w = cell_w_px as f32;
    let cell_h = cell_h_px as f32;
    let (vp_w, vp_h) = ctx.viewport_px();
    let cols = ctx
        .cols
        .unwrap_or_else(|| (vp_w as f32 / cell_w).max(1.0) as u16);
    let rows = ctx
        .rows
        .unwrap_or_else(|| (vp_h as f32 / cell_h).max(1.0) as u16);

    let palette = resolve_palette(ctx);
    let bg = palette.default_bg;
    let clear = [
        bg.r as f32 / 255.0,
        bg.g as f32 / 255.0,
        bg.b as f32 / 255.0,
        1.0,
    ];
    renderer.set_clear_color(clear[0], clear[1], clear[2], clear[3]);

    let term = bedterm_core::ffi::bt_term_new(cols, rows);
    unsafe {
        let view = BtPaletteView {
            default_fg: palette.default_fg,
            default_bg: palette.default_bg,
            ansi: palette.ansi,
        };
        bedterm_core::ffi::bt_term_set_palette(term, &view);
        bedterm_core::ffi::bt_term_feed(term, bytes.as_ptr(), bytes.len());
    }

    let target = OffscreenTarget::new(&device, &queue, vp_w, vp_h)
        .ok_or("failed to create offscreen texture")?;

    unsafe {
        let ret = bedterm_core::renderer::ffi::bt_renderer_draw(
            &mut renderer as *mut Renderer as *mut bedterm_core::renderer::ffi::BtRenderer,
            term,
            target.texture_ptr(),
            vp_w,
            vp_h,
            0.0,
        );
        if ret != 0 {
            bedterm_core::ffi::bt_term_free(term);
            return Err("bt_renderer_draw failed".into());
        }
        bedterm_core::ffi::bt_term_free(term);
    }

    let pixels = target.read_pixels();
    let stdout = io::stdout();
    png::write_png(&mut stdout.lock(), &pixels, vp_w, vp_h)?;

    eprintln!(
        "[grid] cols={cols} rows={rows} cell={cell_w_px}×{cell_h_px}px viewport={vp_w}×{vp_h}px → PNG {vp_w}×{vp_h}"
    );
    Ok(())
}

fn resolve_palette(ctx: &RenderContext) -> Palette {
    match &ctx.palette {
        Some(name) => crate::blocks::PalettePreset::from_str(name)
            .map(|p| p.build())
            .unwrap_or_default(),
        None => Palette::default(),
    }
}
