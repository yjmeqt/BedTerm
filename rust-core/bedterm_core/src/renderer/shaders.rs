//! Metal shader source. Compiled at runtime via
//! `MTLDevice.newLibraryWithSource:options:error:`.

pub const TERMINAL_METAL_SOURCE: &str = r#"
#include <metal_stdlib>
using namespace metal;

struct Uniforms {
    uint32_t cols;
    uint32_t rows;
    float2   cellPx;
    float2   viewportPx;
};

struct VertexOut {
    float4 position [[position]];
    float4 colour;
};

vertex VertexOut clear_vertex(uint vid [[vertex_id]],
                              constant Uniforms &u [[buffer(1)]]) {
    // Full-screen triangle.
    float2 pos = float2((vid << 1) & 2, vid & 2);
    VertexOut out;
    out.position = float4(pos * 2.0 - 1.0, 0.0, 1.0);
    out.colour = float4(0.05, 0.05, 0.08, 1.0); // placeholder bg
    return out;
}

fragment float4 clear_fragment(VertexOut in [[stage_in]]) {
    return in.colour;
}
"#;
