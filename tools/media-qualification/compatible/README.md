# Compatible native media boundary

The builder supplies the application's pinned LGPL FFmpeg 8.0.3 libraries,
with static BSD libopus 1.6.1. It verifies the signed FFmpeg release and the
pinned Opus archive, runs the upstream Opus tests, and disables FFmpeg's native
Opus decoder. The separate developer qualification harness runs the pinned
rsmpeg revision against the original numbered-picture and audio impulse
fixtures. Gate A and export acceptance remain open.

Prerequisites: Apple Silicon macOS, Python 3.12+, Xcode command-line tools,
`make`, `git`, `gpg`, `pkg-config`, and the repository's Rust 1.97.1 toolchain.
Python, GPG, Cargo, and `ffprobe` are developer tools, not end-user requirements.
No Homebrew FFmpeg libraries or external H.264 encoders are used by this build.

From the repository root:

```sh
python3 tools/media-qualification/compatible/build.py \
  --output /tmp/deadpan-media-compatible-build.json
python3 tools/media-qualification/compatible/qualify.py \
  --build-report /tmp/deadpan-media-compatible-build.json \
  --output /tmp/deadpan-media-compatible-qualification.json
python3 tools/media-qualification/compatible/qualify.py \
  --build-report /tmp/deadpan-media-compatible-build.json \
  --sanitizers --output /tmp/deadpan-media-compatible-sanitizers.json
```

The build creates a fresh `/tmp/deadpan-media-compatible-*` prefix, verifies the
archive, detached signature, release key bytes, and signer fingerprint against
`pins.json`, and installs only into that prefix. An optional `--download-cache`
directory may supply the pinned download files; hashes and signature are
still verified. `--work` must name an empty directory. The default build uses
eight jobs, and the report retains configure arguments, command logs/hashes,
versions, license text, dylib hashes/linkage, and deployment load commands.
The release archive checksum is the authoritative source input; the recorded
Git tag/commit is a source reference, not a claimed archive/tree comparison.

Opus is built as PIC into a private prefix, with neural extensions disabled and
extra programs enabled so `make check` actually runs the 16 upstream tests.
FFmpeg's pkg-config search is restricted to that prefix; libopus is statically
linked into libavcodec, with no new runtime dylib. Its source, build flags,
test log and license hash are retained in the report. See the
[Opus qualification](../../../docs/qualification/opus-sources-2026-10-09.md).

The qualification script fetches the exact rsmpeg commit into a fresh scratch
Git repository and extracts a committed-source archive. Its sibling source
snapshot is the `../rsmpeg` dependency in the isolated probe manifest. The
checked-in `Cargo.lock` is copied as an input; every build, metadata query, and
Clippy invocation uses `--locked`. Existing upstream checkouts, untracked
additions, ignored artifacts, and root workspace dependencies are not used.

The C wrapper includes the existing original native fixture harness unchanged.
It adds the explicit MP4 `movie_timescale=240000` option to preserve rational
video ticks and audio offsets, and retains a default-timescale negative control.
The Rust probe independently reads authored picture numbers, checks exact PTS,
builds a frame/keyframe index, compares every seek with linear decode, and checks
a retained AVFrame after decoder/demuxer destruction. A swapped-picture fixture
must fail even though its timestamps are unchanged.

The process exits nonzero for unexpected failures. The known hardware CFR
B-frame mux failure, software VFR B-frame mux failure, and VFR terminal-duration
loss are explicit negative capability expectations. In the VFR case the Rust
probe itself emits its full seek evidence and exits nonzero on the authored
duration discrepancy. The orchestration succeeds only if these exact failures
remain visible and the required working configurations pass. A future fix that
changes one of these negative results intentionally requires updating the
expectation and requalifying it; success is not silently inferred.

`--sanitizers` instruments the C fixture/adapter with ASan and UBSan. It does
not rebuild FFmpeg, Rust, or Apple frameworks with sanitizer instrumentation.
Recorded timings are single-run observations on tiny warm-cache fixtures, not
performance targets or OS compatibility claims. Binary hashes identify the
measured build; fresh scratch paths and toolchain details mean byte-identical
binary reproducibility is not promised.

See [measured results and limits](../../../docs/qualification/media-compatible-2026-09-20.md).

## SDR encoder timing experiment

`qualify_encoder.py` reuses an existing successful pinned build receipt and
requires an empty scratch directory. It does not fetch rsmpeg or rebuild FFmpeg:

```sh
python3 -m unittest discover -s tools/media-qualification/compatible -p 'test_*.py' -v
python3 tools/media-qualification/compatible/qualify_encoder.py \
  --build-report /tmp/deadpan-media-compatible-build.json \
  --work /tmp/deadpan-encoder-NEW \
  --output /tmp/deadpan-encoder-NEW-report.json
```

The first five cases compare explicit hardware/software and zero/two requested
B-frames, with the default edit-list reference and `use_editlist=0` candidates.
A captured no-B path then receives edge-content, 60 fps, one-frame and exact
AAC-block-boundary fixtures. Default and disabled-edit-list pairs retain their
encoded packet hashes. Every case is recorded independently; no case silently
switches encoder or repairs timestamps.

The separate Python oracle uses exact rational frame times, origin-based
ties-to-even audio allocation, known picture identities, independent Rec.709
patch values and absolute decoded sample coordinates. Ordinary and manual-skip
AAC decoding both drain to EOF. Exact-event checks remain visible diagnostics
beside the declared encoded tolerance, which must be strictly below one output
frame. The MP4 inspector traverses bounded structural boxes; it never searches
arbitrary payload bytes for `elst`. Its fast-start observation alone is not a
media-validity result.

The report keeps full command logs, before-mux and demuxed packets, decoded
frame metadata, PCM/MP4 hashes, actual byte/linkage admission and final file
rechecks. Unknown command failures, timeouts, crashes and sanitizer failures
fail the experiment even in an unselected mode. The known invalid reordered
PTS/DTS rejection remains a separate measured capability result. Exit zero
requires the selected path and its references to pass the scoped matrix.

`--sanitizers` instruments this C probe, not FFmpeg or Apple frameworks. Fresh
decoder GOP independence, a second native decoder stack, complete boundary
content/quality acceptance, the renderer's encoder transform and product export
integration remain separate requirements. No passing fixture is a complete
export qualification.

## Independent AVFoundation audio observation

`qualify_native_audio.py` reads the retained 120-frame, 60 fps default/disabled
edit-list pair from an encoder report. It independently checks the MP4 hashes
and compiles a small Objective-C reader against Apple frameworks. It does not
encode new media, link FFmpeg, open a window or play through an audio device.

```sh
python3 tools/media-qualification/compatible/qualify_native_audio.py \
  --encoder-report /tmp/deadpan-encoder-NEW-report.json \
  --work /tmp/deadpan-native-audio-NEW \
  --output /tmp/deadpan-native-audio-NEW-report.json
```

Each file gets two fresh readers, first stored AAC samples and then 48 kHz
interleaved stereo float32 PCM. Both drain to `AVAssetReaderStatusCompleted`.
Reports retain raw and output CMTime values, source/PCM formats, track segments,
trim and presentation attachments, complete untouched PCM, commands and hashes.
The default reader range is intersected with the asset duration; completion
does not prove access to coded padding outside that range.

`native_audio_oracle.py` admits exact, unrounded sample coordinates before
using the shared absolute event oracle. It does not apply attachment trims,
segment offsets or detected-event alignment. Ambiguous native timing is
explicitly unqualified. Keep `outcome`, `passed`, `event_timing_qualified` and
`unqualified` together: passing applicable checks alone is not timing admission.
The 799-sample tolerance stays strictly below the 800-sample video frame.
`--sanitizers` instruments the observer, not the Apple decoder/frameworks.
This is a second audio reader comparison, not acoustic playback, native video
qualification or approval of a nonconforming export path.

## Shared renderer encoder planes

`qualify_picture_export.py` consumes the successful real Metal report from
`crates/deadpan-render/examples/qualify_sdr_export.rs`. Build that example with
the normal locked workspace graph, then run its recorded Cargo artifact with
a new report path and a new fixture directory. It writes separate actual and
independent-reference tight I420 frames at 320×180 and 318×180, with their
SHA-256 identities. The [pixel contract](../../../docs/SDR_ENCODER_PIXELS.md)
defines the color transform, left-sited filter, bounds and readback lifetime.

```sh
python3 tools/media-qualification/compatible/qualify_picture_export.py \
  --build-report /tmp/deadpan-media-compatible-build.json \
  --renderer-report /tmp/deadpan-renderer-NEW.json \
  --work /tmp/deadpan-picture-export-NEW \
  --output /tmp/deadpan-picture-export-NEW-report.json
```

The runner rechecks both input hashes, compares every renderer code with the
independent reference, and passes only the actual renderer planes to hardware
H.264. The C probe hashes the bytes it owns before encoding and records the
consumed identity. Fresh full-frame decoding must retain the geometry, explicit
Rec.709 limited-range/left-chroma metadata, square pixels and exact one-frame
30 fps interval. Every decoded plane code is compared with the actual input;
maximum error 12 codes and mean error 2 codes per plane are fixed before
measurement. Spatial failures and per-plane statistics remain in the report.

These fixtures are video-only and disable edit lists. They neither resolve nor
relax the AAC timing constraint. The experiment does not qualify multi-frame
closed GOPs, real footage, full resolution, HDR, physical viewing or product
export. `--sanitizers` instruments the C probe only. Preserve failed observations
and process faults; correct metadata alone does not establish correct pixels.
