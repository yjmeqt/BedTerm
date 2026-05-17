//! Metal renderer.

pub mod ffi;
pub mod pipeline;
pub mod shaders;

use metal::foreign_types::ForeignType;
use metal::{CommandQueue, Device};
use pipeline::Pipelines;

pub struct Renderer {
    pub(crate) device: Device,
    pub(crate) queue: CommandQueue,
    pub(crate) pipelines: Pipelines,
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
        Some(Self {
            device,
            queue,
            pipelines,
            pixel_size: 14.0,
            dpr: 3.0,
        })
    }

    pub fn set_font(&mut self, pixel_size: f32, dpr: f32) {
        self.pixel_size = pixel_size.max(1.0);
        self.dpr = dpr.max(1.0);
        // Real atlas invalidation lands in Task 5.
    }
}
