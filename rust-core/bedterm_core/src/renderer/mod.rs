//! Metal renderer.

pub mod ffi;

use metal::foreign_types::ForeignType;
use metal::{CommandQueue, Device};

pub struct Renderer {
    pub(crate) device: Device,
    pub(crate) queue: CommandQueue,
    pub(crate) pixel_size: f32,
    pub(crate) dpr: f32,
}

impl Renderer {
    /// Build a renderer from raw `id<MTLDevice>` / `id<MTLCommandQueue>` pointers.
    /// Pointers are retained by the renderer for its lifetime.
    ///
    /// # Safety
    /// `device_ptr` and `queue_ptr` must be non-null and point to live ObjC
    /// objects of the respective Metal protocols.
    pub unsafe fn from_ptrs(
        device_ptr: *const std::ffi::c_void,
        queue_ptr: *const std::ffi::c_void,
    ) -> Option<Self> {
        if device_ptr.is_null() || queue_ptr.is_null() {
            return None;
        }
        let device = Device::from_ptr(device_ptr as *mut _);
        let queue = CommandQueue::from_ptr(queue_ptr as *mut _);
        Some(Self {
            device,
            queue,
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
