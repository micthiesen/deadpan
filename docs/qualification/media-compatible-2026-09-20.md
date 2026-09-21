# Compatible media boundary, 2026-09-20

The pinned rsmpeg revision works with the isolated FFmpeg 8.0.3 build for the
measured decode, seek, and retained-frame operations. This resolves the earlier
FFmpeg 9 binding mismatch for this bounded combination. Gate A remains open:
this is a developer qualification harness, not an integrated app adapter or a
qualified export configuration.

## Exact inputs and environment

- Host: Apple M5 Max, 128 GiB memory, arm64 macOS 26.5.2 build 25F84.
- Apple clang 21.0.0 (`clang-2100.1.1.101`), SDK 26.5; Rust 1.97.1
  (`8bab26f4f68e0e26f0bb7960be334d5b520ea452`, LLVM 22.1.6).
- FFmpeg release **8.0.3**, published 2026-06-18, with libavcodec 62.11.103,
  libavformat 62.3.103, libavutil 60.8.103. It was selected to match the
  binding's FFmpeg 8.0 API, not because it is the newest FFmpeg release.
  [Official release inventory](https://ffmpeg.org/download.html).
- Release archive SHA-256:
  `6136812ea6d4e68bdba27e33c2a94382711cdf4f8602ffef056ff792bd6f9818`.
  The downloaded archive, detached signature, and key bytes match
  [pins.json](../../tools/media-qualification/compatible/pins.json).
  GPG verified signer `FCF986EA15E6E293A5644F10B4322F04D67658D8` using an
  isolated scratch keyring. The key fingerprint is published with FFmpeg's
  [release verification instructions](https://ffmpeg.org/download.html#releases).
  Git reference: tag `n8.0.3`, commit
  [`8ae0b34901ba60a802f183ee75a250a9fc3e09a5`](https://github.com/FFmpeg/FFmpeg/tree/8ae0b34901ba60a802f183ee75a250a9fc3e09a5).
  The signed release archive is the actual build input; a tarball/Git-tree
  comparison was not performed.
- rsmpeg **0.18.0+ffmpeg.8.0**, pinned to
  [`b21fcfde8bb1ffdc179504e370e330385baa9819`](https://github.com/larksuite/rsmpeg/tree/b21fcfde8bb1ffdc179504e370e330385baa9819),
  under MIT. The resolved binding is `rusty_ffmpeg 0.16.7+ffmpeg.8`, MIT.
  The [upstream manifest](https://github.com/larksuite/rsmpeg/blob/b21fcfde8bb1ffdc179504e370e330385baa9819/Cargo.toml)
  declares the FFmpeg 8 default feature. The exact dependency lock is a
  checked-in input; all builds and Clippy checks use `--locked`.

The [build report](../../tools/media-qualification/compatible/results/build-2026-09-20.json)
contains signature output, full configure arguments/configuration, versions,
source/license hashes, dylib paths and hashes, linkage, and load commands.
The [normal qualification report](../../tools/media-qualification/compatible/results/qualification-2026-09-20.json)
and [sanitizer report](../../tools/media-qualification/compatible/results/sanitizers-2026-09-20.json)
contain every command, fixture hash, frame record, seek result, AAC event,
positive assertion, and explicit negative capability. Their source hashes match
the checked-in harness, manifest, and lock.

## Isolated build and license configuration

The build installs only into
`/tmp/deadpan-media-compatible-xyhilms4/prefix`. It uses shared libraries,
`--disable-gpl --disable-nonfree --disable-version3 --disable-autodetect`,
explicit VideoToolbox/AudioToolbox enablement, and no external x264/x265 encoder.
Network support, ffmpeg/ffplay programs, documentation, and debug build output
are disabled. The developer `ffprobe` program is built from the same source.
No host package was installed or replaced, and no application dependency changed.

All seven installed dylibs and both native/Rust probes have `LC_BUILD_VERSION`
`minos 15.0`, SDK 26.5. All relevant native and Rust linkage points to the
isolated libraries and Apple system libraries/frameworks, with no Homebrew
dylib dependency. This verifies deployment build metadata, **not execution on
macOS 15**. Only macOS 26.5.2 on this M5 Max was exercised.

The running library reports **LGPL version 2.1 or later**. Full FFmpeg license
text and Cargo dependency license declarations are recorded. Dynamic linking
and component selection follow the configuration considerations in the
[FFmpeg license guidance](https://ffmpeg.org/legal.html). This is not a completed
distribution audit: source delivery, notices, framework usage, install-name
relocation, app bundling, signing, notarization, and clean-machine behavior
remain to be qualified.

Reproduction is documented in the
[harness README](../../tools/media-qualification/compatible/README.md).
The rsmpeg source is fetched by exact commit into fresh scratch Git storage and
extracted with `git archive`; no supplied working tree, ignored build output,
or untracked source enters the downstream build. The committed-source archive
SHA-256 is `0dbb228348d9762fe08c18563e13480381a6d23eb5b813ef15b9780d5b1d6800`.
The result is reproducible source/configuration/behavior evidence; byte-identical
binaries across fresh absolute prefixes and different SDKs are not promised.

## Actual media results

Every fixture has 120 original 320×180 numbered pictures, a moving target, a
sharp cut, known BT.709 limited-range patches, and stereo AAC-LC at 48 kHz with
three authored impulses per channel. All media is generated locally by the
repository's native C fixture code. No personal source footage or downloaded
media was used. CFR is exactly `30000/1001`; VFR repeats 1001, 2002, and 3003
ticks at `1/30000`. The offset source begins at `60060/30000` seconds.

| Configuration | Result | Actual B-run / maximum GOP | Encode ms | Rust linear decode ms | Rust seek median / maximum ms |
|---|---|---|---:|---:|---:|
| OS software, CFR, B requested | Scoped assertions pass | 1 / 15 | 161.644 | 55.605 | 1.394 / 2.667 |
| OS software, VFR, B disabled | PTS/seek/audio pass; terminal duration fails | 0 / 15 | 154.210 | 53.532 | 1.204 / 2.650 |
| OS software, offset, B requested | Scoped assertions pass | 1 / 15 | 162.556 | 52.962 | 1.336 / 2.796 |
| Hardware, CFR, B disabled | Scoped assertions pass | 0 / 16 | 62.331 | 52.695 | 1.081 / 1.988 |
| OS software, CFR, B disabled | Scoped assertions pass | 0 / 15 | 153.794 | 50.231 | 1.197 / 2.191 |
| Hardware, CFR, B requested | Mux fails | Not qualified | n/a | n/a | n/a |
| OS software, VFR, B requested | Mux fails | Not qualified | n/a | n/a | n/a |

These are single observations on tiny warm-cache fixtures while other work and
the sanitizer run were active. They do not establish full-size throughput,
preview latency, quality, or stable encoder performance. The Rust probe is a
development build. Hardware is required with `allow_sw=0`; OS software uses
`require_sw=1`. No implicit GPL fallback exists in this configuration.

Both normal and sanitizer runs pass **252 orchestration assertions** each,
with the explicitly expected negative results kept separately. For each run:

- Native C and actual rsmpeg each verify **600 authored frame identities**,
  exact rational PTS, and **660 seeks** covering every frame in reverse order
  plus repeated nonmonotonic requests. The Rust probe also builds a frame and
  keyframe index. Its frame hashes, timestamps, raw durations, identities, and
  key flags match the C path for every frame.
- Each of five Rust cases retains an AVFrame clone through decoder reuse and
  seek flushes, destroys the decoder and demuxer, then reads the retained
  pixels again and verifies the original hash and authored identity.
- All **30 channel/impulse events** peak at the exact authored sample, with
  zero offset. The offset source's events are absolute samples 96196, 144096,
  and 288088. Audio stream ends match the intended sample boundary under the
  explicit movie-timescale setting below.
- A swapped-picture control retains original timestamps but swaps picture
  contents 10/11. Rust exits 1 with
  `authored identity mismatch: expected=10 actual=11`. This protects against
  merely comparing a wrong seek result with an equally wrong linear index.
- The ASan/UBSan C harness run has no sanitizer diagnostics. FFmpeg, Rust,
  and Apple frameworks were not rebuilt with sanitizer instrumentation.

The Rust executable performs real `AVFormatContextInput::open` stream probing,
packet reads, `AVCodecContext` decode/drain, `av_seek_frame` through rsmpeg,
decoder flushing, and owned `AVFrame` operations. It is not a mock or a
`cargo check` result. Its short in-memory index and CPU-plane buffers are still
fixture machinery, not the application's durable index or GPU frame provider.

## Failures and the working configuration

**VideoToolbox B-frame muxing remains conditional.** Hardware CFR and OS
software VFR both produce `pts (1001) < dts (2002) in stream 0`; muxing exits 1
with `Invalid argument (-22)`. OS software CFR and offset inputs work with a
maximum observed B-run of one despite requesting two. Hardware with B disabled
works, with a maximum 16-frame GOP despite requesting 15. Keep these results
specific to the tested FFmpeg/OS/hardware and fixture cadence.

**VFR terminal duration is lost.** With software VFR and B disabled, all 120
PTS and pictures are correct, but frame 119's duration is 1001 ticks instead
of its authored 3003. Video stream duration is short by 2002 ticks. The Rust
probe emits its completed decode/seek evidence and then exits 1 with
`authored indexed duration mismatch`; this case is explicitly marked as failed
duration qualification. Computing preceding intervals from adjacent PTS does
not recover the final authored duration. Do not guess it or report this as full
VFR qualification.

**The default MP4 movie timescale loses AAC offset precision.** A retained
negative control using FFmpeg 8's default 1000 Hz movie timescale starts decoded
audio at sample 95040 rather than 95072, which is 32 samples early after the
encoder's 1024-sample priming. Its stream end is 288256 rather than 288288.
The compatible C wrapper sets `movie_timescale=240000`, the least common
multiple of 30000 video ticks and 48000 audio samples. The offset case then
preserves exact impulse positions and stream end, exposes exactly 1024 leading
priming samples, and has 320 terminal decoded padding samples. Zero-start CFR
has no exposed leading priming and 320 decoded padding samples; VFR has 640.

For the next narrow adapter work, the measured starting configuration is the
isolated FFmpeg 8.0.3/rsmpeg pin, persistent FFmpeg contexts, exact PTS indexing,
hardware H.264 with B disabled, and explicitly selected OS software fallback.
The fixture mux uses `+faststart` and `movie_timescale=240000`. These files still
use FFmpeg's default **edit lists**, so they do **not** meet Section 22's
no-edit-list export rule. That rule needs its own measured priming/start/sync
solution before export acceptance. No product deviation is approved here.

## Remaining boundary

The prior [Cutlass comparison](media-2026-09-20.md) and offset-timestamp rejection
remain unchanged. The measured compatible rsmpeg boundary supports continuing
the spec's direct adapter approach without a binding fork; no Cutlass code was
extracted, and no upstream library was added to the application in this slice.

Still open: application provider/error/cancellation integration; durable source
and keyframe indexes; negative/missing PTS and duration policies; full input
format coverage (including HEVC, VP9, AV1, ProRes, MP3, Opus, images, unusual
audio layouts/rates, rotation, anamorphic/interlaced sources, HDR/color);
hardware decode and GPU texture lifetime; long GOP, corruption, seek churn and
memory-pressure behavior; macOS 15 and the rest of the supported OS/hardware
matrix; full-size timing/quality; actual audio output/DSP; preview/export
equivalence; no-edit-list exports; model/runtime qualification; and distributable
app packaging. Merely compiling a decoder does not qualify its format.

No live GUI work was needed for these library behaviors. GUI aesthetics,
natural keyboard navigation, accessibility, and native focus/IME remain part
of the application's separate interface qualification as its editor develops.
