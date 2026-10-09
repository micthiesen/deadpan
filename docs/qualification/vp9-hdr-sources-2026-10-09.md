# VP9 HDR source qualification, 2026-10-09

Ten-bit profile-2 limited-range BT.2020 NCL PQ/HLG admission is implemented for
MP4 and finite WebM/Matroska. Native, release and packaged checks pass, with the
gate completed in parts and initial failures retained below. DP-02 and DP-16
remain partial; this is not complete source-format or release qualification.

Host: Apple M5 Max, 128 GiB, macOS 26.5.2 (25F84), Rust 1.97.1. The unchanged
qualified runtime is FFmpeg 8.0.3 with static Opus 1.6.1 and dav1d 1.5.4, at
`~/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix`.

## Interpretation and metadata

The existing bounded VP9 frame/superframe guard and software decoder retain
ten-bit planes and the shared RGBA64 HDR path. Both the native decoder and stored
receipt validator require ten-bit 4:2:0, limited range, BT.2020 primaries and
non-constant matrix; VP9 receipts remain progressive. Unsupported profiles,
twelve-bit/full-range HDR and changing interpretations still fail explicitly.

The [VP9 MP4 binding](https://www.webmproject.org/vp9/mp4/) uses version-zero
`SmDm` and `CoLL` FullBoxes. Admission checks exact lengths and flags, rejects
duplicates across these and `mdcv`/`clli`, and handles their different rational
units. Only exact conversion enters the shared ST 2086 contract; other values
are recorded as ignored. Unsigned luminance beyond FFmpeg's signed rational
numerator is refused before demuxing.

[WebM/Matroska mastering floats and content-light integers](https://www.matroska.org/technical/elements.html#Colour) are retained before
demuxing. Pinned FFmpeg otherwise drops zero/partial declarations and narrows
64-bit light values to 32 bits. VP9 has no in-band static metadata; the retained
container declaration supplies valid values or explicit ignored flags. Zero
light levels retain their unknown meaning. A missing member or value that
cannot be represented by the shared contract is recorded as ignored. SDR with
static HDR metadata is refused, including declarations FFmpeg would omit.

## Fixtures and current evidence

`generate_vp9_hdr_fixtures.py` produces thirteen retained synthetic files using
development-only FFmpeg 9.0.1/libvpx 1.17.0, never linked into Deadpan. Lossless
96x64 picture patches and moving ten-bit low codes are independently authored.
The inputs cover PQ/HLG, left/top-left chroma, 3:2 SAR, omitted VFR picture
positions, both MP4 metadata forms and WebM static metadata. MP4 retains pinned
mono AAC through sample 19219; WebM retains the identical video packets. All
thirteen regenerate byte-for-byte; hashes are in the fixture manifest.

Every decoded plane matches the authored values. Independent BT.2020 scalar
conversion matches the colored patch within one RGBA64 code. One-, eight- and
sixteen-thread reads and reverse seeks preserve pixels, PTS, duration and key
identity. Tests check exact versus ignored metadata, preserved zero light
values, malformed FullBoxes, duplicate metadata and signed-rational limits.
All 22 host receipt tests pass, including thirteen new HDR cases with exact
timing/color round-trips and forged depth/range/matrix/primaries/interlace refusal.

The previous release CLI at source `7270be5d`, binary SHA-256
`e4497b05013ba55f19fe035c091d563c55c58438d27143a8f11deb192fbb75b0`,
refuses PQ, HLG and static-metadata MP4 registration. Each before/after project
dump remains identical. Logs and source/binary evidence are retained in
`/tmp/deadpan-vp9-hdr-20261009`.

The first full native test run passed all cases except an existing WebM refusal
test's old error-message substring (`SDR transfer`). It was updated for the
expanded explicit color rule; the malformed input remains refused.

No live GUI inspection was performed.

The first gate passed both strict lint stages and ran 5,589 workspace tests:
5,587 passed, two failed and twelve were skipped. Both failures were existing
SDR proxy tests: the pause case refused a VideoToolbox packet with PTS 22528
instead of 22016; the stalled-range case observed four attempts instead of the
expected three. Each passed unchanged in a separate follow-up after the broad
run finished. This does not erase the original failures or establish that
VideoToolbox is reliable under concurrent test load. A passing
`source_registration::hold_audio::pauses_inserted_with_reversed_audio_need_admitted_source_samples`
also had nextest's delayed-stdio `LEAK` diagnostic. Logs retain the initial
native-test message mismatch and all gate results.

The separate UI run passes all 1,097 tests, with two skipped; its
`headless_errors_keep_the_same_structured_protocol` also reports delayed stdio.
Documentation tests pass. All 167 native source tests pass with ASan/UBSan and
none skipped. Instrumentation covers the native C adapter and target C
dependencies, not Rust, FFmpeg, Opus or dav1d. The sanitizer report retains exact
test-binary and runtime hashes in `/tmp/deadpan-vp9-hdr-source-asan-20261009`.
Both delayed-stdio tests pass separately under nextest without that diagnostic.

## Small HEVC output regression

The first release import succeeded, but its automatic HDR Render failed before
publication: the verification decoder's pixel budget was too small for the
encoder's coded raster. A retained regression encode confirms that VideoToolbox
uses a 160x64 HEVC SPS raster for a 96x64 visible picture on this host. The old
64x64-block calculation allowed only 128x64. The same shared budget serves the
encoder admission probe, candidate verification and public `verify-export`.

The budget now applies the measured 160x64 minimum before block rounding. Exact
sample-entry dimensions, conformance crop and every visible decoded picture
remain required. The new regression fails against the old budget and passes
eight real PQ/HLG files: 32x32, 96x64 and 144x64 use 160x64 coded planes; 64x96
uses 160x96. Their files, hashes and independently parsed HEVC headers are in
`small-hdr/` in the qualification directory. The initial failed release binary
is retained as `verified-release-cli-before-padding-fix`; its failed job reports
confirmed cleanup and no published movie. Release qualification uses the rebuilt
binary in the fresh `release-fixed/` run.

After the fix, strict lint passes for the affected CLI, media and source crates.
All 32 verification tests pass (one measurement ignored), and all twenty encoder
admission tests pass (three opt-in qualifications ignored). Native source code
and fixtures remain unchanged from the full gate and sanitizer run. These
targeted checks cover the new shared budget without repeating unrelated tests.

## Release public Render

The rebuilt release CLI creates thirteen Ready one-Original projects and renders
all thirteen. Independent emitted-file verification passes 156 pictures, seven
signal-bearing windows at zero offset and six silent windows. Every project
keeps the 30000/1001 basis, exact retained source PTS and ten-bit HDR policy; VFR
keeps ten source pictures over twelve output frames. Anamorphic output is 144x64.
AAC ends at sample 19219, and video-only WebM exports have the corresponding
silent interval. The successful reports and complete commands are retained in
`release-fixed/`.

Two additional WebM inputs combine the static-PQ picture stream with retained
`opus-mono-silk.webm`, and HLG with `opus-stereo-20.webm`. Stream-copy assembly
preserves every video/audio packet, PTS, duration and trim declaration; hashes
and commands are in `combined/fixtures.json`. Both retain exactly 8197 Original
audio samples. Their release renders add 24 verified pictures and two signal
windows at zero offset, for fifteen imports, 180 pictures, nine signal windows
and six silent windows in total.

Release CLI SHA-256:
`ac60a0a696f4d5f6e09e9aa585d7a377e854b4870b4d4b9ee71e7d6bb6e5863d`.

## Packaged runtime and final review

`/tmp/deadpan-vp9-hdr-bundle-20261009/Deadpan.app` passes the normal bundle audit,
relocated positive checks and all tamper/refusal checks. The ad-hoc signed bundle
contains the same pinned FFmpeg dependency. A fresh home directory, unrelated
working directory and `/usr/bin:/bin` PATH import all thirteen retained inputs
and both combined Opus inputs. Seven representative renders verify 84 pictures,
four signal windows at zero offset and three silent windows, including static
metadata, both transfers, chroma siting, VFR and Opus. Packaged CLI SHA-256:
`352cde316d9b0bfe42896656b9153da65e9268ee6b43ca53539ad49f58dd275f`.

Work and review were performed by the sole agent, as requested. The review
manifests bind the checked sources and fixture hashes; final checks confirm no
source change after the affected tests, release runs or package build. Logs,
binary hashes, commands and retained failures remain in the qualification
directory. Full-range/twelve-bit HDR, high-resolution performance, other
container grammars and the broader source/operation matrix remain open.
