// Brightness: a gamma curve over the finished frame (world and UI), as the original adjusted the
// display gamma. Black and white stay put; the darks and mid-tones move.

struct FullscreenVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

struct Brightness {
    exponent: f32,
    _padding: vec3<f32>,
};

@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var screen_sampler: sampler;
@group(0) @binding(2) var<uniform> settings: Brightness;

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let colour = textureSample(screen_texture, screen_sampler, in.uv);
    return vec4(pow(max(colour.rgb, vec3(0.0)), vec3(settings.exponent)), colour.a);
}
