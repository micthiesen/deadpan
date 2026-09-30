# SDR encoder pixel boundary

`deadpan-render` converts the same composed working picture used by preview
into owned Rec.709 encoder planes. This is a library boundary, not a project
export worker or a Render control. It does not choose project geometry, frame
rate, HDR policy, bitrate, encoder or destination.

## Picture and color ownership

Call the existing `PictureRenderer::render_composed` with the committed plan's
geometry. `begin_working_readback` snapshots that target's linear D65 Rec.2020
`Rgba16Float` texture on the renderer's own queue. Do not use `display_texture`
as encoder input: it has already been clipped and encoded with the sRGB display
transfer. Do not repeat framing or reconstruct it with FFmpeg expressions.

`WorkingRgba16Frame` owns little-endian binary16 RGBA, including final-row
padding. It checks dimensions, stride and exact allocation length. Conversion
rejects nonfinite active channels and alpha other than exactly one. Padding is
never interpreted as a pixel. This boundary expects the canonical opaque
composite; it does not blend alpha or unpremultiply again.

`Rec709Yuv420Frame::from_working` requires even dimensions. It transforms signed
linear Rec.2020 to linear Rec.709 before clipping to the SDR gamut/reference
white, then applies the BT.709 OETF and YCbCr matrix. It retains signed working
values until that output transform. This SDR clipping policy is not an HDR tone
mapper; the host must qualify a different path before accepting HDR sources or
effects that require tone mapping.

The fixed `Rec709LimitedLeft` policy means progressive 8-bit planar Y, Cb, Cr
with square pixels, Rec.709 primaries/transfer/matrix and limited range. Luma
uses codes 16–235; chroma uses 16–240 with neutral 128. Chroma is filtered before
quantization: horizontal weights `[1,2,1]/4` center it on each even luma column,
and vertical `[1,1]/2` centers it between the row pair. Edges clamp. Each code
is quantized once to nearest, ties upward. A centered 2×2 average must not be
substituted while retaining the left-sited metadata.

The resulting I420 storage is tight, with the complete Y plane followed by Cb
and Cr. Plane/stride getters and the policy make the encoder adapter's required
interpretation explicit. The [committed picture host](EXPORT_PICTURES.md)
supplies rational output timestamps separately; source PTS is not an edited
output clock.

## Work and lifetime bounds

The existing dimension and pixel bounds still apply. Working storage has a
separate limit of 128 MiB of pixels plus 2 MiB of padding. Readback owns one
bounded staging buffer and then a bounded CPU copy; conversion retains two
chroma rows plus its output allocation. The host owns the number of completed
frames it retains.

One renderer admits one outstanding working readback. A ticket polls without
a blocking device wait. Its fixed monotonic deadline and caller cancellation
are checked around polling and copying. A completed or failed ticket cannot
be reused. Dropping or cancelling it unmaps its buffer, but submitted GPU work
still owns the permit until both the map and queue-completion callbacks drain.
Repeated cancellation therefore cannot enqueue unbounded staging allocations.
Normal device polling or a later begin attempt drains callbacks.

These controls are cooperative, not a way to preempt a driver call. The host
must run readback and CPU conversion on preparation/export workers, preserve
its immutable frame identity, bound completed-frame retention and reject late
results. The pixel converter itself is bounded synchronous work, with no
internal thread, device or job queue.

## Qualification entry point

`crates/deadpan-render/examples/qualify_sdr_export.rs` renders actual Metal
working targets and compares every output code against an independent f64
reference from known linear Rec.709 RGBA8 input. The fixtures cover color and
near-knee neutral patches, horizontal/vertical chroma patterns, 320-pixel rows
and padded 318-pixel rows, plus readback lifecycle failures.

The example accepts a new report path and a new fixture directory. It writes
actual and reference I420 separately, with byte counts and SHA-256 identities
in the report. Only the actual renderer planes may enter the encoder check.
The reference comparison and the later lossy encode/decode comparison are
distinct measurements; correct tags alone do not qualify either one.

The [measured qualification](qualification/sdr-encoder-pixels-2026-09-28.md)
records 22 real Metal checks, exact equality across all 172,260 reference codes,
and the normal/sanitized hardware H.264 round trip, including complete planes
and the initial compile failures.

The full immutable project renderer, media provider ownership, audio mastering,
AAC/mux policy, encoded-file verification, atomic publication, native workflow,
HDR and complete resolution/OS qualification remain separate required work.
