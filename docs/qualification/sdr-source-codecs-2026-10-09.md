# SDR HEVC and H.264 High10 sources, 2026-10-09

DP-02 / DP-16, specification §§4 and 16.3. This qualifies SDR HEVC Main
eight-bit, HEVC Main10 ten-bit and H.264 High10 ten-bit 4:2:0 in the admitted
MP4 grammar. It does not close the required codec/profile or source/operation
matrix. Implementation, review and verification are owned by the sole agent.

## Source and seek behavior

HEVC configuration admits Main/Main10, equal eight/ten-bit luma and chroma,
4:2:0 and the existing bounded single-layer grammar. Main requires eight bits.
Every SPS profile and depth must match `hvcC` before native decoding; geometry,
NAL, packet and resource limits remain checked. The existing explicit SDR
range, matrix, transfer and primaries policy applies. Static HDR metadata on
SDR remains refused, as do unqualified profiles, depths and layouts.

Ten-bit SDR uses the existing RGBA64 conversion through source sessions and
the shared picture renderer. Matrix and range are applied in double precision
with one output rounding; source transfer and primaries are retained. Original
ten-bit low codes are not quantized to RGBA8 before rendering. The automatic
output branch remains SDR Rec.709. Eight-bit full-range HEVC's decoded
`yuvj420p` name is retained and validated. Core 48, SQLite 76 and the source
receipt wire format are unchanged.

Repeated seeks exposed an existing pinned FFmpeg 8.0.3 HEVC failure. Its
`hevc_decode_flush` clears the DPB but leaves `output_fifo`; `hevc_receive_frame`
can return that queued picture before reading a new packet. After stopping at
PTS 20020, seeking to 18018 returned the previous sequence's 22022 picture.
A fresh decoder returned the correct sequence. The retained packet/frame trace
is `/tmp/deadpan-sdr-hevc-seek-trace-20261009.log`.

Before flushing HEVC, the adapter now drains queued output through the public
codec API, including frame-thread output. It reads no new source packets,
checks the cooperative deadline/cancellation, counts discarded decode work and
fails after 512 API steps rather than retaining stale state. The persistent
decoder stays open. Sequential and repeated backward seeks match at 1, 8 and
16 codec threads for the new SDR fixtures and existing HDR closed/open GOPs.

Review found a second integration failure in proxy verification: its block
comparison assumed RGBA8 and read RGBA64 low/high bytes as different pixels.
It now reads complete little-endian channels and normalizes the block sums
to the proxy's eight-bit scale, preserving low bits until the mean. A padded
two-row independent vector fails against the old production helper and passes
against the fixed helper; both extracted sources, binaries and results are
retained in `/tmp/deadpan-sdr-proxy-{before,after}-20261009`.

## Fixtures and failures

The [manifest](../../native/deadpan-source/tests/fixtures/manifest.json) pins six
new files and their development-only FFmpeg 9.0.1/libx265/libx264 producer.
`generate_sdr_hevc_fixtures.py --check` reproduces their retained bytes. Each
has 12 independently authored 96x64 pictures at 30000/1001, two B frames, IDR
every four pictures, explicit BT.709 and left chroma siting, and unchanged
AAC from the hash-pinned `fields-bff.mp4`. Its exact endpoint is sample 19219.
HEVC is lossless; High10 uses QP 1. Full/limited black, gray, colored and white
patches plus moving lower rows provide independent pixel and temporal evidence.

The first producer invocation omitted raw-input color tags, so development
FFmpeg applied a matrix conversion before encoding. Independent authored-code
assertions caught it. Matching explicit input/output tags fixed the fixtures;
the initial bytes and failed logs remain retained. The first receipt test then
caught the `yuvj420p` mismatch; the validator now admits that full-range
eight-bit HEVC interpretation. Two old negative tests were updated to keep
rejecting unsupported codecs and twelve-bit pixels.

Review of the emitted-file reports also found that the initial ten-bit motion
had 22 unobservable neighboring-picture comparisons per movie. Its low-code
precision checks were useful, but its motion contrast was too small to prove
encoded picture identity. The final generator gives both depths the same
visible motion while retaining low codes. The permanent export test now
requires 22 observable comparisons and zero unobservable comparisons per case.
The earlier files, generator and hashes are retained in
`/tmp/deadpan-sdr-codecs-before-temporal-20261009`.

The previous packaged CLI, source `a6b0acd114ad036abfcde6accccc018d21804062`,
SHA-256 `de89cb16080cb68a29c8b86f8fedbfaa7a858d4774b3edca65ac1f6806ac368a`,
refuses every final fixture. Eight-bit HEVC fails the old Main10-only guard;
ten-bit HEVC/H.264 fails the old SDR depth guard. Fresh-home commands, final
fixture hashes and exact errors are retained in
`/tmp/deadpan-sdr-codecs-before-verified-20261009`.

## Verification

Apple M5 Max, macOS 26.5.2 (25F84), Rust 1.97.1, pinned LGPL FFmpeg 8.0.3.

- Native source suite: 128 passed. New tests independently check BT.709
  range/matrix values, retained ten-bit low codes, exact B-frame PTS/durations,
  thread equivalence and repeated seek identity, including HDR regressions.
- Source qualification: 18 passed. All six files retain their exact clocks,
  automatic project basis, audio endpoint and picture depth after receipt
  serialization. Unsupported forged codec/depth combinations remain refused.
- Focused debug and final release public Render each pass six cases, 72 pictures
  and six signal-bearing audio windows, exact endpoints and zero measured offset. Every
  Original ordinal is checked. All 132 neighboring-picture comparisons are
  observable, with none skipped as unobservable. All outputs retain SDR
  Rec.709 color tags.
- Locked workspace with `deadpan-app/ui-harness`: 5,567 passed, including two
  documentation tests; 12 existing opt-in tests were ignored. Strict all-target
  Clippy passed with and without the UI harness.
- The separate source ASan/UBSan build passed all 128 tests. Native C adapters
  and target C dependencies were instrumented; Rust and the pinned FFmpeg
  libraries were not.
- Final media suite: 140 passed, including its documentation test. All 14
  real-worker proxy tests passed. The strengthened six-case proxy rerun retains
  every PTS/duration and passes the unchanged fidelity thresholds; actual
  proxy movies and sidecars are retained.

The workspace build preceded the proxy-depth comparison fix; final media/proxy
checks, strict lint, release and packaged checks include it. The initial native,
workspace, sanitizer and proxy suites used the earlier motion fixtures. Focused
source, receipt, proxy, debug/release Render and sanitized source cases rerun
against the final fixtures preserve that distinction in their logs and hashes.

The self-contained bundle passed relocation, strict signatures, native smoke
launch, real full-range Main10 import/Render and helper-tamper/missing-resource
refusals. A separate packaged run used a fresh home, unrelated working directory
and only `/usr/bin:/bin` on PATH. It imported the final full-range Main10
fixture, rendered and ran `verify-export`: 12 pictures at 30000/1001, 22
observable neighboring-picture comparisons, no unobservable comparisons,
sample endpoint 19219 and zero measured offset. Doctor confirmed that the
loaded FFmpeg libraries came from the bundle. This is evidence on this Mac.

Consolidated logs, previous/final fixtures, reports, movies, source hashes and
executed binary hashes are retained in `/tmp/deadpan-sdr-codecs-final-20261009`.
The instrumented target and signed bundle remain in
`/tmp/deadpan-sdr-codecs-source-asan-20261009` and
`/tmp/deadpan-sdr-codecs-bundle-20261009/Deadpan.app`. The relocation log records
its separately retained verification directory. DP-02 and DP-16 remain partial.

## Remaining scope

VP9, AV1, ProRes, broader container/profile/audio support and the complete
source/creative-operation matrix remain required. Wide-gamut, BT.601 and
other source policies need their own end-to-end evidence. The existing AI
conditioning policy still explicitly refuses sixteen-bit decoded inputs;
this change does not qualify deep-SDR model conditioning. It does not establish
large-source performance, clean-machine support or physical-display color.
