# Persistent stretched audio, 2026-10-11

Canonical Preserve and integer-pitch stage output now survives application
restart in a bounded derived cache. Native playback, idle preparation and the
headless audio session use the same implementation. Ordinary and projected
continuous stages share it. [Saved audio](../SAVED_AUDIO.md) specifies identity,
admission, storage and cleanup behavior.

## Correctness and lifecycle

The key combines fresh canonical input samples and exact plan mappings with
complete source fingerprints, the DSP recipe, actual output suppression and a
compiled engine source pin. A hit still scans and admits its complete input.
It then verifies the entire output checksum while copying into private PCM.
No partial or damaged copy is published. Cold misses pay for an extra input
scan; a hit avoids the continuous stretch processor, not source qualification.

The shared cache reserves staging output within 4 GiB and 4,096 entries.
Publication is atomic. Readers and writers hold file locks; cleanup observes
those locks and the unused grace period. Explicit cleanup cannot remove original
or accepted media. Full, busy, damaged and unavailable caches retain the canonical
preparation path. Cancellation, source rejection and deadlines remain errors.

All 516 audio tests and 88 playback tests pass, including 13 focused cache
tests. They cover exact uncached/cold/restored PCM,
nested stages, changed pitch keys, source re-admission, missing sources,
cancellation, complete checksum/header/extent rejection, regeneration reasons,
cleanup during active reads/writes, temporary contention, concurrent budget
reservation, last-use updates, abandoned staging, symlinks, hardlinks and a
replaced cache root. Three CLI storage tests, one native diagnostics test and
24 transport tests pass. Strict all-target workspace Clippy passes.

Four offscreen Metal replays pass 125 checks across Original playback, Sound
playback, Diagnostics and Storage. Follow-up Diagnostics/Storage replays pass
45 checks, adding full paint-clip assertions after scrolling to the new status
and complete cache key. Inspected captures show wrapped keys, truthful unchecked
payload status and readable controls. The debug linker reports its existing
large unwind-table warning; replay reports retain input-driven layout retries.
These are offscreen renders, not live-window or physical display observations.

The primary agent reviewed the complete change without subagents, as requested.
Review corrected permanent disabling after temporary lock contention, missing
idle-cache status, long key labels in Storage and ambiguous corruption-test byte
mutation. The initial native test commands selected the empty library target;
the corrected executable-target runs above execute the actual tests.

The first four-minute fixture runs exposed a real regression: the optional
input scan spent the canonical render's plan-work allowance, causing
`frozen projection work exhausted` before audio began. Caching disabled
successfully prepared the same project. The failed reports are retained.
Persistent-key scans now share a separate bounded allowance across nested
stages; its exhaustion skips lookup, while source and preparation budgets and
the original deadline remain shared. A regression test exhausts that optional
allowance and still compares the complete canonical PCM exactly.

## Fresh-process device checks

Apple M5 Max, 18 physical cores, 128 GiB; macOS 26.5.2 (25F84), Rust 1.97.1,
AC power and the qualified FFmpeg 8.0.3/Opus 1.6.1/dav1d 1.5.4 prefix. Each
run starts a fresh release process. No compiler or replay ran concurrently;
the OS file cache was not purged. The harness prepares the stopped picture
before Play and uses low monitor gain.

The short synthetic 320×180 fixture plays its complete 8.008-second Preserve
section. The existing [four-minute fixture](disk-pcm-2026-10-11.md) starts at
frame 3,600 and auditions about three seconds after preparing the complete
240.24-second stage. Each fixture gets an empty private cache for its first run,
the same directory after restart, and a separate cache-disabled comparison.

| Fixture | Empty cache: first heard report | Restart: first heard report | Cache disabled: first heard report | Saved payload |
| --- | ---: | ---: | ---: | ---: |
| 8.008-second stage | 216.310 ms | 154.430 ms | 158.803 ms | 3,075,072 bytes |
| 240.24-second stage | 4,784.691 ms | 2,399.313 ms | 2,995.108 ms | 92,252,160 bytes |

Both restarted processes report complete verification and restoration of the
same key their first process saved. All six final reports have no failure,
underrun or device fault. The short runs each present all 240 pictures; the
long auditions each present 90, with no leading, internal or trailing misses.
Each short run records 1,664 native silence-padding frames; the long runs
record zero. Peak process footprints range from 366,297,856 to 385,205,016 bytes,
including decoding, GPU, DSP and queue storage.
The long heard intervals are 2.998–3.003 seconds. These observations establish
reuse and expose its cold cost; they are not latency distributions or evidence
that small stages benefit consistently. The complete PCM comparisons are in
the automated tests, not inferred from device timing or listening.

## Limits

This is cross-process reuse of stretched output, not a new decoder cache or a
replacement for source admission. Hold reverse/room-tone blocks remain private.
Nested revision-bearing descriptors and engine source changes can conservatively
invalidate saved work. Cache and filesystem deadlines are cooperative. Physical
listening, other hardware tiers and the remaining voice/group/mastering corpus
are not established by these checks. DP-09 and DP-16 remain partial.

Evidence and reproducible commands are retained in
[`2026-10-11-persistent-audio`](../../tools/audio-qualification/evidence/2026-10-11-persistent-audio/).
