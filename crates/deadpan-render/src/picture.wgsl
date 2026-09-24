struct Parameters {
    sampling_origin_x: vec4<f32>,
    sampling_y: vec4<f32>,
    coverage: vec4<f32>,
    interpretation: vec4<f32>,
    source_row0: vec4<f32>,
    source_row1: vec4<f32>,
    source_row2: vec4<f32>,
    display_row0: vec4<f32>,
    display_row1: vec4<f32>,
    display_row2: vec4<f32>,
}

@group(0) @binding(0) var picture: texture_2d<f32>;
@group(0) @binding(1) var<uniform> parameters: Parameters;

@vertex
fn fullscreen(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let positions = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    return vec4(positions[index], 0.0, 1.0);
}

fn decode(value: f32) -> f32 {
    switch u32(parameters.interpretation.y) {
        case 0u: {
            if value <= 0.04045 { return value / 12.92; }
            return pow((value + 0.055) / 1.055, 2.4);
        }
        case 1u: {
            if value < 0.081 { return value / 4.5; }
            return pow((value + 0.099) / 1.099, 1.0 / 0.45);
        }
        default: { return value; }
    }
}

fn working_texel(position: vec2<i32>) -> vec3<f32> {
    let bounds = vec2<i32>(textureDimensions(picture)) - vec2(1);
    let rgba = textureLoad(picture, clamp(position, vec2(0), bounds), 0);
    let linear = vec3(decode(rgba.r), decode(rgba.g), decode(rgba.b));
    // No clamp in the working transform. Premultiply before resampling to keep
    // transparent color from bleeding into opaque neighbors.
    return vec3(dot(parameters.source_row0.xyz, linear),
                dot(parameters.source_row1.xyz, linear),
                dot(parameters.source_row2.xyz, linear)) * rgba.a;
}

@fragment
fn interpret(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    if any(position.xy < parameters.coverage.xy) || any(position.xy >= parameters.coverage.zw) {
        return vec4(0.0, 0.0, 0.0, 1.0);
    }
    let offset = position.xy - (parameters.coverage.xy + vec2(0.5));
    let uv = parameters.sampling_origin_x.xy
        + offset.x * parameters.sampling_origin_x.zw
        + offset.y * parameters.sampling_y.xy;
    let source = uv * vec2<f32>(textureDimensions(picture)) - vec2(0.5);
    let base = vec2<i32>(floor(source));
    let fraction = fract(source);
    let top = mix(working_texel(base), working_texel(base + vec2(1, 0)), fraction.x);
    let bottom = mix(working_texel(base + vec2(0, 1)), working_texel(base + vec2(1, 1)), fraction.x);
    // Opaque black background: premultiplied RGB already is the composite.
    return vec4(mix(top, bottom, fraction.y), 1.0);
}

fn srgb_encode(value: f32) -> f32 {
    let clipped = clamp(value, 0.0, 1.0);
    if clipped <= 0.0031308 { return clipped * 12.92; }
    return 1.055 * pow(clipped, 1.0 / 2.4) - 0.055;
}

@fragment
fn display(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let working = textureLoad(picture, vec2<i32>(position.xy), 0).rgb;
    let linear = vec3(dot(parameters.display_row0.xyz, working),
                      dot(parameters.display_row1.xyz, working),
                      dot(parameters.display_row2.xyz, working));
    // Rgba8Unorm is intentional: transfer encoding occurs exactly once here.
    return vec4(srgb_encode(linear.r), srgb_encode(linear.g), srgb_encode(linear.b), 1.0);
}
