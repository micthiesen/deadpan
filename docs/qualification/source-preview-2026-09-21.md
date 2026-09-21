# Persistent source preview qualification, 2026-09-21

The native application now opens a local source, indexes its original presentation
timestamps and displays selected frames through the shared SDR GPU pipeline.
This is non-destructive source inspection. It does not import assets into a
project, edit a timeline, play audio, generate footage or export a movie.

The implementation starts from `86fa1e7502c8cd58019e16b62d8da0735adf9347`.
[Recorded evidence](../../tools/media-qualification/evidence/2026-09-21-source-preview/)
contains source manifests, source-index reports, GPU comparisons, sanitizer and
repository-gate logs, and the native interaction record. Hardware is Apple M5 Max,
128 GiB, macOS 26.5.2, Rust 1.97.1, wgpu 30.0.1 and the previously qualified LGPL
FFmpeg 8.0.3 developer prefix. These observations do not establish performance
budgets or clean-machine distribution.

## Source boundary

[`deadpan-source`](../../native/deadpan-source/) owns a persistent descriptor-only
software decoder. Metadata scanning avoids RGB conversion; exact seek preroll
converts only the target frame. Returned RGBA bytes remain owned after further
seeks and decoder destruction. Original PTS/DTS, positive observed duration,
source aspect, right-angle rotation and explicit color interpretation survive
the boundary. No secondary input opens or network protocols are admitted.

[`SourceSession`](../../crates/deadpan-media/src/source_session.rs) copies and
verifies the entire input into a private anonymous file, builds a bounded core
presentation index and recovers a failed decoder from that same snapshot.
The index envelope validates schema, content identity and core timing invariants
on JSON readback. It is derived metadata; project cache persistence is still open.
Native deadlines and cancellation are cooperative. This is not a preemptive
process memory/CPU guarantee.

Ten native integration tests cover 396 reverse/random seeks against independent
linear-decode hashes and authored frame numbers, exact RGB pixels, BT.709 YUV
range conversion, chroma siting, retained frame ownership, SAR/rotation and
unsupported/malformed input. Five host integration tests cover immutable
snapshots, exact frame/PTS selection, cancellation recovery, seek/index budgets
and corrupt source rejection. The source index has checked JSON roundtrip and
invalid-state tests.

The developer `inspect_source` probe exercised six real encoded sources. Each
index roundtripped and seven nonmonotonic seeks preserved exact indexed PTS and
retained pixel hashes:

| Source | Frames | Original interval in ticks | Clock | Open/index seconds |
| --- | ---: | --- | --- | ---: |
| H.264 CFR with B frames | 120 | `[0, 120120)` | 1/30000 s | 0.022 |
| H.264 CFR without B frames | 120 | `[0, 120120)` | 1/30000 s | 0.017 |
| H.264 VFR without B frames | 120 | `[0, 238238)` | 1/30000 s | 0.018 |
| H.264 with nonzero origin | 120 | `[60060, 180180)` | 1/30000 s | 0.020 |
| Hardware-encoded H.264, software decode | 120 | `[0, 120120)` | 1/30000 s | 0.016 |
| Retained generated FFV1 master | 25 | `[0, 1041)` | 1/1000 s | 0.717 |

The VFR source still has the previously observed final duration of 1001 ticks,
despite the fixture's authored 3003 ticks. The adapter records the actual file;
it does not claim to recover those missing 2002 ticks. Missing final duration
fails instead of inventing a nominal-rate endpoint. The generated master is the
actual output retained by [durable acceptance qualification](acceptance-2026-09-21.md);
no inference was repeated. Audio-bearing MP4 probes log expected AAC-not-on-
whitelist warnings because this boundary intentionally does not decode audio.

## Shared GPU baseline

[`deadpan-render`](../../crates/deadpan-render/) uses one host-owned wgpu device
and queue, owned RGBA8 input and one in-flight picture submission. It decodes
the explicit source transfer before interpolation, converts into linear Rec.2020
`Rgba16Float`, then applies the same explicit SDR display transform for preview
and offscreen targets. SAR, rotation, fit/fill and alpha compositing are shared.
The encoded `Rgba8Unorm` output matches egui's gamma-texture contract.

Nine headless tests cover color anchors, geometry, admission limits and WGSL
validation. Actual Metal qualification passed all 76 synthetic cases, including
all supported color/rotation/fit combinations, row padding, odd dimensions,
non-square samples, transparent input and black bars. Every display pixel was
compared with an independent f64 CPU reference. Maximum difference was 2/255,
the declared tolerance. A Display P3 red working sample retained a negative blue
coordinate of `-0.0012102127` before the display transform.

## Native appearance and keyboard review

The native smoke test initialized Metal and completed its shutdown callback.
Computer-use inspection used a temporary development `.app` wrapper around the
same debug executable because the automation inventory could not select Cargo's
unbundled executable. This wrapper is not a signed or portable distribution.

The screen review used numbered/color fixtures and the actual retained FFV1
master. The picture dominates the window, maintains its aspect ratio during
resizing and remains usable at a roughly 670-by-452-pixel observed window size.
Review found dim secondary labels and missing arrow glyphs in shortcut help;
the final UI increases contrast and spells out Left/Right. Controls and source
state remain readable in the small window, including failure text.

Observed keyboard behavior:

- Left/Right, Home/End and focused buttons selected the expected source frames
  and exact PTS. Rapid Home/End/Right settled on the latest requested frame.
- `⌘O`, Select All, paste and Enter opened CFR, offset, VFR and FFV1 sources.
  A fast input sequence initially appended paths because focus was requested
  after constructing the text widget. Focus now precedes text processing;
  repeated fast source replacement passed.
- Text-field arrows did not step video. Escape returned to frame navigation.
  Tab traversed path, Open, picture and frame controls; Space activated First.
- Native Option-E followed by E produced `é` without changing the selected
  video frame. Escape and Right then resumed frame navigation.
- A missing file cleared the previous picture and showed a specific error.
  Opening a valid source recovered normally. Quit during visible loading of an
  8 GiB sparse test file returned exit 0 and left no preview process.

The accessibility tree exposes a labeled path field, Open/frame buttons, picture,
frame count, PTS, loading and error text. This is structural AX inspection, not
VoiceOver or full accessibility acceptance. CJK IME composition, non-US physical
layouts, light appearance and the complete editorial workflow remain untested.

## Verification and remaining work

The repository gate passed formatting, strict workspace Clippy, 374 Rust tests
with no failures or ignored tests, the locked build and CLI diagnostics. Audio,
model, FFV1-report and native-harness Python suites passed 20, 54, 5 and 4 tests.
ASan/UBSan passed 32 source/media tests in 69.765 seconds. Instrumentation covers
the C adapter and target C dependencies; Rust and separately built FFmpeg
libraries are not instrumented. Initial development checks caught a missing
prefix, obsolete egui panel name, a Clippy arithmetic style error and the SHA-2
digest formatting API; final checks include their corrections.

Independent review prompted an explicit decoder pixel budget in addition to the
dimension cap. The 16,777,216-pixel default limits owned RGBA to 64 MiB and applies
before probing, to native decoder allocation and before Rust frame allocation.
The new regression covers invalid/small limits and H.264 coded-padding admission;
cancellation also precedes RGBA allocation. Source-only sanitizer reports now
omit unrelated worker artifact fields. The final gate and sanitizer run include
these corrections.

The native boundary review also found that invalid native hard limits were
checked after snapshot copying, and that global HDR metadata was not rejected
at stream admission. Limits are now validated before any input read. A regression
uses a reader that panics if touched, covering excessive input/pixel/packet limits
and invalid dimensions/frame budgets.

Stream, packet and frame HDR/ICC/ambient metadata now fail admission even when
transfer tags claim SDR. A reproducible 618-byte FFV1 fixture retains BT.709 tags
but carries stream content-light metadata. The regression failed against the
previous decoder and passes with the fix. Its encoded bytes, producer, pinned
ffprobe output and before/after results are retained. Additional side-data enum
branches are checked explicitly but do not each have a separately encoded fixture.
Three independent reviews covered general integration, native decode/ownership,
and GPU/UI lifetimes; all confirmed the production corrections. A proposed
fixture-generator return-value change was dismissed against the pinned API,
which returns a pointer on success and NULL on failure.

This foundation still needs the full source format/profile/color matrix, HDR,
interlace/deinterlacing, hardware decode, durable media ownership/cache/relinking,
plan-driven playback, audio scheduling, effects, shared export/encoded-file
verification and physical display color management. The actual GPU comparisons
do not qualify a calibrated display or realtime throughput. All requirements
and delivery gates remain open or partial.
