# Native SDR encoder and offline PCM qualification

This qualifies the [native encoder boundary](../NATIVE_ENCODING.md), with
synthetic I420/PCM inputs and real emitted MP4 files. It also exercises the
revision-bound offline PCM reader. It does not qualify product Render,
full mastering, HDR, destination publication or release acceptance.

## Measured native result

Host: Apple M5 Max, 128 GiB, macOS 26.5.2 (25F84), Rust 1.97.1 and the pinned
LGPL-only offline FFmpeg 8.0.3 build. The retained reports include executable,
library/header, compiler/SDK, source and build identities.

Both normal and ASan/UBSan matrices contain:

- Ten completed 320x180 encoding cases, 872 frames total. Hardware without
  B-frames and explicit OS software with/without requested B-frames cover
  30000/1001 and 60 fps. Edge fixtures and one-frame 60 fps/nonzero NTSC ranges
  also complete. The latter inputs contain exactly 800 and 1601 samples.
- Forty actual GOP boundaries, each opened in a fresh decoder before submitting
  any packet. All 2432 resulting suffix frames match the linear pass's pixel
  hash, PTS and duration. GOP intervals are at most one frame above the requested
  half-second target. Software requested for two B-frames emits one actual B-frame.
- FFmpeg ordinary/manual and AVFoundation PCM with all 180 measured channel/event
  coordinates exact. No PCM alignment, event-based crop or packet dropping is
  used. The admitted encoded tolerance remains strictly below one picture frame;
  the observed error is zero samples. Short fixtures use nonoverlapping 64-sample
  search radii fixed around the independently authored markers. Negative tests
  reject missing and 1024-sample-shifted markers.
- Exact CFR frame count, visible identity, durations and terminal boundary;
  progressive square pixels, High profile, limited Rec.709 and left chroma;
  decoded color/neutral patches within four codes of the independent reference.
- Fast-start ordering, exact movie/media clocks and zero presented stream starts.
  Each track has one normal-rate edit. AAC offsets match retained 1024-sample
  priming and manual decode coordinates; video offsets match initial reordering.
  Audio's nonzero-origin endpoint stays 8005 movie ticks while video uses 8008.
- Six deliberate failures: byte exhaustion, cancellation, wrong picture ordinal,
  an audio hole, nonfinite PCM and incomplete finish. None returns a completed
  output report. Partial files remain retained for inspection.

Hardware B-frame encoding fails in three attempts per matrix. The muxer reports
`pts (1001) < dts (2002)` at NTSC and `pts (1) < dts (2)` at 60 fps. These are
rejected capabilities, not successful encoding cases. Edit lists do not repair
the invalid hardware packet order. No timestamp rewrite or implicit fallback
was added. The normal and sanitizer runs report no process/sanitizer faults.

ASan/UBSan instruments the C encoder and reader adapters plus target C
dependencies. Rust and the separately built FFmpeg libraries are not instrumented;
leak detection is disabled. This does not establish driver allocation or
full-resolution memory/performance bounds.

## Review and regression checks

Independent reviews covered the C/header, Rust ABI/ownership, offline source
preparation and qualification predicates. Fixes include coherent explicit High
profile requests, descriptor test permissions, deadline checks inside complete
index/hash loops, qualified native/short-event admission, actual GOP policy,
stream starts and parsed edit-list semantics. A second review checked the
deadline fix. All 2,159 locked workspace tests pass, with zero failures or ignored
tests. Strict workspace/all-target Clippy and formatting pass. The bounded Python
suite passes 114 tests, including negative acceptance controls. Command journals
and source inventories bind each check to its tested implementation.

Initial failures remain recorded: two Rust generic inference errors in tests,
one missing `Linked { bookmark: None }` test field, three example Clippy findings,
and the initial native matrix before acceptance rules were strengthened. They
were fixed before final admission. The hardware B-frame failure remains open.

## Evidence and remaining work

[Retained evidence](../../tools/media-qualification/evidence/2026-09-29-native-encoding/)
contains the matrices, command/build journals, source inventories, review notes,
actual MP4/PCM files and reader observations. Its summary and archive audit bind
every retained file to its bytes. The native archive contains 836 files totaling
97,928,126 uncompressed bytes; every archived size and SHA-256 was checked.

Remaining work includes real project pictures plus canonical audio inside the
supervised child, an isolated production file verifier, durable jobs/recovery,
atomic publication, native Render and full mastering/HDR. Full-plane lossy
project comparisons, opening/terminal edge-content fidelity, physical listening,
mid-native drain/fast-start cancellation, disk-full/short-write fault injection,
4K/8K performance and the release Apple Silicon/OS matrix remain unqualified.
No GUI changes were made, so no live GUI replay or screenshots were needed.
