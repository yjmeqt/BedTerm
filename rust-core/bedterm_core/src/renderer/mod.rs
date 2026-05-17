//! Metal renderer.

pub mod atlas;
pub mod ffi;
pub mod pipeline;
pub mod shaders;

use atlas::GlyphAtlas;
use metal::foreign_types::ForeignType;
use metal::{
    CommandQueue, Device, MTLClearColor, MTLLoadAction, MTLPrimitiveType, MTLStoreAction,
    RenderPassDescriptor, Texture,
};
use pipeline::Pipelines;

pub struct Renderer {
    pub(crate) device: Device,
    pub(crate) queue: CommandQueue,
    pub(crate) pipelines: Pipelines,
    pub(crate) atlas: GlyphAtlas,
    pub(crate) pixel_size: f32,
    pub(crate) dpr: f32,
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
        let atlas = GlyphAtlas::new(&device, 14.0, 3.0);
        Some(Self {
            device,
            queue,
            pipelines,
            atlas,
            pixel_size: 14.0,
            dpr: 3.0,
        })
    }

    pub fn set_font(&mut self, pixel_size: f32, dpr: f32) {
        self.pixel_size = pixel_size.max(1.0);
        self.dpr = dpr.max(1.0);
        // Rebuild the atlas at the new scale. The old `Texture` and `CTFont`
        // drop here, releasing their underlying ObjC / CF objects.
        self.atlas = GlyphAtlas::new(&self.device, self.pixel_size, self.dpr);
    }

    /// # Safety
    /// `texture_ptr` must be a live `id<MTLTexture>`. The pointer is borrowed
    /// for the duration of this call only — Rust does not retain it.
    pub unsafe fn draw(
        &mut self,
        _term: *const crate::ffi::BtTerm,
        texture_ptr: *const std::ffi::c_void,
        _viewport_w: u32,
        _viewport_h: u32,
        _time: f64,
    ) -> i32 {
        if texture_ptr.is_null() {
            return -1;
        }

        // Texture is borrowed — wrap in ManuallyDrop to suppress the
        // release-on-drop behaviour of the owned Texture. Swift owns the
        // drawable's retain count.
        let texture = Texture::from_ptr(texture_ptr as *mut _);
        let texture = std::mem::ManuallyDrop::new(texture);

        let pass = RenderPassDescriptor::new();
        let attachment = pass.color_attachments().object_at(0).unwrap();
        attachment.set_texture(Some(&*texture));
        attachment.set_load_action(MTLLoadAction::Clear);
        attachment.set_store_action(MTLStoreAction::Store);
        attachment.set_clear_color(MTLClearColor::new(0.05, 0.05, 0.08, 1.0));

        let cmd = self.queue.new_command_buffer();
        let enc = cmd.new_render_command_encoder(&pass);
        enc.set_render_pipeline_state(&self.pipelines.clear_pso);
        // Single full-screen triangle.
        enc.draw_primitives(MTLPrimitiveType::Triangle, 0, 3);
        enc.end_encoding();
        cmd.commit();
        // NOT cmd.wait_until_completed — Swift's MTKView present happens on
        // its own schedule after this returns.
        0
    }
}
