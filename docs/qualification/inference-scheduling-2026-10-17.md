# Cooperative inference scheduling, 2026-10-17

This advances DP-18; the full requirement remains partial. Base revision:
`4d20ad6c`. Environment: Mac17,7, 128 GiB RAM, macOS 26.5.2,
Rust 1.97.1 and the qualified FFmpeg 8.0.3 prefix.

## Behavior

Playback and rendering request a cooperative hold from app-owned AI picture
generation. Jobs distinguishes a pending request from a confirmed pause.
The worker reaches a stage, denoising-step or decoded-chunk boundary and
synchronizes dispatched MLX work before acknowledging the hold. Loading and
GPU kernels are not arbitrarily preempted. Playback does not wait for the
acknowledgment.

Installed MLX 0.32.2 documents `synchronize()` as waiting for the default stream
of the default device. The pinned adapter and LTX inference pipeline use that
GPU stream; the sole explicit pipeline stream context is CPU RoPE calculation.
This is not a device-wide guarantee for arbitrary future stream usage.

Identity/token-bound controls use increasing sequence numbers. The host retains
one in-flight scheduling request and one replaceable desired state, reserving
capacity for cancellation. Every newly validated acknowledgment reports physical
state, even if a newer request superseded it. Duplicate acknowledgments cannot
reset the clock. The paused worker retains its loaded weights and inference
slot. Cancellation wakes the hold and retains normal supervised teardown.

Only acknowledged paused time is excluded from the 30-minute computation
deadline. Cancellation and terminal-exit grace periods use wall time; cumulative
acknowledged holds are capped at two hours. The store rejects scheduling records:
a hold is not a restart checkpoint. Sleep still interrupts generation and
requires explicit retry. Acceptance and authored state are unchanged.

Framed progress now counts synchronized prior denoising steps. The pinned sampler
calls the hook before its Euler update, so reporting `index + 1` would be false.
Bridge combines its 20-step and 3-step passes into one monotonic 23-step counter;
Extension uses 30 steps. Synchronization adds a dispatch barrier per callback.

## Review and regression verification

Independent review caught a rapid-toggle race: a superseded resume acknowledgment
was suppressed, allowing Jobs to show Paused while the worker ran. All validated
physical state changes now reach the app. Pure-state, subprocess and app tests
cover pause, resume in flight, and another pause.

`cargo xtask check` passed normal and UI-harness strict Clippy and all 259 selected
tests in 13.869 seconds. Coverage includes bounded controls, wrong or unsolicited
acknowledgments, paused computation deadlines, cancellation while paused, slot
retention, writer responsiveness, durable-state rejection, and existing hostile
worker/cleanup/publication behavior. One unrelated render test had a nextest
output-handle leak warning; its isolated rerun passed in 0.077 seconds without
that warning. All 134 Python contract tests pass.

Initial checks found an omitted FFmpeg environment setting, formatting of an
in-progress test file, two test fixture API/hash errors and a channel type
inference error. An old app test expected running inference never to yield;
it now checks that a request alone cannot establish a pause. No production
failure was hidden by weakening an assertion or increasing a timeout.

## Real MLX pause, resume and cancellation

The explicitly opted-in Rust integration test passed in 30.312 seconds using
the installed `ltx-2.3-q4-extension/1` pack and pinned MLX runtime. The retained
owned 24 fps source fixture was reconstructed with a 12-frame trailing Hold.
The test copies that package and uses normal context capture, allocation,
worker supervision and durable finalization.

The worker acknowledged its first hold and emitted no inference progress during
a two-second quiet interval. After resume, completed-step progress advanced from
0 to 1. It acknowledged a second pause and cancellation completed in 157 ms.
The result and stored state were Cancelled, with no candidate receipt, unchanged
candidate selection and unchanged authored revision. This is actual GPU inference,
not the scripted app worker. An earlier setup attempt found that the old `/tmp`
fixture had expired; no inference ran until the fixture was reconstructed.

[Retained evidence](../../tools/model-qualification/evidence/2026-10-17-inference-scheduling/)
contains the fixture recipe, event sequence, worker log, model identity, executed
test-binary hash, complete adapter source inventory, and test logs. The routine
test remains opt-in because it requires installed weights and occupies the GPU;
the successful execution above is the evidence.

A separate complete run through the release CLI reached Ready in 152.630 seconds:
150.995 seconds in the worker, 1.184 seconds in host qualification and 0.193 seconds
in publication. The 768×320 FromLeft plan retained nine context frames, generated
eight native frames and produced exactly 12 project frames at 24 fps. The complete
authored document matched the pre-generation dump. CLI SHA-256:
`8690b3444a53cdb01b3ab16742e60a9e06ba9f4e434a6e38310d67b12e9d03dd`.
The execution record, Ready report, after-dump, actual native/sampled footage
and provenance are retained with the evidence. The conditioned entry join has
the existing "noticeable" advisory (mean absolute RGB difference 7.227); Ready
is engineering validation, not acceptance or an aesthetic approval.
This qualifies completion with the new dispatch barriers; it is not a controlled
performance comparison with the prior adapter.

## Final release UI and native checks

The final app SHA-256 is
`9fd083e7fc4bde79fb1f03d937ca7b2da8d4cf90d374eed592654de877b2777c`.
Visual and isolated performance Jobs replays each pass 38 checks plus the
production key-routing audit. The final pause-state capture was inspected:
the explanation wraps, the paused state is distinct and each row's Cancel
action remains visible. Visual review corrected the panel's outdated yield
policy text. The final binary also passes native Apple M5 Max Metal startup,
window creation and shutdown. Normal and UI-harness strict Clippy pass after
the final edits.

The scripted Jobs performance replay records 2,278 frames with UI CPU p95
0.502 ms, 98 inputs with state latency p95 1.366 ms, and seven commits with
p95/max 82.597 ms. These are not inference/playback interference measurements.
Full reports and their existing layout warnings remain in the evidence; no
layout assertion failed. The selected screenshot is `paused-jobs.png` there.

## Limits

This does not qualify playback interference, worst-case pause latency, byte
budgets, simultaneous app instances or other model workers. Bridge scheduling
has protocol and callback coverage but no real Bridge model run in this slice;
its weights were not installed. No physical sleep or quit path was exercised.
