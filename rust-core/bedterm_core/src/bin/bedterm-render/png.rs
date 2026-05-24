//! Offscreen Metal → PNG pipeline.

use std::io::Write;

use image::ImageEncoder;
use metal::foreign_types::ForeignType;
use metal::{
    CommandQueue, Device, MTLPixelFormat, MTLStorageMode, MTLTextureUsage, Texture,
    TextureDescriptor,
};

pub(crate) struct OffscreenTarget {
    pub texture: Texture,
    pub width: u32,
    pub height: u32,
    queue: CommandQueue,
}

impl OffscreenTarget {
    pub fn new(device: &Device, queue: &CommandQueue, width: u32, height: u32) -> Option<Self> {
        let desc = TextureDescriptor::new();
        desc.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
        desc.set_width(width as u64);
        desc.set_height(height as u64);
        desc.set_storage_mode(MTLStorageMode::Managed);
        desc.set_usage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
        let texture = device.new_texture(&desc);
        Some(Self {
            texture,
            width,
            height,
            queue: queue.clone(),
        })
    }

    pub fn texture_ptr(&self) -> *const std::ffi::c_void {
        self.texture.as_ptr() as *const std::ffi::c_void
    }

    pub fn read_pixels(&self) -> Vec<u8> {
        let cmd = self.queue.new_command_buffer();
        let blit = cmd.new_blit_command_encoder();
        blit.synchronize_resource(&self.texture);
        blit.end_encoding();
        cmd.commit();
        cmd.wait_until_completed();

        let bytes_per_row = self.width as usize * 4;
        let byte_count = bytes_per_row * self.height as usize;
        let mut pixels = vec![0u8; byte_count];
        self.texture.get_bytes(
            pixels.as_mut_ptr() as *mut _,
            bytes_per_row as u64,
            metal::MTLRegion {
                origin: metal::MTLOrigin { x: 0, y: 0, z: 0 },
                size: metal::MTLSize {
                    width: self.width as u64,
                    height: self.height as u64,
                    depth: 1,
                },
            },
            0,
        );
        pixels
    }
}

/// Encode BGRA8 pixel buffer as PNG, writing to `out`.
pub(crate) fn write_png(
    out: &mut dyn Write,
    pixels: &[u8],
    width: u32,
    height: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    // Convert BGRA → RGBA.
    let mut rgba = pixels.to_vec();
    for chunk in rgba.chunks_exact_mut(4) {
        chunk.swap(0, 2);
    }
    image::codecs::png::PngEncoder::new(out).write_image(
        &rgba,
        width,
        height,
        image::ExtendedColorType::Rgba8,
    )?;
    Ok(())
}
