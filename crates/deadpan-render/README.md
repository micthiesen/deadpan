# Shared picture baseline

`PictureRenderer` uses the host's existing wgpu 30.0.1 device and queue. It owns
one reusable upload texture and admits one picture submission at a time. A busy
renderer returns `RenderError::Busy`; the host keeps only its newest pending
request and retries after device polling. It never waits for the GPU in
`render()`, creates a device per frame, or owns an unbounded work queue.

`Rgba8Frame` owns progressive, full-range, straight-alpha RGBA8 bytes, including
explicit row padding, color interpretation, sample aspect, clockwise right-angle
rotation, and the original exact `deadpan_core::SourceTimestamp`. Dimensions,
pixel count, row stride, exact buffer length, and bytes are bounded before GPU
upload. The source adapter must explicitly convert YUV/range and reject
unsupported/missing interpretation; constructing a surface is not color
qualification. This crate cannot represent PQ, HLG, premultiplied input, YUV,
interlaced input, native pixel buffers, or imported GPU textures.

The same WGSL and geometry run for every `RenderTarget`, whether the caller is
previewing or reading pixels for an offline render:

1. Upload source as `Rgba8Unorm`, so hardware never implicitly decodes transfer.
2. Inverse sRGB, inverse BT.709 OETF, or explicit linear RGB, followed by the
   named Rec.709, Rec.2020, or Display P3 D65 conversion to linear Rec.2020 D65.
3. Apply sample aspect and rotation, then centered fit/fill. Bilinear sampling
   decodes each texel before interpolation and premultiplies alpha before
   filtering. The composite background is opaque black.
4. Store the composite in `Rgba16Float` without normalized range clipping.
   Negative and above-reference values retain binary16 precision and range.
5. Transform working RGB to linear Rec.709, clip to the SDR display gamut and
   reference white, and explicitly encode sRGB into opaque `Rgba8Unorm`.

The display view has stable lifetime in its `RenderTarget` and can be registered
with egui. Keep the target alive until registration is released. Do not mark the
encoded output as an sRGB texture and apply transfer encoding again. The host
must choose its compositing/display integration accordingly. The working and
display textures allow `COPY_SRC`; readback on the same queue is ordered after
the render submission. Targets from another renderer are rejected.

Color definitions follow [W3C's named D65 RGB transforms and sRGB transfer](https://www.w3.org/TR/css-color-4/#color-conversion-code) and the
[BT.709 OETF](https://www.itu.int/rec/R-REC-BT.709). The CPU reference performs
separate f64 RGB-to-XYZ and XYZ-to-working/display operations; the shader uses
f32 composed matrices and a binary16 intermediate. Unit tests include independent
numeric primaries, transfer, geometry, and compositing anchors.

Run headless logic checks with `cargo test -p deadpan-render --locked`. Run actual
Metal offscreen qualification explicitly on macOS:

```sh
cargo run -p deadpan-render --example qualify_picture --locked -- /tmp/picture-report.json
```

The report path must be new. The executable fails on unavailable/wrong hardware,
mapping timeouts, invalid pixels, and numerical mismatches. Every output pixel
in each synthetic fixture is compared with a CPU reference; the tolerance is two
8-bit channel codes to account for f32/binary16 error. A separate working-texture
readback checks a negative Display P3 red coordinate with 0.0005 absolute tolerance.
The fixture suite includes odd dimensions, padded source and readback rows, all
supported color/rotation/fit combinations, non-square samples, color patches,
black bars, partial/zero alpha, and size rejection. Report timings include CPU
submission and GPU readback overhead and are not playback performance evidence.

This foundation does not establish full DP-16/DP-17 or Gate A completion. HDR
inputs, tone mapping, color-correct physical display/ICC integration, editorial
effects, attachments, encoder pixel conversion, full-quality downsampling,
timeline playback, output-file verification, native zero-copy ownership, and
resource-contention benchmarks remain separate work.
