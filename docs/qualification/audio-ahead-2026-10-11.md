# Audio preparation during playback, 2026-10-11

The existing playback preparation worker now continues into a bounded FIFO
while the device queue is full. This gives future canonical audio work more
time to finish before it is heard. It uses the same immutable snapshot, source
admission, continuous DSP, limiter and monitor gain as ordinary playback.

## Bounds and lifecycle

Each engine retains at most 175 batches of at most 8192 stereo frames:
1,433,600 frames, 29.867 seconds and 10.9375 MiB of PCM. One producer batch,
one controller batch, the four-second native queue and canonical caches have
separate bounds. A full FIFO blocks the worker through the existing wakeable
condition variable. One reserved failure slot preserves the valid prefix's
order. No extra thread or cache owner is introduced.

Activation still waits only for the first 8192-frame batch. Device delivery
intervals remain the only heard clock. Stop and supersession cancel the worker
and clear the FIFO; automatic terminal teardown also clears it. An atomic
capability check on each control tick revokes an edited proposal after its store
closes, including when almost 30 seconds are already buffered. It performs no
filesystem or database work on the controller or callback.

The diagnostics panel and exported report expose queued stereo frames separately
from reply count and native packets. Values aggregate across engines. The
performance harness records bounded preparation milestones beside actual heard
samples, without treating producer progress as playback time.

## Verification

Apple M5 Max, 18 physical cores, 128 GiB; macOS 26.5.2 (25F84), Rust 1.97.1
and the qualified FFmpeg 8.0.3/Opus 1.6.1/dav1d 1.5.4 prefix.

All 88 playback tests, four diagnostics tests and three doctor tests pass.
The new cases verify FIFO ordering and allocation bounds, the terminal failure
slot, Stop with a full queue, a fresh generation after Stop and revocation of a
fully buffered edited proposal. A decoded-source Preserve stage after five
seconds of silence finishes while the simulated heard clock remains at zero;
complete delivered PCM matches the separately prepared canonical reference.

An isolated copy of the final source with FIFO capacity reduced to one fails
that future-stage assertion. It uses a separate Cargo target and a pinned test
binary. This is a controlled single-slot variant, not an exact historical build.

The release `perf playback` harness exercised the actual output device and
shared Metal picture path on a scratch project. Its generated 320×180 fixture
contains an 8.008-second silent Hold followed by 8.008 seconds of Preserve
output at 30000/1001 fps. Each run used a fresh process and PCM cache; the OS
file cache was not purged. No compiler or replay ran during either measurement.

| Run | Heard interval | First delivered-sample report | FIFO maximum | Underruns / faults |
| --- | --- | --- | --- | --- |
| Complete edit | 16.016 s | 100.219 ms | 539,392 frames | 0 / 0 |
| Selection loop | 35.004 s | 68.085 ms | 1,433,600 frames | 0 / 0 |

At 667.499 ms in the complete run, 507,904 frames (10.58 seconds) were queued
while the heard sample was 27,338 (0.570 seconds). The future Preserve was
therefore ready well before its 8.008-second entry. Initial prefill stayed at
8192 frames in both runs, and Stop left zero FIFO frames. Reported peak process
footprints were 392,348,464 and 413,057,840 bytes, including all decoding, GPU
and DSP allocations. These two observations do not establish a latency
distribution or an improvement over the previous implementation.

The complete edit presents all 480 pictures. The timed loop presents 1,049 with
zero internal skips; its reporting cutoff leaves one final heard-frame picture
unpresented. The complete run also records 1,280 device-silence padding frames;
the loop records zero. Neither reports starvation or a device fault. The
deterministic PCM comparison above covers sample identity; these device reports
do not establish physical listening or acoustic synchronization.

Strict all-target workspace Clippy and formatting pass. Offscreen Metal
replays pass 102 checks across diagnostics, Original playback and Sound
playback, including export of the new gauge, cancellation, focus and existing
keyboard routing. Inspected diagnostics captures retain visible, aligned rows
and report/Close controls. These are offscreen renders, not live-window captures.

## Retained failures and limits

Initial test compilation used an incorrect diagnostics field name. Two subsequent
test attempts advanced a simulated heard clock before the resumed controller
admitted its first covering delivery report. Waiting for that first Playing
update and retaining a midpoint inside each reported interval corrected the
test; production watchdogs and clock rules were unchanged. Strict Clippy also
identified a collapsible conditional, which was corrected.

The replay reports retain input-driven layout retries and their egui warning;
the later diagnostics capture is clear. The debug build retains its existing
large `__eh_frame` linker warning.

This horizon cannot guarantee that every expensive stage finishes before its
audible deadline. Persistent PCM reuse across restart, the remaining cache
lifecycle, voice/group processing and full mastering remain open. Physical
listening and acoustic synchronization remain in the owner verification scope.
DP-09 and DP-16 are still partial.

Commands, failures, source and executed-binary hashes, the controlled negative,
real-device reports and offscreen replay evidence are retained in
[`2026-10-11-audio-ahead`](../../tools/audio-qualification/evidence/2026-10-11-audio-ahead/).
