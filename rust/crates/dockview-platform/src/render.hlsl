// Full-screen triangle + YUV -> RGB conversion for NV12 and YUY2 frames.

struct VsOut {
    float4 pos : SV_Position;
    float2 uv  : TEXCOORD0;
};

cbuffer Params : register(b0) {
    float2 uv_scale;   // visible fraction of the texture (width / stride)
    float  hd;         // 1.0 = BT.709, 0.0 = BT.601
    float  visible_w;  // YUY2: visible texel pairs per row
    float  full_range; // 1.0 = 0-255 levels, 0.0 = video levels (16-235 / 16-240)
    float3 padding;    // the buffer is two 16-byte rows
};

Texture2D    tex_y  : register(t0); // NV12 luma, or packed YUY2 texels
Texture2D    tex_uv : register(t1); // NV12 interleaved chroma
SamplerState samp   : register(s0);

VsOut vs_main(uint id : SV_VertexID) {
    VsOut o;
    float2 uv = float2((id << 1) & 2, id & 2);
    o.pos = float4(uv * float2(2.0, -2.0) + float2(-1.0, 1.0), 0.0, 1.0);
    o.uv = uv;
    return o;
}

float3 yuv_to_rgb(float y, float u, float v) {
    if (full_range > 0.5) {
        u = u - 128.0 / 255.0;
        v = v - 128.0 / 255.0;
    } else {
        y = (y - 16.0 / 255.0) * (255.0 / 219.0);
        u = (u - 128.0 / 255.0) * (255.0 / 224.0);
        v = (v - 128.0 / 255.0) * (255.0 / 224.0);
    }
    if (hd > 0.5) {
        return float3(y + 1.5748 * v, y - 0.1873 * u - 0.4681 * v, y + 1.8556 * u);
    }
    return float3(y + 1.402 * v, y - 0.3441 * u - 0.7141 * v, y + 1.772 * u);
}

float4 ps_nv12(VsOut i) : SV_Target {
    float2 uv = i.uv * uv_scale;
    float y = tex_y.Sample(samp, uv).r;
    float2 c = tex_uv.Sample(samp, uv).rg;
    return float4(saturate(yuv_to_rgb(y, c.x, c.y)), 1.0);
}

// Catmull-Rom (bicubic) sampling with 9 bilinear taps: the 4 weights of each
// axis are folded into 3 positions, so the linear sampler does the rest. The
// weights are the ones of `catmull_rom_weights` in dockview-domain.
float4 sample_catmull_rom(Texture2D tex, float2 uv) {
    float2 size;
    tex.GetDimensions(size.x, size.y);
    float2 position = uv * size;
    float2 center = floor(position - 0.5) + 0.5;
    float2 f = position - center;
    float2 f2 = f * f;
    float2 f3 = f2 * f;
    float2 w0 = -0.5 * f3 + f2 - 0.5 * f;
    float2 w1 = 1.5 * f3 - 2.5 * f2 + 1.0;
    float2 w2 = -1.5 * f3 + 2.0 * f2 + 0.5 * f;
    float2 w3 = 0.5 * f3 - 0.5 * f2;
    float2 w12 = w1 + w2;
    // Stay inside the visible picture: the end of each row can be padding.
    float2 lo = 0.5 / size;
    float2 hi = float2(uv_scale.x, 1.0) - lo;
    float2 p0 = clamp((center - 1.0) / size, lo, hi);
    float2 p12 = clamp((center + w2 / w12) / size, lo, hi);
    float2 p3 = clamp((center + 2.0) / size, lo, hi);
    return tex.SampleLevel(samp, float2(p0.x, p0.y), 0) * (w0.x * w0.y)
         + tex.SampleLevel(samp, float2(p12.x, p0.y), 0) * (w12.x * w0.y)
         + tex.SampleLevel(samp, float2(p3.x, p0.y), 0) * (w3.x * w0.y)
         + tex.SampleLevel(samp, float2(p0.x, p12.y), 0) * (w0.x * w12.y)
         + tex.SampleLevel(samp, float2(p12.x, p12.y), 0) * (w12.x * w12.y)
         + tex.SampleLevel(samp, float2(p3.x, p12.y), 0) * (w3.x * w12.y)
         + tex.SampleLevel(samp, float2(p0.x, p3.y), 0) * (w0.x * w3.y)
         + tex.SampleLevel(samp, float2(p12.x, p3.y), 0) * (w12.x * w3.y)
         + tex.SampleLevel(samp, float2(p3.x, p3.y), 0) * (w3.x * w3.y);
}

float4 ps_nv12_bicubic(VsOut i) : SV_Target {
    float2 uv = i.uv * uv_scale;
    float y = sample_catmull_rom(tex_y, uv).r;
    float2 c = sample_catmull_rom(tex_uv, uv).rg;
    return float4(saturate(yuv_to_rgb(y, c.x, c.y)), 1.0);
}

float4 ps_yuy2(VsOut i) : SV_Target {
    // One texel holds two pixels: (Y0, U, Y1, V).
    float2 uv = i.uv * uv_scale;
    float4 t = tex_y.Sample(samp, uv);
    float second = frac(i.uv.x * visible_w) >= 0.5 ? 1.0 : 0.0;
    float y = lerp(t.r, t.b, second);
    return float4(saturate(yuv_to_rgb(y, t.g, t.a)), 1.0);
}
