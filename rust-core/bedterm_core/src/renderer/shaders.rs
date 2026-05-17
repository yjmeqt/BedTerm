//! Metal shader source. Compiled at runtime via
//! `MTLDevice.newLibraryWithSource:options:error:`.

pub const TERMINAL_METAL_SOURCE: &str = r#"
#include <metal_stdlib>
using namespace metal;

struct CellVertex {
    float2 pos;        // pixel coords inside the viewport (origin top-left)
    float2 uv;         // atlas UV (0..1, origin top-left)
    float4 fg;
    float4 bg;
};

struct Uniforms {
    float2 viewportPx;
};

struct VertexOut {
    float4 position [[position]];
    float2 uv;
    float4 fg;
    float4 bg;
};

vertex VertexOut cell_vertex(uint vid [[vertex_id]],
                             const device CellVertex *cells [[buffer(0)]],
                             constant Uniforms &u [[buffer(1)]]) {
    CellVertex v = cells[vid];
    float2 ndc = (v.pos / u.viewportPx) * 2.0 - 1.0;
    ndc.y = -ndc.y; // flip so row 0 is at the top
    VertexOut out;
    out.position = float4(ndc, 0.0, 1.0);
    out.uv = v.uv;
    out.fg = v.fg;
    out.bg = v.bg;
    return out;
}

constexpr sampler glyph_sampler(filter::linear, address::clamp_to_edge);

fragment float4 cell_fragment(VertexOut in [[stage_in]],
                              texture2d<float> atlas [[texture(0)]]) {
    float a = atlas.sample(glyph_sampler, in.uv).r;
    return mix(in.bg, in.fg, a);
}
"#;
