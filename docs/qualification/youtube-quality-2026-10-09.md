# YouTube AV1/VP9 and Opus acquisition

This extends the existing URL import through the real media worker and ordinary
single-Original qualification. It does not qualify every YouTube rendition or
complete the broader source-format and rendering requirements.

## Implementation

- Direct H.264 MP4 and qualified VP9/AV1 MP4/WebM video, plus AAC MP4 and
  mono/stereo 48 kHz Opus MP4/WebM audio. Selection prefers raster area, frame
  rate, retained HDR, codec efficiency, then bitrate. Original language and
  normal dynamics precede audio bitrate. The native confirmation names the
  selected codec and dynamic range.
- Remux protocol 2 copies encoded content through bounded, descriptor-only
  readers with one pending packet per input. Rust admits each WebM region in
  its own address space before FFmpeg sees it. No decoder or encoder is opened
  during assembly. Ordinary source qualification follows before publication.
- Exact packet rescaling and the checked common movie clock preserve video
  timing and Opus's physical 48 kHz clock, gain, pre-skip, terminal trim and
  positive offsets. Bounded edit-list rewriting restores offsets without
  exposing priming samples. Header durations upgrade to 64 bits when needed.
- Re-demuxing checks encoded payload digests, timestamps, picture intervals,
  flags, skip samples, codec configuration, SAR, color and static metadata.
  Only the empty initial AV1 temporal delimiter (`12 00`) is removed and
  counted. Other muxer filtering fails verification. This follows the
  [AV1 MP4 sample-format recommendation](https://aomediacodec.github.io/av1-isobmff/#av1-sample-format).
- WebM `DefaultDuration` is retained as a nominal hint in movie UUID
  `164d86bf-c898-481b-9920-91633daee697`, whose payload is one big-endian u64
  nanosecond duration. Every measured picture must still fit the proposed
  cadence; this metadata supplies no timestamps.

## Explicit chroma interpretation

The real YouTube AV1 and VP9 downloads omit chroma siting. Deadpan uses a
documented left interpretation for this specific absence: horizontal chroma
samples coincide with left luma samples, vertically halfway between luma rows.
This is Deadpan's rendering policy, not evidence of the encoder's intent.

AV1 retains `CSP_UNKNOWN` in its unchanged sequence/configuration. VP9 WebM
requires both siting coordinates absent or unspecified; partial, unsupported
or conflicting declarations fail. Its MP4 `vpcC` records the chosen left
position, while movie UUID `fab35b69-357d-478b-97e4-c7f96a655432` records the
original absence (big-endian u64 value 1). That UUID requires exactly one
left-sited VP9 picture track and survives another assembly. Both cases retain
`chroma_assumed_left` in the source qualification receipt. The app shows a
notice with the displayed picture. Other missing color interpretation still
follows the existing admission rules.

## Retained fixtures and real investigation

The assembly matrix covers 22 video files and 16 Opus files, including both
containers, eight/ten-bit SDR, PQ/HLG, VFR, SAR, AV1 grain/super-resolution,
static HDR metadata, SILK/hybrid/CELT, 2.5/20/60/120 ms packets, gain and long
pre-skip. Additional cases cover positive offsets, zero pre-skip, malformed
headers, bounded descriptor regions, cadence metadata, and missing chroma
through assembly and reassembly. AV1 missing-siting fixtures retain the exact
pixels of the independently checked left-sited references in both containers.

The authored `av1-temporal-delimiters.mp4` regression adds exactly two bytes
to each of twelve unchanged coded temporal units. Its generator and JSON
manifest retain the input/output hashes and operation. Replacing a delimiter
with padding fails the encoded-byte check.

Real source: Blender's *Caminandes 2: Gran Dillama*, YouTube ID `Z4C82eyhwgU`.
The earlier project qualification established the permitted animated source.
No cookies or account were used. Downloads retained in
`/tmp/deadpan-youtube-quality-20261009/downloads`:

| Format | Bytes | SHA-256 |
| --- | ---: | --- |
| 399, 1080p/24 AV1 MP4 | 26,193,661 | `269274bb8de027eab4db610156173d28a7d85d8f9cbec8d193f4b1412686d53c` |
| 248, 1080p/24 VP9 WebM | 23,665,363 | `74784039194bad7b6d3b82d645f861ebc9969d290f85dc1efe0fb20e7fea6199` |
| 251, stereo Opus WebM | 2,538,420 | `d00d2d1ecf0852a64efcc7ccab2e8bba575c065fa51d0350e7dc7e52c34db69c` |

The first real assembly failed because FFmpeg removes AV1 temporal delimiters;
the explicit normalization above fixes it without relaxing coded-byte equality.
The next admission failed on missing chroma siting, leading to the recorded
policy above. Both debug imports then produced Ready 1920×1080/24 projects:
3,504 source pictures and 3,506 edit frames, including the measured audio tail.
The saved qualification receipts retain the chroma interpretation after reopen.

## Verification

Runs on 2026-10-10 used this Mac's Apple M5 Max, 128 GiB memory, macOS
26.5.2 (25F84), Rust 1.97.1 and the qualified-3 FFmpeg 8.0.3 prefix with
static libopus 1.6.1 and dav1d 1.5.4. The source is based on
`c40998800dc17e4a79a6b470f7a55ba18b14502a`; changed-file hashes and the
separate sole-agent review record are retained in
`/tmp/deadpan-youtube-quality-20261009/source-identity.json`.

- `cargo xtask gate`: formatting and strict workspace/UI Clippy passed.
  Workspace nextest ran 5,605 tests: 5,602 passed, three failed and twelve
  were skipped. Two failures were stale expectations for the new SDR label
  and invalid zero admission timeout; both were corrected. The unchanged
  VideoToolbox paused-proxy test reported packet 68 PTS 35,328 rather than
  34,816. Its isolated rerun passed, as did all sixteen proxy tests under
  sanitizers. No timing guard was relaxed.
- Follow-up formatting, strict workspace/UI Clippy, all 44 selected affected
  tests, all 1,099 UI tests/replays (two skipped), and documentation tests
  passed. The retained fixture generator's `--check` passed. Logs and commands
  are in `gate.log`, `followups.json` and the adjacent named logs under the
  same scratch root. The gate completed in these parts, not as one green run.
- The env-enabled real app-job test imported the public URL through details,
  confirmation, AV1/Opus transfer, assembly and qualification in 13.8 seconds,
  then reopened and validated the Ready package (`app-real-final.log`). This
  runs the native app's job implementation without opening a GUI. A final
  strict worker Clippy pass after the fixture correction also passed.
- All eight assembly tests passed, including a 19,000-second Opus offset
  requiring 64-bit MP4 movie and track durations without rounding. Its first
  sanitizer run exposed an inconsistent authored fixture endpoint; the
  fixture was corrected before the full rerun.
- All 242 native worker/source tests passed with ASan/UBSan in
  `/tmp/deadpan-youtube-quality-asan-final-20261010/rerun-report.json` and
  `rerun.log`. Instrumentation covers the selected native C adapters and
  target C dependencies; Rust and FFmpeg libraries are not instrumented.
  The sanitizer harness now prepares the tracking worker required by the
  bundle test. Earlier setup and fixture failures remain in the retained logs.

The archived release executables in `release-verified/` have SHA-256:

| Executable | SHA-256 |
| --- | --- |
| `deadpan-cli` | `36f726f6aadbc37548be8a0aa4444a457e47132486213b04ae9ab1637dcf3eb9` |
| `deadpan-media-worker` | `5833c23c70a9b1948bb03ceb28167458d845f8e25b80af25463a992ed074ae33` |

That release worker independently assembled the retained real VP9/Opus
downloads. Its output SHA-256
`ca5c15f010d9de85615f206558bd2dd20df21bba5eaeccb3bbeb34ed90c8c2e9`
matches the imported VP9 candidate byte for byte (`release-final/vp9-remux.json`).
The independently downloaded release and packaged AV1 Originals also match
the earlier assembly byte for byte: 28,721,216 bytes, SHA-256
`f15ff79a5c5680dd63dbc4c7b57a7726394158903c0a4d288c2256bd2861f2b8`
(`av1-original-identity.json`).

The real AV1/Opus URL import completes in 10.1 seconds and the full 3,506-frame
Render publishes successfully. Independent `verify-export` checks all 3,506
frame timestamps, compares 585 pictures with the shared renderer and verifies
143 signal windows at zero measured audio offset. Minimum sampled luma/chroma
PSNR is 36.97/40.11 dB. The default verifier strides long movies to at most
600 picture comparisons; it does not compare every picture's pixels. The first
scratch assertion incorrectly expected 3,506 comparisons and was corrected
to assert the complete frame inventory and the documented 585 comparisons.
The successful verification report was reused without another export.

The 741.3 MiB ad hoc signed bundle in
`/tmp/deadpan-youtube-quality-bundle-20261010/Deadpan.app` passes its 75-file
Mach-O audit and `bundle-verify`: relocated scrubbed-home smoke/doctor/helper
checks, local import/Render, bundled AI runtime GPU smoke, and all helper,
AI-worker and notice tamper/deletion negatives. No model pack was imported in
this run. `bundle-final.log`, `bundle-verify-final.log` and their command reports
retain the results. Final `cargo fmt --all -- --check` passes after correcting
one test-only line wrap; runtime source is unchanged from the archived builds.

Signed bundle executable SHA-256 values:

| Executable | SHA-256 |
| --- | --- |
| `deadpan-app` | `a5ed900b07cbb9e1f94c008fd1b1847f4a2c45dc7a72e5a09f734104bb968f61` |
| `deadpan-cli` | `e75927c042ff7f10bc1f96fe47e225dffe4c106cb688fb973ead91e75daea67f` |
| `deadpan-media-worker` | `fcf184901dcdb38c83e7311ce0024ec41244d360c59460cfda087af1278652b9` |

The full-length VP9 release export also passes: all 3,506 frame timestamps,
585 sampled pictures and 143 zero-offset signal windows. Its minimum sampled
luma/chroma PSNR is 33.63/39.13 dB. Together the release runs verify 7,012 frame
timestamps, 1,170 picture comparisons and 286 signal windows; full reports and
movie hashes are in `release-final/report.json`.

The packaged URL import succeeds with an isolated home and
`PATH=/usr/bin:/bin`, and its full Render passes structural verification and
publishes. Independent content verification **fails** at frame 3,126, the
picture after a scene cut: luma PSNR 30.54 dB against the unchanged 32 dB gate.
All 3,506 timestamps and 143 audio signal windows pass. Running the release
verifier on that same movie reproduces the exact failure. Both encodes use
hardware H.264 without B frames; this packet is 36,212 bytes in the failing
movie versus 143,192 bytes and 37.91 dB in the passing release movie.
The failed movie/report remain in `packaged-final/work/youtube-av1-export`
and `packaged-final/youtube-av1-verify.stdout`; the cross-check is
`cross-release-frame3126.json`. No threshold was lowered. The cause and an
isolated-encode follow-up remain under investigation. This milestone proves
the packaged import, not complete export-quality qualification.

Live GUI inspection was not run. A user override for the assumed chroma position,
broader codec profiles, above-1080p performance and clean/second-Mac acceptance
remain outside this evidence.
