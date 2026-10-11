# Optional VideoToolbox source decoding, 2026-10-11

The persistent source adapter now implements an opt-in VideoToolbox decoder
through the existing pinned FFmpeg 8.0.3 libraries. H.264, HEVC and VP9
progressive 4:2:0 eight/ten-bit pictures preserve the software reference's
pixels, source interpretation and presentation clocks.

**Normal preview and playback keep threaded software.** Long-GOP measurements
on this Mac found the VideoToolbox path substantially slower, even when preroll
does not download pixels. The implementation is available through
`DecodeLimits::acceleration = DecodeAcceleration::PreferVideoToolbox` for
qualification and explicit backend callers. It adds no user-facing toggle and
does not close DP-16 or DP-24. Fast hardware-backed application decoding remains
open; correctness alone does not qualify a slower interactive default.

## Boundary and ownership

- The same admitted descriptor, bounded demuxer, packet guards and persistent
  codec context own decoding. There is no per-frame process or decoder reopen.
  Coded/visible geometry is checked before hardware format selection.
- Unsupported device/format initialization falls back through FFmpeg's
  software format negotiation before returning a picture. Errors after decode
  begins poison the session normally; there is no silent replay, frame discard
  or replacement of a failed hardware picture.
- H.264 field/repeat cadence, unknown/non-frame-only HEVC constraints, RGB,
  AV1, ProRes and FFV1 use software. HEVC eligibility checks both hvcC and every
  SPS. Its result is captured before codec threads start; callbacks never read
  the live demuxer, caller cancellation pointer or error buffer.
- A hardware frame keeps its actual surface format and ownership. The public
  codec `sw_pix_fmt` supplies its logical interpretation, including full-range
  `yuvj420p`. Receipt metadata gains no device-dependent field.
- Pixel access downloads one NV12/P010 surface and unpacks exact codes into
  owned, bounded planar buffers. It does not rescale, reinterpret range or
  resample chroma. Both RGBA converters then consume the same layout as software.
  Repeated copies reuse the download; metadata-only preroll performs none.
  Next, seek and restart discard the cached CPU pixels.
- Software buffer allocations, returned VideoToolbox frames and downloaded
  surfaces have separate counters. VideoToolbox is not a claim about HEVC's
  physical engine: the pinned FFmpeg adapter allows Apple's software fallback
  for that codec. It requires hardware for its H.264/VP9 route.
- Complete and background receipt measurement always use one software codec
  thread, including when an explicitly configured serving decoder uses hardware.
  A one-thread hardware serving request opens its own decoder instead of
  accidentally reusing the software measurement session.

## Correctness checks

`native/deadpan-source/tests/hardware_decode.rs` exercises actual macOS decoders:

- 29 source fixtures at one and 16 codec threads: **2,208 pictures**, each with
  complete RGBA8 and RGBA16 equality against sequential software. This includes
  B-pyramids, offset/VFR clocks, codec padding, integer/fractional clean aperture,
  limited/full range, HEVC open GOP, PQ/HLG, VP9 altref/show-existing pictures,
  top-left chroma, anamorphic pixels and MP4/WebM metadata.
- **864 backward seeks** across six fixtures, checking every target and up to
  two following pictures. Metadata preroll never increments downloads.
- Ten unsupported hardware/field fixtures produce **194 identical software
  pictures**. A separate weakened HEVC frame-only declaration also stays in
  software without changing its pictures.
- Geometry/frame limits, pre-call cancellation, repeated copies, retained
  current pictures and poisoning after a decode failure remain enforced.

The media integration test opts serving into VideoToolbox at one/16 threads,
verifies independent software receipts, compares requested pictures, and
rejects a valid-but-tampered index after background measurement.

Final source/media tests, strict workspace Clippy, native ASan/UBSan and the
long-clip comparison are recorded in the evidence manifest below. The sanitizer
scope covers the native C adapter and target C dependencies; Rust, FFmpeg and
Apple's frameworks are not instrumented. No live GUI, listening, physical
display or second-Mac check is claimed.

## Longer clips and latency

The reusable `deadpan-media` example `compare_decode` first hashes every
canonical software picture, then compares complete sequential output and 100
random seeks per serving path. Reports retain source hashes, per-picture
reference hashes/PTS/durations, target/anchor pairs, raw timings and actual
decoder work. Timings include requested pixel copy and hashing, without GPU,
proxy or audio work. A source hash check at the end refuses a changed input.

Four deterministic clips cover 1,800 H.264 1080p60 pictures, 600 H.264 4K30
pictures, 240 HEVC 4K30 pictures and 240 VP9 4K30 pictures. Fixture generation
uses the separately identified Homebrew FFmpeg 9.0.1 CLI; decoding uses the
pinned LGPL 8.0.3 libraries. This does not change the shipped FFmpeg prefix.

| Source | Software seek p95 | VideoToolbox seek p95 |
| --- | ---: | ---: |
| H.264 1080p60 | 48.941 ms | 436.169 ms |
| H.264 4K30 | 163.113 ms | 863.560 ms |
| HEVC 4K30 | 186.789 ms | 1019.475 ms |
| VP9 4K30 | 616.927 ms | 1624.747 ms |

All **5,760 sequential picture comparisons and 800 random seeks** pass exact
pixels and PTS/duration. Each case began below load 4, with no concurrent build
or heavy qualification task. The same-file software comparison runs before
VideoToolbox, after a complete reference scan, so input pages are warm.

Final release comparison binary SHA-256:
`95f2a710da91e40468b03f2c75d62056eb7390c8c0d1dc2b80353202698551ba`.
Source identity is base `9479499f92e654b8ab0cc40e030a4894b84d32b5` plus the
nine final source/test hashes in `final/identity.json`; those files are committed
with this record. This is Apple M5 Max, 128 GiB, macOS 26.5.2 and Rust 1.97.1.

The 339-test source/media regression run passes, followed by the final focused
admission and five native hardware tests. All 182 native sanitizer tests pass;
strict workspace/all-target Clippy and rustfmt checks pass. A late test edit
initially failed rustfmt-only checking; it was formatted and the final check
passes. No behavior or assertion failure was suppressed.


The pinned VideoToolbox adapter waits for asynchronous frame completion after
each submitted coded picture. That is consistent with the measured long-GOP
cost; it is an explanation from source inspection, not an isolated causal
benchmark. The SDK documents realtime decoding as the default, so merely
setting that property is not evidence of a fix. No private FFmpeg/VideoToolbox
session ABI, upstream patch or entitlement change was introduced.

## Evidence and retained failures

Evidence is collected under
[`2026-10-11-hardware-decode`](../../tools/media-qualification/evidence/2026-10-11-hardware-decode/README.md).
It includes binary/source identities, raw logs, final reports, fixture recipes
and hashes, prototype comparisons and the sole agent's review notes. Large
generated fixtures and executed binaries remain in
`/tmp/deadpan-hardware-decode-20261011`; the manifest identifies them exactly.

The first public-API probe falsely differed on full-range HEVC because its
normalization let swscale change YUVJ range. The corrected probe set source and
destination range explicitly and matched; production instead unpacks codes
without swscale. An initial example build used an unsupported SHA-256 hex
formatter and was corrected. Both failures and the successful reruns are
retained. The VideoToolbox latency failures are retained as failures, not
reclassified as meeting interactive targets.
