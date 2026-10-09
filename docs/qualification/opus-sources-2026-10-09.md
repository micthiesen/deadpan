# Opus source audio, 2026-10-09

DP-02 / DP-16, specification §§16.3 and 28. Finite WebM/Matroska now admits
mono/stereo Opus audio, alone or beside qualified VP9/Matroska video. Original
creation and sound registration use the normal immutable receipts, exact
sample mappings and shared playback/Render path. This does not complete the
remaining source formats or the source/operation matrix. Implementation and
review belong to the sole agent.

The native Add sound picker includes `.webm`, `.mkv` and audio-only Matroska's
`.mka`; admission still checks the bytes, independent of extension.

## Container admission and sample time

The bounded EBML guard inventories all admitted tracks before FFmpeg opens the
file. It supports one optional video and up to 32 Opus tracks, preserving entry
order as stream identity. Audio-first and multiple-audio files need no guessed
stream index. Every track must have packets. Seek and cue targets must identify
actual boundaries and declared tracks. Finite sizes, metadata/packet budgets,
one CueTrackPositions per CuePoint and the existing no-lacing restriction remain.

Opus requires a 19-byte, version-one, mapping-family-zero header, matching
mono/stereo channel declarations, explicit 48 kHz input/container rates,
80 ms preroll, and CodecDelay within strictly less than one nanosecond of the
exact pre-skip. Timestamp ticks may be no longer than one millisecond. A bounded
[RFC 6716 packet parser](https://www.rfc-editor.org/rfc/rfc6716.html#section-3)
checks frame lengths, VBR/CBR framing, padding and the 120 ms packet limit before
native allocation. The [Matroska Opus mapping](https://www.matroska.org/technical/codec_specs.html#a_opus)
and [Opus header](https://www.rfc-editor.org/rfc/rfc7845.html#section-5.1)
define the retained metadata; unqualified extensions fail explicitly.

Raw container PTS and reported durations stay in the receipt. The first Block
must name an integral 48 kHz sample; subtracting pre-skip establishes the physical
decode origin. Decoded sample counts supply subsequent exact positions. Each
raw PTS is checked against this origin within one container tick, accounting
for FFmpeg's rounding of CodecDelay to ticks. This is an explicit continuous
sample-clock interpretation of quantized metadata. A deliberate gap inside
that tick envelope cannot be distinguished from quantization. No rounded duration accumulates,
no event alignment runs, and no packets or samples are inserted to conceal a
failure. Drift outside the tick envelope fails.

Manual skip evidence must match the header. Leading pre-skip can span packets;
explicit terminal DiscardPadding must represent a positive whole-sample count
with strictly less than one nanosecond representation error. It removes only
that declared count. Negative or ambiguous fractional-sample padding fails. Extra
leading skips, interior trailing skips, discard flags, mismatched packet/sample
counts and contradictory durations fail. Container duration cannot replace the
measured endpoint. Receipt deserialization reconstructs and rechecks all derived
coordinates. Existing AAC/PCM timing and export tolerances are unchanged.

## Decoder choice and retained failures

FFmpeg 8.0.3's native Opus decoder passed the CELT fixtures but disagreed with
the independent SILK and hybrid reference. SILK peak error was 0.100519 and RMS
error 0.0104295; the retained comparison also found a hybrid discrepancy. These
failed logs and measurements remain in `/tmp/deadpan-opus-20261009`; failed PCM
scratch dumps were subsequently overwritten, so they are not retained evidence.
No alignment or tolerance increase was used to admit the result.

The selected backend is [libopus 1.6.1](https://opus-codec.org/downloads/), built
from SHA-256
`6ffcb593207be92584df15b32466ed64bbec99109f007c82205f0194572411a1`.
The isolated builder links its static PIC library into LGPL FFmpeg libavcodec,
disables FFmpeg's native Opus decoder and the libopus encoder, and retains
source/configuration/library identities. Neural extensions are disabled.
All 16 upstream Opus tests pass with zero skips. An earlier prototype disabled
extra programs and ran zero tests; it was superseded by the fresh qualified
build, not counted as upstream-test evidence.

The native adapter explicitly selects `libopus` and requests interleaved float
output. A first reference accidentally requested the wrapper's default signed16
output; regeneration with float output removed that reference quantization.
The ten initial real fixtures then matched the independent reference floats
exactly, including SILK and hybrid. The later cross-packet pre-skip fixture also
passes the unchanged peak/RMS bounds. Opus's declared header gain is handled by
the codec; the host adds no gain, clipping or resampling at this boundary.

FFmpeg's parser logs `Error parsing Opus packet header` for its empty EOF buffer.
The pinned source's complete-frame parser explains this diagnostic. Every real
packet is independently checked and the complete packet/sample count must match
at EOF; no global log suppression or corrupt-packet acceptance was introduced.

The final prefix is
`~/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-qualified/prefix`.
The previous prefix remains intact. Build and runtime checks reject a prefix
without the selected decoder configuration. The bundle includes Opus's full
BSD notice and source pin in its SBOM, with no external Opus dylib dependency.

## Fixtures and regression evidence

The generator and `opus-manifest.json` retain twelve repository-authored WebM
inputs and independent float PCM references. Development FFmpeg 9.0.1/libopus
1.6.1 produces these independently of the shipped FFmpeg 8.0.3 build.
`generate_opus_fixtures.py --check` reproduces their bytes. Inputs exercise
2.5/20/60/120 ms packets, mono/stereo, CELT/SILK/hybrid, VP9 with Opus, delayed
audio, audio-first order and two audio tracks. The pre-skip fixture changes the
header and exact CodecDelay from 120 to 600 samples; five complete packets are
unavailable and the remaining length is 7717 samples. Other clips retain 8197
samples. The delayed audio starts at sample 6048, reflecting the actual muxed
126 ms Block timestamp, rather than the requested 125 ms offset.

A separate +2 dB Opus header-gain fixture checks amplitude preservation. Its
independent reference is compared with the unmodified header's PCM multiplied
by `10^(2/20)`, with peak error below 0.000002, before the native decoder is
compared against it. The manifest retains the signed Q8 gain value.

Native tests compare manual trimming with ordinary decode, both against the
independent PCM, and assert the selected codec, metadata, packet sizes, track
order and all twelve VP9 pictures. Media tests check exact available boundaries,
unavailable priming/padding, receipt round trips and forged delay/drift refusals.
A 100,000-packet synthetic index proves origin-based arithmetic over 12 million
physical samples, including cross-packet pre-skip and terminal trim. Container
and decoder mutation campaigns now include Opus-bearing EBML inputs.

Late review found that ambiguous fractional-sample DiscardPadding could be
rounded silently by the native demuxer. A retained pre-fix worktree binary
accepts a real fixture whose padding was changed from 2,729,167 to 2,730,167 ns.
The new guard requires a whole-sample count with sub-nanosecond representation
error. A regression covers both neighboring nanoseconds of one sample,
ambiguous fractions, negative padding and the maximum bound. This change
postdates the initial workspace binary and is covered by the final native and
release checks below.

The prior release CLI from `21372b19d5c53a921766bd0f045a6aad5f5f7f39`, SHA-256
`a6daa6704e2c64ac092e4588a659826558bdb13be3ddc9d93da66f3755f6010b`, refuses
all four audio-bearing Original fixtures. Its executed bytes and refusal report
are retained. No archived source was built into the current Cargo target.

## Final verification

Implementation source: `f7b3ada440ffdcb2d351d3653050f30a3215dff1`. The retained source/binary map is
`/tmp/deadpan-opus-20261009/identity.json`.

Apple M5 Max, macOS 26.5.2 (25F84), Rust 1.97.1. Detailed commands, reports and
failures live in `/tmp/deadpan-opus-20261009`; the pinned dependency build report
is `ffmpeg-qualified-build.json`.

The full workspace run completes with 5,550 passes, one proxy-test failure and
12 existing ignored qualification/stress/network checks. The proxy test reused
an output containing a stalled attempt's partial tail; the production resume
path already truncates that tail after confirmed worker teardown. A new
deterministic stalled-worker regression reproduces the exact failure. Both
range-test staging closures now follow production cleanup. All 16 proxy tests
then pass, including the original failing assembly and the new regression.
Initial formatting and a Clippy `div_ceil` finding were also repaired; their
failed logs remain available. Final gate stages, sanitizer, release and packaged
results are recorded below.

The UI harness passes all 1,097 tests, with two existing ignored checks, and
workspace documentation tests pass. Strict Clippy passes for the workspace and
UI harness, plus final lint of the native source, proxy worker and app targets.
ASan/UBSan passes all 142 native source tests, including the late padding
guard and twelve Opus fixtures. Instrumentation covers native C adapters and
target C dependencies; Rust, FFmpeg and libopus are not instrumented. The report
and binary hashes are retained in `/tmp/deadpan-opus-source-asan-20261009`.

The final optimized public creation/Render test passes all four WebM Originals:
48 pictures with independent fixture-derived provenance and four signal-bearing
audio windows at zero measured offset. Existing AAC tolerances remain unchanged;
the emitted reports retain declared priming of 2048 samples and all content
measurements. Evidence is in
`/var/folders/37/7y1z4kxn5mv9bkbckqxrnzy40000gn/T/.tmprFGUAS`.
The executed release CLI is retained in the scratch evidence with SHA-256
`080118d3b18356cdf84dd7a26798ed722d840c86f6808e506c158d0a14db4944`.

That final CLI rejects the fractional-padding counterexample with
`unqualified Matroska DiscardPadding`. The pre-guard worktree binary from the
initial workspace build, SHA-256
`a1166c49a8a05b1ca7b79b3a0dd0494db866528c6b72d115268b0261a513f02e`,
accepted those identical bytes. Both binaries and command results are retained.

The ad-hoc app bundle builds and passes relocation, native/AI runtime smoke,
real Opus Original import and negative integrity checks. All 75 Mach-O files
pass its dependency audit. The Opus SBOM entry has the pinned archive hash;
the installed `opus/COPYING` SHA-256 is
`01e1167d54a096d123cf6dfbbeb19587278845c6481d2d66d545669846079551`.
The complete bundle is `/tmp/deadpan-opus-bundle-20261009/Deadpan.app`.

A packaged run with an isolated home, unrelated working directory and
`PATH=/usr/bin:/bin` creates, renders and independently verifies the four WebM
Originals plus a real Matroska variant: 60 pictures and five signal windows at
zero measured offset. It registers all eight standalone sounds, including
SILK, hybrid, cross-packet pre-skip and header gain, without changing timeline
nodes, then validates the complete project. Reports and exported files remain
under `/tmp/deadpan-opus-20261009/packaged`. The packaged CLI SHA-256 is
`3832ea33b18bd0d2cdf9deec9c0799f5479b305712d432f8d4f09ce2004849a2`.

The first packaged harness used `mono-2.5` as a typed revision ID, which correctly
failed the identifier grammar. The harness now uses a valid ID while retaining
the original label; only unfinished actions were resumed. No product validation
was weakened. Final rustfmt and diff whitespace checks pass. Fixture manifests,
reviewed source hashes, binary identities and command results are retained in
the scratch evidence's `identity.json`.

Ogg Opus, mapping families above zero, broader Matroska grammar, arbitrary input
rates, unqualified video/audio codecs and the full editing/analysis/AI operation
matrix remain open. Listening and physical-device judgments follow the owner's
verification scope. DP-02 and DP-16 remain partial; overall progress stays 89%
with five of twenty-four sections complete.
