# One-play explicit mute intent, 2026-10-21

This increment admits the existing `SetAudio::Mute` Boolean with explicit This
play at every enclosing Repeat and no Visual selection. It changes authoring,
recording and dot routing through the existing scoped `SetAudioTreatments`
transaction. No schema, DSP, timing, model or encoder changes are involved.

The native mute key and `:gain-mute` capture the inverse of the entry owner's
mute once. Saved calls and dot apply that explicit on/off state to their current
target without toggling. The planner preserves gain, envelopes, mute ranges,
saturation and authored stage order. Exact isolation pools, mapped owners and
concrete presentation carry forward through subsequent instructions and calls.
Picture timing, the absolute Edit cursor and ordinary enclosing selection stay
unchanged. Successful runs commit and undo once. An already chosen state refuses
before another allocation; native preflight adds no history or instruction.
Stale authority and late failures leave the whole run unchanged.

## Failing baseline

Before production changes, the nested 3×2 Repeat witness proved valid authority
and exactly ten fresh nodes and two marks for the public scoped setter. The old
semantic planner then refused the same mute instruction at its allowlist. The
command exited 100 with one failure and 550 skipped. Source hashes and executed
binary identity are retained in `old-identities.json`. The copied old test
binary is
`7d216bf7d0248f693bbe876a32c3a34ec72ae8ca52655b00bbfb698b656c0254`.

## Review and logic checks

A fresh general reviewer and scope reviewer found no issues. The verification
reviewer found that Mute's Help/palette context omitted Repeat contents. The
registry now includes that context and explains key-range behavior, whole-owner
`:gain-mute`, explicit play authority and Boolean replay. Independent recheck
confirmed the fix. The generated command reference was refreshed and passes its
currentness check.

Six new core tests cover first/last and already owned branches, true and false,
exact pools, inverse/JSON, counted nested calls with staged gain changes,
mute/gain/pause/Zoom continuation, composite owners, no-op refusal, malformed
authority and atomic late failure. Two native service tests use the qualified
Original in a real managed package and a nested 720-frame edit. They check all
720 picture/mute positions, other treatments, separate peers, receipts, Undo,
Redo, reopen, unchanged history/registers on refusals and saved-state replay.

Formatting initially caught module order. UI lint then caught Rust array-repeat
syntax inside two JSON assertions; explicit arrays fixed compilation without
changing their conditions. Final formatting and strict normal/UI Clippy pass.
The normal selection passed 490 of 491 cases; the only failure was the stale
generated reference. Its refreshed targeted check passes. Two passing tests
reported output-handle leak diagnostics in the parallel run. Both pass cleanly
in a serial rerun with unchanged code and timeouts. No retained process or
descriptor identifies their owner; the cause remains unproven. Initial failures
and diagnostics are retained.

The UI-feature selection passes all 202 tests in 15.165 seconds, with no
output-handle diagnostics. The normal reference and two diagnostic controls pass
serially in 0.320 seconds. Source and final normal/UI test-binary identities are
retained beside the logs.

## Final release replay

The locked release build passes. The final executed private app has SHA-256
`55f8a2fb92cf43b456783efc0fb430832be12fbd5a082470ce133cdd39ca1205`;
helper identities are retained in `binaries.json`. The qualified
`cfr-bframes.mp4` Original has SHA-256
`5a820a79bf550d484d8ecb37ffddd048d0636794ebce5db83e4f5ce5f5e64918`.
Both final visual and quiet performance replays pass all 735 scoped checks plus
the 21,885,504-case production Kestrel audit. No ignored layout retry or same-cause
retry repeating for three frames was accepted.

The first release visual run failed at check 167: the long macro record hint
pruned the execute hint at 960×640. Root inspected its failure capture. Concise
record/run hints now keep both actions visible; their painted-text assertions
remain required. Independent source recheck found no issue. Final strict
normal/UI lint passes again after this UI change.

Three final 960×640 captures were inspected. Gain recording and mute recording
show the selected play, separate clocks, save/cancel and configured gain/mute
keys legibly. The mute-recording capture retains the preceding deliberate
mismatched-capture refusal. The final mute-state capture shows the expected
same-state no-edit message, with both compact macro hints visible. Existing
inspector-scroll and bounded layout-retry warnings are retained; no assertion,
warning or timing gate was weakened.

The quiet performance run measures 988 completed input-to-state samples:
p50 0.477 ms, p95 0.972 ms, p99 1.379 ms and maximum 1.723 ms.
All 293 picture completions finish without failure or timeout: p50 2.052 ms,
p95 14.838 ms, p99 21.476 ms and maximum 92.858 ms. These are offscreen native
measurements on this Mac, not physical-display or audio-device latency.

Evidence is in
[tools/media-qualification/evidence/2026-10-21-scoped-mute-macros](../../tools/media-qualification/evidence/2026-10-21-scoped-mute-macros).
It retains exact commands, source/binary hashes, original failures and
diagnostics, passing checks, three review lenses with follow-ups, both visual
generations and the quiet performance report.

## Scope limits

This is an authoring increment. Visual ranges, mixed/multiple-play macro
authority and other scoped audio-change families remain open. Existing gain
PCM/export evidence is not new emitted-file qualification for this increment.
Physical keyboard delivery, VoiceOver speech and listening remain on the owner
list under specification §29.1. DP-06, DP-09 and DP-24 remain partial.
