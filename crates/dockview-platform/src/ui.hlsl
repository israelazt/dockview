// UI pass: rounded rectangles (signed distance field), soft shadows and
// glyphs from a single-channel atlas, alpha-blended over the video.

cbuffer Frame : register(b0) {
    float2 viewport;   // render target size in pixels
    float2 _pad;
};

Texture2D    atlas : register(t0);
SamplerState samp  : register(s0);

struct VsIn {
    float2 pos   : POSITION;
    float2 uv    : TEXCOORD0;
    float4 color : COLOR0;
    float4 shape : TEXCOORD1;
};

struct VsOut {
    float4 pos   : SV_Position;
    float2 uv    : TEXCOORD0;
    float4 color : COLOR0;
    float4 shape : TEXCOORD1;
};

VsOut vs_main(VsIn v) {
    VsOut o;
    o.pos = float4(v.pos.x / viewport.x * 2.0 - 1.0,
                   1.0 - v.pos.y / viewport.y * 2.0,
                   0.0, 1.0);
    o.uv = v.uv;
    o.color = v.color;
    o.shape = v.shape;
    return o;
}

// Distance from p to a rounded box centred at the origin (negative inside).
float sd_round_box(float2 p, float2 half_size, float radius) {
    float2 q = abs(p) - half_size + radius;
    return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - radius;
}

float4 ps_main(VsOut i) : SV_Target {
    // Glyph: shape.z < 0, uv addresses the atlas.
    if (i.shape.z < 0.0) {
        float coverage = atlas.Sample(samp, i.uv).r;
        return float4(i.color.rgb, i.color.a * coverage);
    }

    float d = sd_round_box(i.uv, i.shape.xy, i.shape.z);
    float blur = i.shape.w;
    float alpha;
    if (blur > 0.0) {
        // Shadow: fades out over `blur` pixels and is hidden under the shape
        // itself so it never darkens a translucent panel.
        alpha = (1.0 - smoothstep(0.0, blur, d)) * saturate(d + 0.5);
        alpha *= alpha;
    } else {
        // Shape: one pixel of antialiasing across the edge.
        alpha = saturate(0.5 - d);
    }
    return float4(i.color.rgb, i.color.a * alpha);
}
