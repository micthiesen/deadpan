# Real inference and playback interference, 2026-10-17

The production app scheduler sustained the measured 1080p60 and 4K30 loops
while a real Extension attempt reached its cooperative pause boundary, then
resumed inference and cancelled cleanly. This advances DP-18 and DP-24;
it does not close either requirement or the broader performance gate.

Base revision `eb33f5ce`, Apple M5 Max / Mac17,7, 128 GiB RAM, macOS 26.5.2,
Rust 1.97.1. Executed release app SHA-256:
`5cdfea503252e02529380927650e72d3d51bff8e4bdb856620dac1c60e44ac76`.
The app and helpers ran from a private binary directory. Cargo work stopped
before both serial runs. Both began on AC power with normal memory pressure
and no recorded thermal warning. Other host activity and OS caches were not
controlled. Native helpers use the qualified FFmpeg 8.0.3 prefix; this is the
installed development MLX runtime, not a new packaged-runtime qualification.

## Workload and evidence boundary

The explicit `ai-playback-interference` performance replay opens a private
portable copy of a fresh full-source Original. Owned synthetic testsrc2/sine
fixtures supply 120 seconds of 1080p60 and 60 seconds of 4K30, with 48 kHz
audio. It disables proxies, uses 5% monitoring gain, and adds a trailing
three-second deterministic Hold through the production command path.

Each run first plays six five-second Edit loops without inference. It then
starts a real app-owned `:generate mode=extend-left`, waits until at least one
denoising step has completed, and repeats the same loops. Production power,
memory and foreground admission remain enabled. The approved Extension plan
uses 768×320, nine context pictures plus 72 generated pictures, 30 denoising
steps, and 180 project frames at 60 fps or 90 at 30 fps.

Full offscreen Metal UI composition runs throughout actual device playback.
Every displayed ticket is matched to the device lap and sample captured when
the production transport submitted that exact request. A delayed picture cannot
fill coverage in a later lap. Leading, interior and trailing pictures all count;
missing attribution, missing required picture coverage, reduced-tier pictures, device failures
or absent pause acknowledgement fail the replay. Both intervals are retained
even when one fails. Cancellation and bounded draining also run after errors.

## Results

| Measurement | 1080p60 baseline / inference | 4K30 baseline / inference |
| --- | --- | --- |
| Measured picture coverage | 1,800 / 1,800, all present | 900 / 900, all present |
| Missing ticket attributions | 0 / 0 | 0 / 0 |
| Skipped or reduced pictures | 0 / 0 | 0 / 0 |
| Device reports | 2,817 / 2,817 | 2,817 / 2,816 |
| Underruns, faults, padding frames | 0 / 0 for each counter | 0 / 0 for each counter |
| First observed Playing state | 292.164 / 124.450 ms | 261.372 / 124.901 ms |
| Callback cost high-water | 12.125 / 13.250 µs | 9.709 / 16.542 µs |

Callback maxima are cumulative engine high-water observations, not interval
deltas. Coverage excludes the next partial lap. Requested/exact decode counts
may include its first request; the retained report keeps those counts separately.

The observed Pausing-to-Paused intervals were 15.537 seconds at 1080p60 and
15.522 seconds at 4K30. Playback had already entered Playing and maintained
complete coverage and uninterrupted audio during that wait. These are sampled
UI-state intervals, not precise protocol acknowledgement or GPU preemption
timestamps. Model progress stayed at one completed step while paused; after
playback stopped, Running and then two completed steps were observed. Jobs X
cancelled the captured attempt; both private databases retain one durable
`cancelled` attempt, no selected Ready candidate and no accepted generated media.

Three stopped seeks, one split and one undo were exercised both before and
after inference started. Active-inference seek command-to-settled samples were
41.326–83.745 ms at 1080p and 105.356–144.268 ms at 4K; split/undo samples were
41.206/38.435 ms and 41.700/29.952 ms respectively. These include multi-key
navigation and are smoke samples, not the specified warm-seek or edit latency
percentiles. Proxies were deliberately off. Full-run input-to-state p95 was
2.691 ms over 118 inputs and 5.212 ms over 116 inputs; those aggregates mix
baseline, setup, inference and cleanup, so they do not isolate interference.

Each release run passed 22 scenario checks plus the production shortcut audit.
Existing second-pass footer-layout warnings remain in the reports; there were
no unsettled painted frames or uncaused retry oscillations. The initial launch
failed argument validation before opening audio because explicit project
retention was forbidden. The parser now permits retention for this private-copy
scenario, with a regression test. Review also corrected false success when an
interval failed and replaced inferred loop attribution with exact ticket-bound
request coordinates. All nine focused coverage and option tests and both strict Clippy
modes passed.

## Limits and retained material

[Evidence](../../tools/model-qualification/evidence/2026-10-17-inference-interference/)
contains reports, execution/fixture identities, source snapshots, durable
request/input bindings and cancellation records, plus the initial rejected
launch. Cancellation intentionally removed temporary conditioning PNGs, runtime
configuration and worker outputs; they are not reconstructed as executed
provenance. Installed-runtime observations collected afterward are labelled
separately. No candidate was accepted and no complete inference movie was
produced by these interference runs.

This measures two ordinary full-resolution source loops on this Mac, one model
and one active job. It does not establish physical display or acoustic timing,
all effects, render/inference interference, other models, hardware tiers,
long-running hostile stress, or the full latency sample requirements. Baseline
precedes inference in each process and warms caches. Broader workloads and
requirements remain open; no performance threshold was relaxed.
