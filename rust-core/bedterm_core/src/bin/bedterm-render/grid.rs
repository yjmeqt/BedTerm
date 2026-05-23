//! Grid mode: stdin/fixture → BtTerm → Renderer::draw → PNG.

use std::fs;
use std::io::{self, Read};

use metal::foreign_types::ForeignType;
use metal::Device;

use bedterm_core::ffi::BtPaletteView;
use bedterm_core::renderer::Renderer;
use bedterm_core::term::Palette;

use crate::png::{self, OffscreenTarget};

pub(crate) struct GridArgs {
    pub font_size: f32,
    pub viewport_w: u32,
    pub viewport_h: u32,
    pub input: Option<String>,
    pub palette: Palette,
}

impl Default for GridArgs {
    fn default() -> Self {
        Self {
            font_size: 14.0,
            viewport_w: 1200,
            viewport_h: 800,
            input: None,
            palette: Palette::default(),
        }
    }
}

pub(crate) fn run(args: GridArgs) -> Result<(), Box<dyn std::error::Error>> {
    let bytes: Vec<u8> = match &args.input {
        Some(path) => fs::read(path)?,
        None => {
            let mut buf = Vec::new();
            io::stdin().read_to_end(&mut buf)?;
            buf
        }
    };

    let cell_h = args.font_size;
    let cell_w = args.font_size * 0.5;
    let cols = (args.viewport_w as f32 / cell_w).max(1.0) as u16;
    let rows = (args.viewport_h as f32 / cell_h).max(1.0) as u16;

    let device = Device::system_default().ok_or("no Metal device found (must run on macOS)")?;
    let queue = device.new_command_queue();

    let mut renderer = unsafe {
        Renderer::from_ptrs(
            device.as_ptr() as *const std::ffi::c_void,
            queue.as_ptr() as *const std::ffi::c_void,
        )
    }
    .ok_or("failed to create renderer")?;

    // Clear color from palette background if no explicit --clear-color.
    let bg = args.palette.default_bg;
    let clear = [
        bg.r as f32 / 255.0,
        bg.g as f32 / 255.0,
        bg.b as f32 / 255.0,
        1.0,
    ];

    renderer.set_font(args.font_size, 2.0);
    renderer.set_clear_color(clear[0], clear[1], clear[2], clear[3]);

    let term = bedterm_core::ffi::bt_term_new(cols, rows);
    unsafe {
        let view = BtPaletteView {
            default_fg: args.palette.default_fg,
            default_bg: args.palette.default_bg,
            ansi: args.palette.ansi,
        };
        bedterm_core::ffi::bt_term_set_palette(term, &view);
        bedterm_core::ffi::bt_term_feed(term, bytes.as_ptr(), bytes.len());
    }

    let target = OffscreenTarget::new(&device, &queue, args.viewport_w, args.viewport_h)
        .ok_or("failed to create offscreen texture")?;

    unsafe {
        let ret = bedterm_core::renderer::ffi::bt_renderer_draw(
            &mut renderer as *mut Renderer as *mut bedterm_core::renderer::ffi::BtRenderer,
            term,
            target.texture_ptr(),
            args.viewport_w,
            args.viewport_h,
            0.0,
        );
        if ret != 0 {
            return Err("bt_renderer_draw failed".into());
        }
    }

    let pixels = target.read_pixels();
    let stdout = io::stdout();
    png::write_png(
        &mut stdout.lock(),
        &pixels,
        args.viewport_w,
        args.viewport_h,
    )?;

    unsafe {
        bedterm_core::ffi::bt_term_free(term);
    }

    Ok(())
}
