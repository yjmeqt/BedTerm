//! Metal shader source. Compiled at runtime via
//! `MTLDevice.newLibraryWithSource:options:error:`.
//!
//! Two pipelines live in this source: `cell_vertex / cell_fragment` for
//! text cells, and `panel_vertex / panel_fragment` for Warp-style
//! rounded block backgrounds. The panel pipeline renders one quad per
//! visible block and produces an anti-aliased rounded-rectangle alpha
//! mask via a signed-distance function in the fragment stage.

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

struct PanelVertex {
    float2 pos;          // pixel coords inside the viewport
    float2 local;        // 0..1 within the panel
    float2 size;         // panel size in pixels
    float4 color;        // premultiplied bg colour
    float  cornerRadius; // in pixels
};

struct Uniforms {
    float2 viewportPx;
};

struct CellVertexOut {
    float4 position [[position]];
    float2 uv;
    float4 fg;
    float4 bg;
    float  isColor;
};

struct PanelVertexOut {
    float4 position [[position]];
    float2 local;
    float2 size;
    float4 color;
    float  cornerRadius;
};

vertex CellVertexOut cell_vertex(uint vid [[vertex_id]],
                                 const device CellVertex *cells [[buffer(0)]],
                                 constant Uniforms &u [[buffer(1)]]) {
    CellVertex v = cells[vid];
    float2 ndc = (v.pos / u.viewportPx) * 2.0 - 1.0;
    ndc.y = -ndc.y; // flip so row 0 is at the top
    CellVertexOut out;
    out.position = float4(ndc, 0.0, 1.0);
    out.uv = v.uv;
    out.fg = v.fg;
    out.bg = v.bg;
    out.isColor = v.isColor;
    return out;
}

constexpr sampler glyph_sampler(filter::linear, address::clamp_to_edge);

fragment float4 cell_fragment(CellVertexOut in [[stage_in]],
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

vertex PanelVertexOut panel_vertex(uint vid [[vertex_id]],
                                   const device PanelVertex *panels [[buffer(0)]],
                                   constant Uniforms &u [[buffer(1)]]) {
    PanelVertex p = panels[vid];
    float2 ndc = (p.pos / u.viewportPx) * 2.0 - 1.0;
    ndc.y = -ndc.y;
    PanelVertexOut out;
    out.position = float4(ndc, 0.0, 1.0);
    out.local = p.local;
    out.size = p.size;
    out.color = p.color;
    out.cornerRadius = p.cornerRadius;
    return out;
}

fragment float4 panel_fragment(PanelVertexOut in [[stage_in]]) {
    // Anti-aliased rounded-rect SDF. `q` is the offset from the
    // nearest corner's inner anchor; `d` is the signed distance to the
    // rounded boundary (negative inside, positive outside).
    float2 px = in.local * in.size;
    float2 halfSize = in.size * 0.5;
    float2 q = abs(px - halfSize) - (halfSize - in.cornerRadius);
    float d = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - in.cornerRadius;
    float alpha = 1.0 - smoothstep(-1.0, 0.0, d);
    // Premultiplied output: scale the colour by the coverage alpha.
    return float4(in.color.rgb * alpha, in.color.a * alpha);
}
"#;
