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
    float  isColor;    // 1.0 = color bitmap glyph, 0.0 = monochrome alpha mask
};

struct Uniforms {
    float2 viewportPx;
};

struct VertexOut {
    float4 position [[position]];
    float2 uv;
    float4 fg;
    float4 bg;
    float  isColor;
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
    out.isColor = v.isColor;
    return out;
}

constexpr sampler glyph_sampler(filter::linear, address::clamp_to_edge);

fragment float4 cell_fragment(VertexOut in [[stage_in]],
                              texture2d<float> atlas [[texture(0)]]) {
    // Atlas is BGRA8Unorm premultiplied. Metal's float sample returns RGBA
    // in linear order regardless of the underlying byte order, so `sample`
    // is (R, G, B, A) — already premultiplied.
    float4 sample = atlas.sample(glyph_sampler, in.uv);
    if (in.isColor > 0.5) {
        // Color bitmap glyph (Apple Color Emoji). Composite over the cell
        // background using the premultiplied source-over formula.
        return float4(sample.rgb + in.bg.rgb * (1.0 - sample.a), 1.0);
    }
    // Monochrome glyph: the bitmap is white premultiplied by coverage, so
    // sample.a is the alpha mask. Tint by foreground colour over background.
    return mix(in.bg, in.fg, sample.a);
}
"#;
