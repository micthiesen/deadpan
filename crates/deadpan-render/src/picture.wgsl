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
    // x: tone-map each source texel (SDR output, HDR source); y: tone-map the
    // composite for the SDR preview (HDR output); z: HLG OOTF; w: unused.
    hdr: vec4<f32>,
    // Highlight tone map: knee k, headroom 1 - k, normalized peak Y, 1 / Y^2.
    tone: vec4<f32>,
}

@group(0) @binding(0) var picture: texture_2d<f32>;
@group(0) @binding(1) var<uniform> parameters: Parameters;
// Packed RGBA64 sources: exact integer codes, normalized in the shader.
@group(0) @binding(2) var picture16: texture_2d<u32>;

const REFERENCE_WHITE_NITS: f32 = 203.0;
const PQ_M1: f32 = 0.1593017578125;
const PQ_M2: f32 = 78.84375;
const PQ_C1: f32 = 0.8359375;
const PQ_C2: f32 = 18.8515625;
const PQ_C3: f32 = 18.6875;
const HLG_A: f32 = 0.17883277;
const HLG_B: f32 = 0.28466892;
// Intentionally BT.2100's published rounded c. The f64 CPU reference
// (color.rs) derives c = 0.5 - a ln(4a) = 0.5599107295 instead; the two differ
// by 4.7e-10, below one f32 ulp (6e-8) here. The independent qualification
// reference (examples/qualify_hdr_export.rs) also uses the published value.
const HLG_C: f32 = 0.55991073;

@vertex
fn fullscreen(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let positions = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    return vec4(positions[index], 0.0, 1.0);
}

// SMPTE ST 2084 EOTF to cd/m^2.
fn pq_eotf(value: f32) -> f32 {
    let power = pow(clamp(value, 0.0, 1.0), 1.0 / PQ_M2);
    return pow(max(power - PQ_C1, 0.0) / (PQ_C2 - PQ_C3 * power), 1.0 / PQ_M1) * 10000.0;
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
        case 3u: {
            return pq_eotf(value) / REFERENCE_WHITE_NITS;
        }
        case 4u: {
            // HLG inverse OETF to scene light; the OOTF follows the matrix.
            let signal = clamp(value, 0.0, 1.0);
            if signal <= 0.5 { return signal * signal / 3.0; }
            return (exp((signal - HLG_C) / HLG_A) + HLG_B) / 12.0;
        }
        default: { return value; }
    }
}

// Reference-white-preserving tone map on max(R,G,B) (see ToneMap in tone.rs):
// identity up to the knee, then the extended-Reinhard shoulder reaching 1.0 at
// the source peak. Ratio scaling keeps hue.
fn tone_map(rgb: vec3<f32>) -> vec3<f32> {
    let peak = max(max(rgb.r, rgb.g), rgb.b);
    let tone = parameters.tone;
    if peak <= tone.x { return rgb; }
    let y = min((peak - tone.x) / tone.y, tone.z);
    let mapped = tone.x + tone.y * y * (1.0 + y * tone.w) / (1.0 + y);
    return rgb * (mapped / peak);
}

fn interpret_rgba(rgba: vec4<f32>) -> vec3<f32> {
    let linear = vec3(decode(rgba.r), decode(rgba.g), decode(rgba.b));
    var working = vec3(dot(parameters.source_row0.xyz, linear),
                       dot(parameters.source_row1.xyz, linear),
                       dot(parameters.source_row2.xyz, linear));
    if parameters.hdr.z > 0.5 {
        // BT.2100 HLG OOTF, Lw 1000 cd/m^2, gamma 1.2, on Rec.2020 scene light.
        let luminance = dot(vec3(0.2627, 0.6780, 0.0593), working);
        var gain = 0.0;
        if luminance > 0.0 { gain = 1000.0 / REFERENCE_WHITE_NITS * pow(luminance, 0.2); }
        working = working * gain;
    }
    if parameters.hdr.x > 0.5 {
        working = tone_map(working);
    }
    // No clamp in the working transform. Premultiply before resampling to keep
    // transparent color from bleeding into opaque neighbors.
    return working * rgba.a;
}

fn working_texel(position: vec2<i32>) -> vec3<f32> {
    let bounds = vec2<i32>(textureDimensions(picture)) - vec2(1);
    return interpret_rgba(textureLoad(picture, clamp(position, vec2(0), bounds), 0));
}

fn working_texel16(position: vec2<i32>) -> vec3<f32> {
    let bounds = vec2<i32>(textureDimensions(picture16)) - vec2(1);
    let codes = textureLoad(picture16, clamp(position, vec2(0), bounds), 0);
    return interpret_rgba(vec4<f32>(codes) / 65535.0);
}

struct Footprint {
    inside: bool,
    base: vec2<i32>,
    fraction: vec2<f32>,
}

fn footprint(position: vec4<f32>, size: vec2<u32>) -> Footprint {
    if any(position.xy < parameters.coverage.xy) || any(position.xy >= parameters.coverage.zw) {
        return Footprint(false, vec2(0), vec2(0.0));
    }
    let offset = position.xy - (parameters.coverage.xy + vec2(0.5));
    let uv = parameters.sampling_origin_x.xy
        + offset.x * parameters.sampling_origin_x.zw
        + offset.y * parameters.sampling_y.xy;
    let source = uv * vec2<f32>(size) - vec2(0.5);
    return Footprint(true, vec2<i32>(floor(source)), fract(source));
}

@fragment
fn interpret(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let at = footprint(position, textureDimensions(picture));
    if !at.inside {
        return vec4(0.0, 0.0, 0.0, 1.0);
    }
    let top = mix(working_texel(at.base), working_texel(at.base + vec2(1, 0)), at.fraction.x);
    let bottom = mix(working_texel(at.base + vec2(0, 1)), working_texel(at.base + vec2(1, 1)), at.fraction.x);
    // Opaque black background: premultiplied RGB already is the composite.
    return vec4(mix(top, bottom, at.fraction.y), 1.0);
}

// RGBA64 (HDR) sources snap bilinear fractions within 1/1024 of a texel to
// that texel. f32 coordinate error (about 1e-5 at 100 px, 1e-3 at 8K) would
// otherwise leak a 10000 cd/m^2 neighbor into an unscaled black PQ texel.
const TEXEL_SNAP: f32 = 0.0009765625;

@fragment
fn interpret16(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    var at = footprint(position, textureDimensions(picture16));
    if !at.inside {
        return vec4(0.0, 0.0, 0.0, 1.0);
    }
    let upper = at.fraction > vec2(1.0 - TEXEL_SNAP);
    at.base = at.base + select(vec2(0), vec2(1), upper);
    at.fraction = select(at.fraction, vec2(0.0), upper | (at.fraction < vec2(TEXEL_SNAP)));
    let top = mix(working_texel16(at.base), working_texel16(at.base + vec2(1, 0)), at.fraction.x);
    let bottom = mix(working_texel16(at.base + vec2(0, 1)), working_texel16(at.base + vec2(1, 1)), at.fraction.x);
    return vec4(mix(top, bottom, at.fraction.y), 1.0);
}

fn srgb_encode(value: f32) -> f32 {
    let clipped = clamp(value, 0.0, 1.0);
    if clipped <= 0.0031308 { return clipped * 12.92; }
    return 1.055 * pow(clipped, 1.0 / 2.4) - 0.055;
}

@fragment
fn display(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    var working = textureLoad(picture, vec2<i32>(position.xy), 0).rgb;
    if parameters.hdr.y > 0.5 {
        // HDR output: tone-map the composite for this SDR preview only.
        working = tone_map(working);
    }
    let linear = vec3(dot(parameters.display_row0.xyz, working),
                      dot(parameters.display_row1.xyz, working),
                      dot(parameters.display_row2.xyz, working));
    // Rgba8Unorm is intentional: transfer encoding occurs exactly once here.
    return vec4(srgb_encode(linear.r), srgb_encode(linear.g), srgb_encode(linear.b), 1.0);
}

// Caption overlay: red is fill coverage and green outline coverage at the
// exact target raster. The output is premultiplied over the working composite
// (blend One, OneMinusSrcAlpha; destination alpha kept): a black outline,
// then white fill, in linear working light.
@fragment
fn caption(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let coverage = textureLoad(picture, vec2<i32>(position.xy), 0);
    let fill = coverage.r;
    let alpha = 1.0 - (1.0 - coverage.g) * (1.0 - fill);
    return vec4(vec3(fill), alpha);
}
