use metal::{
    CompileOptions, Device, Library, MTLBlendFactor, MTLBlendOperation, MTLPixelFormat,
    RenderPipelineDescriptor, RenderPipelineState,
};

use super::shaders::TERMINAL_METAL_SOURCE;

pub struct Pipelines {
    pub library: Library,
    /// Cell quads (text). Opaque output — no blending.
    pub cell_pso: RenderPipelineState,
    /// Panel quads (Warp-style rounded block backgrounds). Alpha-blended
    /// premultiplied output so the rounded corners anti-alias against
    /// whatever drew before (i.e. the underlying clear colour).
    pub panel_pso: RenderPipelineState,
}

impl Pipelines {
    pub fn build(device: &Device) -> Result<Self, String> {
        let options = CompileOptions::new();
        let library = device
            .new_library_with_source(TERMINAL_METAL_SOURCE, &options)
            .map_err(|e| format!("shader compile failed: {e}"))?;

        let cell_pso = {
            let vfn = library.get_function("cell_vertex", None)?;
            let ffn = library.get_function("cell_fragment", None)?;
            let desc = RenderPipelineDescriptor::new();
            desc.set_vertex_function(Some(&vfn));
            desc.set_fragment_function(Some(&ffn));
            desc.color_attachments()
                .object_at(0)
                .unwrap()
                .set_pixel_format(MTLPixelFormat::BGRA8Unorm);
            device
                .new_render_pipeline_state(&desc)
                .map_err(|e| format!("cell pipeline build failed: {e}"))?
        };

        let panel_pso = {
            let vfn = library.get_function("panel_vertex", None)?;
            let ffn = library.get_function("panel_fragment", None)?;
            let desc = RenderPipelineDescriptor::new();
            desc.set_vertex_function(Some(&vfn));
            desc.set_fragment_function(Some(&ffn));
            let att = desc.color_attachments().object_at(0).unwrap();
            att.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
            // Standard premultiplied-source-over blending — rounded
            // corner alpha fades into whatever's already in the target.
            att.set_blending_enabled(true);
            att.set_rgb_blend_operation(MTLBlendOperation::Add);
            att.set_alpha_blend_operation(MTLBlendOperation::Add);
            att.set_source_rgb_blend_factor(MTLBlendFactor::One);
            att.set_source_alpha_blend_factor(MTLBlendFactor::One);
            att.set_destination_rgb_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
            att.set_destination_alpha_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
            device
                .new_render_pipeline_state(&desc)
                .map_err(|e| format!("panel pipeline build failed: {e}"))?
        };

        Ok(Self {
            library,
            cell_pso,
            panel_pso,
        })
    }
}
