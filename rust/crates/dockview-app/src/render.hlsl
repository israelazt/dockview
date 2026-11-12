// Full-screen triangle + YUV -> RGB conversion for NV12 and YUY2 frames.

struct VsOut {
    float4 pos : SV_Position;
    float2 uv  : TEXCOORD0;
};

cbuffer Params : register(b0) {
    float2 uv_scale;   // visible fraction of the texture (width / stride)
    float  hd;         // 1.0 = BT.709, 0.0 = BT.601
    float  visible_w;  // YUY2: visible texel pairs per row
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
    y = (y - 16.0 / 255.0) * (255.0 / 219.0);
    u = (u - 128.0 / 255.0) * (255.0 / 224.0);
    v = (v - 128.0 / 255.0) * (255.0 / 224.0);
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

float4 ps_yuy2(VsOut i) : SV_Target {
    // One texel holds two pixels: (Y0, U, Y1, V).
    float2 uv = i.uv * uv_scale;
    float4 t = tex_y.Sample(samp, uv);
    float second = frac(i.uv.x * visible_w) >= 0.5 ? 1.0 : 0.0;
    float y = lerp(t.r, t.b, second);
    return float4(saturate(yuv_to_rgb(y, t.g, t.a)), 1.0);
}
