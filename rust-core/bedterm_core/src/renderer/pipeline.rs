use metal::{
    CompileOptions, Device, Library, MTLPixelFormat, RenderPipelineDescriptor,
    RenderPipelineState,
};

use super::shaders::TERMINAL_METAL_SOURCE;

pub struct Pipelines {
    pub library: Library,
    pub cell_pso: RenderPipelineState,
}

impl Pipelines {
    pub fn build(device: &Device) -> Result<Self, String> {
        let options = CompileOptions::new();
        let library = device
            .new_library_with_source(TERMINAL_METAL_SOURCE, &options)
            .map_err(|e| format!("shader compile failed: {e}"))?;

        let vfn = library.get_function("cell_vertex", None)?;
        let ffn = library.get_function("cell_fragment", None)?;

        let desc = RenderPipelineDescriptor::new();
        desc.set_vertex_function(Some(&vfn));
        desc.set_fragment_function(Some(&ffn));
        desc.color_attachments()
            .object_at(0)
            .unwrap()
            .set_pixel_format(MTLPixelFormat::BGRA8Unorm);

        let cell_pso = device
            .new_render_pipeline_state(&desc)
            .map_err(|e| format!("pipeline build failed: {e}"))?;

        Ok(Self { library, cell_pso })
    }
}
