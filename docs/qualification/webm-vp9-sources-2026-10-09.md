# Video-only VP9 WebM and Matroska, 2026-10-09

DP-02 / DP-16, specification §§16.3 and 28. This adds finite, video-only SDR
VP9 WebM/Matroska sources. Eight/ten-bit 4:2:0, full/limited range, anamorphic
display, hidden references, existing-reference pictures and measured VFR
timestamps are implemented. It does not complete Opus, audio-bearing WebM,
unknown-length streaming containers, VP9 HDR, missing-color interpretation,
VP9 CodecPrivate extensions or the remaining source/operation matrix.
Implementation and review belong to the sole agent.

## Admission and timing

The existing bounded EBML grammar now recognizes WebM and `V_VP9`, retaining
the same finite sizes, metadata/packet limits, closed elements, and structural
SeekHead/Cue checks. Explicit color and chroma siting must agree with a bounded
first keyframe header. The existing native VP9 packet guard checks every later
picture and superframe before codec submission. No runtime dependency changes.
The [WebM container guidelines](https://www.webmproject.org/docs/container/)
describe the container fields; unsupported interpretations fail explicitly.

Matroska display dimensions provide a stream-level SAR. The native decoder
now receives that SAR, preserving the distinction between encoded pixel raster
and displayed aspect. It continues to reject differing VP9 `render_size`.

Raw millisecond PTS and durations stay unchanged. A retained nanosecond
`DefaultDuration` can select a standard fractional presentation cadence only
when its representation error is below one nanosecond and every measured PTS
and terminal endpoint is within one source tick of that grid. Contradictory
hints fall back to measured interval analysis. Repeated VFR intervals may use
their bounded exact common-divisor grid; the 60/40 ms fixture selects 50 fps.
Project-frame centers select the original picture intervals. Neither rule
snaps source PTS, changes source endpoints, or claims that VFR is CFR.

The native Original/relink pickers include `.webm`. The usual typed project
creation, immutable receipt, preview, playback and Render paths consume these
sources without a separate authored format or a converted Original.

## Fixtures and review

`generate_webm_fixtures.py --check` reproduces ten retained files using
development-only FFmpeg 9.0.1. It copies compressed pictures from hash-pinned
MP4 fixtures and removes AAC. Four twelve-picture clips exercise both bit
depths and ranges. Others cover a Matroska document, 4:3 SAR, sixty displayed
pictures with hidden altrefs, two existing-reference clips, and twelve VFR
pictures over exactly 600 ms. The manifest retains file hashes and provenance.

Native tests compare every decoded pixel with the source MP4, then check
sequential and backward-seek equivalence with 1/8/16 threads. They assert actual
millisecond PTS/durations and SAR. Negative cases cover missing/contradictory
color and phase, unsupported profile/depth, truncation, raster/display changes
and resource limits. WebM files also seed container and decoder mutations.
Receipt tests round-trip all ten sources and assert exact source endpoints.
Cadence regressions exercise long fractional-rate sequences, contradictory
hints, high-rate reduction and a VFR grid shorter than its modal interval.

Review caught the dropped Matroska SAR and rejected quantized cadence; both
are repaired and exercised through public Render. An initial VFR fixture used
the wrong bitstream-filter clock and was corrected before qualification.
A renamed error message also required keeping the existing refusal assertion
consistent; no acceptance check was removed.

## Verification and limits

Apple M5 Max, macOS 26.5.2 (25F84), Rust 1.97.1, pinned LGPL FFmpeg 8.0.3.
Commands, failures, reports and fixture reproduction are retained in
`/tmp/deadpan-webm-20261009`.

The regression suites pass 138 native source tests, 143 media tests (including
the receipt and cadence regressions), and 459 CLI unit tests. ASan/UBSan also
passes all 138 source tests. It instruments native C adapters and target C
dependencies; Rust and the separately built FFmpeg libraries are not
instrumented. The VFR proxy's twelve pictures preserve every PTS/duration;
its mean difference is 0.776 eight-bit levels, maximum block difference 6.667
and largest channel bias 0.408, within unchanged limits 3, 16 and 1.5.
Final ordinary source/media reruns include the late display-unit and coarse-clock
guards. Strict all-target workspace Clippy passes both with and without the UI
harness, and rustfmt passes. This milestone uses the targeted suites above;
the full workspace run belongs to the preceding VP9 MP4 qualification.

The prior release CLI from `f0ffda502b05d7937955b33c0ff7836c8e2eae85`, SHA-256
`17ecefe86a8195843a10dee628bdca557bf51f747ba132569647b03c2b5b603b`, refuses
all ten final files. Debug and release public creation/Render each pass all ten: 166 exported
pictures, including the VFR fixture's thirty project frames. Independent
fixture-derived provenance checks verify frame-center selection. These inputs
have no source audio; silent encoded output does not qualify Opus or audio sync.
The verified release CLI SHA-256 is
`a6daa6704e2c64ac092e4588a659826558bdb13be3ddc9d93da66f3755f6010b`;
its executed bytes are retained beside `release-cli.json` in the evidence.

The relocatable ad-hoc bundle builds and `bundle-verify` passes its native/AI
runtime smoke, import, relocation and negative integrity checks. A further
packaged CLI run uses a fresh home, unrelated working directory and
`PATH=/usr/bin:/bin`: ten-bit full-range, anamorphic and VFR inputs create,
render and verify 54 pictures. There are 66 observable neighboring-picture
comparisons and 36 unobservable comparisons from intentionally repeated VFR
pictures. The packaged CLI SHA-256 is
`de699cfb6fefe380dbd189ea273773b39655b1d865fc2aa677009f05a6b8f976`.
The initial harness incorrectly parsed `doctor`'s plain-text output as JSON;
that failed harness run is retained separately, with no product-code workaround.
The sanitizer harness likewise needed a fresh empty build directory.

Quantized CFR WebM remains ineligible for the existing strict MP4 proxy recipe:
reported durations of 33 ms differ from some adjacent intervals of 34 ms.
The application retains exact Original decoding. The first direct proxy test
bypassed eligibility and correctly failed correspondence; the qualification
keeps that failure and tests the eligibility refusal. The explicit 60/40 ms
VFR fixture is expressible and uses the unchanged exact proxy checks.

Opus remains implementation work. The retained real WebM probe reports packet
PTS -7, 14, 34 ms with 312 leading samples. The
[Matroska Opus mapping](https://www.matroska.org/technical/codec_specs.html#a_opus)
requires codec delay accounting. A generic contiguous sample-index rule cannot
silently reinterpret that coarse clock. No Opus audio has been admitted by this
change, and no audio timing requirement has been relaxed.
