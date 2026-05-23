//! Block list mode: stdin → layout → Renderer::draw_block_list → PNG.

use std::fs;
use std::io::{self, Read};

use metal::foreign_types::ForeignType;
use metal::Device;

use bedterm_core::renderer::block_list_ffi::bt_renderer_draw_block_list;
use bedterm_core::renderer::Renderer;
use bedterm_core::term::Terminal;

use crate::layout::{self, PaletteColors, TOKYO_NIGHT};
use crate::png::{self, OffscreenTarget};

pub(crate) struct BlockArgs {
    pub font_size: f32,
    pub viewport_w: u32,
    pub viewport_h: u32,
    pub clear_color: [f32; 4],
    pub ui_scale: f32,
    pub wrap_single_block: bool,
    pub input: Option<String>,
}

#[allow(dead_code)]
fn load_blocks_from_db() {
    // SQLite loading will be added in a follow-up.
}

impl Default for BlockArgs {
    fn default() -> Self {
        Self {
            font_size: 14.0,
            viewport_w: 1200,
            viewport_h: 800,
            clear_color: [0.0, 0.0, 0.0, 1.0],
            ui_scale: 2.0,
            wrap_single_block: false,
            input: None,
        }
    }
}

pub(crate) fn run(args: BlockArgs) -> Result<(), Box<dyn std::error::Error>> {
    let device = Device::system_default().ok_or("no Metal device found (must run on macOS)")?;
    let queue = device.new_command_queue();

    let cell_h = args.font_size;
    let cell_w = args.font_size * 0.5;
    let cols = (args.viewport_w as f32 / cell_w).max(1.0) as u16;
    let rows = (args.viewport_h as f32 / cell_h).max(1.0) as u16;

    let mut renderer = unsafe {
        Renderer::from_ptrs(
            device.as_ptr() as *const std::ffi::c_void,
            queue.as_ptr() as *const std::ffi::c_void,
        )
    }
    .ok_or("failed to create renderer")?;

    renderer.set_font(args.font_size, 2.0);
    renderer.set_clear_color(
        args.clear_color[0],
        args.clear_color[1],
        args.clear_color[2],
        args.clear_color[3],
    );
    renderer.set_ui_font_sizes(15.0 * args.ui_scale, 12.0 * args.ui_scale, args.ui_scale);

    // Load bytes and feed through terminal to build blocks.
    let bytes: Vec<u8> = match &args.input {
        Some(path) => fs::read(path)?,
        None => {
            let mut buf = Vec::new();
            io::stdin().read_to_end(&mut buf)?;
            buf
        }
    };

    let mut terminal = Terminal::new(cols, rows);
    if args.wrap_single_block {
        terminal.feed(b"\x1b]133;A\x07");
        terminal.feed(b"\x1b]133;B\x07");
        terminal.feed(&bytes);
        terminal.feed(b"\x1b]133;C\x07");
        terminal.feed(b"\x1b]133;D;0\x07");
    } else {
        terminal.feed(&bytes);
    }

    let block_count = terminal.blocks().len();
    if block_count == 0 {
        let target = OffscreenTarget::new(&device, &queue, args.viewport_w, args.viewport_h)
            .ok_or("failed to create offscreen texture")?;
        let pixels = target.read_pixels();
        let stdout = io::stdout();
        return png::write_png(
            &mut stdout.lock(),
            &pixels,
            args.viewport_w,
            args.viewport_h,
        );
    }

    let scale = args.ui_scale;
    let width_px = args.viewport_w as f32;
    let colors: &PaletteColors = &TOKYO_NIGHT;

    let row_height_pt = args.font_size / args.ui_scale;
    let ranges = layout::compute_block_ranges(terminal.blocks(), row_height_pt);
    let layout_entries = layout::build_layout_entries(&ranges, scale, width_px);
    let (mut headers, storage) = layout::build_header_descriptors(&ranges, scale, width_px, colors);
    let _blob = layout::patch_header_pointers(&mut headers, &storage);

    // Build an FFI term from the block data so draw_block_list can resolve cells.
    let ffi_term = bedterm_core::ffi::bt_term_new(cols, rows);
    unsafe {
        for block in terminal.blocks() {
            if !block.stylized_output.is_empty() {
                bedterm_core::ffi::bt_term_feed(
                    ffi_term,
                    block.stylized_output.as_ptr(),
                    block.stylized_output.len(),
                );
            }
        }
    }

    let target = OffscreenTarget::new(&device, &queue, args.viewport_w, args.viewport_h)
        .ok_or("failed to create offscreen texture")?;

    unsafe {
        let ret = bt_renderer_draw_block_list(
            &mut renderer as *mut Renderer as *mut bedterm_core::renderer::ffi::BtRenderer,
            ffi_term,
            target.texture_ptr(),
            args.viewport_w,
            args.viewport_h,
            0.0,
            layout_entries.as_ptr(),
            layout_entries.len(),
            headers.as_ptr(),
            headers.len(),
        );
        if ret != 0 {
            return Err("bt_renderer_draw_block_list failed".into());
        }
        bedterm_core::ffi::bt_term_free(ffi_term);
    }

    let pixels = target.read_pixels();
    let stdout = io::stdout();
    png::write_png(
        &mut stdout.lock(),
        &pixels,
        args.viewport_w,
        args.viewport_h,
    )
}
