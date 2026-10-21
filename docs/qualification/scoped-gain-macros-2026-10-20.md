# One-play relative gain intent, 2026-10-20

This increment admits existing `SetAudio::Step` intent with explicit This play
at every enclosing Repeat and no Visual range. It changes authoring and native
macro/dot routing, using the existing scoped `SetAudioTreatments` command.
No schema, gain processor, timing map or encoded-output path changes.

Each instruction reads the staged owner's current gain, preserves its other
treatments, and uses exact isolation node/mark pools. It carries the actual
mapped owner and concrete presentation into the next instruction or named call.
The absolute Edit cursor and ordinary enclosing selection remain unchanged.
Counted runs commit and undo once. Out-of-range gain is refused rather than
clamped. Zero steps allocate nothing. Unsupported authority or a later failure
leaves document, history and registers unchanged.

The native `+`, `-` and relative `:gain` routes record only intent. Dot and saved
macros resolve the newly selected play. All/mixed/several-play nonrecording gain
retains its direct path. Other scoped audio-change variants remain refused.

## Failing baseline

Before production changes, the new nested 3×2 Repeat test proved the captured
target valid and required exactly ten fresh nodes and two marks. The old planner
then refused at its single-play macro allowlist. The command exited 100 with
one failing test and 544 skipped. Source identities and the executed test binary
SHA-256 are retained in `old-identities.json`; its binary was copied before cache
maintenance. The old test binary is
`c47a3a87d582a5ba987ae27b13856ab41115a91708fea2b6f5152b8471223a25`.

## Review

A general reviewer and two focused reviewers initially returned no findings.
A separate read-only follow-up identified a source-level command-gate bypass:
several audio commands used `GainStep` as their representative recording action.
After admitting gain steps, `:gain-mute` could reach a direct unrecorded edit.
No runtime reproduction of that bypass is claimed. The final gate separates
relative steps and refuses the seven other audio command families during scoped
recording after its existing context/pending checks. Seven replay controls check
that document, revision, history, captured target, bank and instruction count
remain unchanged. An independent follow-up reviewed every representative
`GainStep` caller and found no further bypass.

## Logic and durable native checks

The focused core selection passes all 30 tests, including six new gain tests.
They cover nested first/last and already owned branches, exact pools and
inverse/JSON, counted nested calls, pause/gain/Zoom continuation, composite
owners, inclusive gain bounds and atomic late refusal. The pure gain tests
preserve authored mute and saturation choices while stepping the current trim.

The normal scoped/semantic/macro/gain/completion selection initially passed
466 of 467 tests. The new zero-Step test incorrectly used `send`, then the
receipt helper, both of which unwrap success even though zero steps deliberately
return a no-edit refusal. The final test uses `command`, asserts the refusal and
exact unchanged document, history rows and register bank. Its targeted rerun
passes in 0.234 seconds. Both failed logs are retained; production behavior and
assertion gates were not weakened. The other 466 passing results are preserved.

The final formatting and strict normal/UI Clippy checks pass. The UI-feature
selection passes all 186 tests in 12.008 seconds, with one output-handle
leak diagnostic from
`unobserved_transition_cannot_be_hidden_by_later_mark_or_cut_proof`.
That test only manipulates in-memory semantic State and launches no worker.
No retained process or descriptor identifies the output-handle owner; its cause
remains unproven. Its isolated rerun passes cleanly in 0.009 seconds without a source or timeout change.

The service-level tests import the qualified `cfr-bframes.mp4` Original into
a real managed package and wrap it into a nested 720-frame edit. They check all
720 picture/gain positions, separate peer treatment, mapped receipts, one Undo
per action/run, redo/reopen and document/bank/history atomicity on malformed,
stale and out-of-range requests.

## Final release replay

The locked release build passes. Its private app and helper copies have retained
SHA-256 identities; the final executed app is
`14185d83d00831d5bfd6a023613a2229a472be5b197beddfdf206169e3486ec5`.
The qualified Original is `cfr-bframes.mp4`, SHA-256
`5a820a79bf550d484d8ecb37ffddd048d0636794ebce5db83e4f5ce5f5e64918`.
Both final visual and separate quiet performance runs pass all 671 scoped checks
plus the production Kestrel routing audit. They cover actual `+`, relative
`:gain`, recording, saved calls, current-play dot, Undo/Redo, seven refused audio
commands, mismatched captures, zero steps and withheld durable replies. No
ignored layout retry or same-cause repeating retry was accepted.

The first release runs passed 666 checks, but inspection caught stale footer
hints advertising only pause/framing macros and no gain key during recording.
The final UI includes gain in both scoped macro hints and displays the configured
gain key pair while recording. The final replay adds painted-text and footer
checks at 960×640. Both recording and dot captures were inspected: the chosen
play, independent clocks, gain buttons, active recording/save/cancel status and
key hints are legible. Existing inspector-scroll and bounded layout-retry
warnings remain retained; no warning or timing gate was relaxed.

The performance run measures 940 completed input-to-state samples: p50 0.475 ms,
p95 0.984 ms, p99 1.319 ms and maximum 1.838 ms. All 275 input-to-picture
completions finish without failure or timeout: p50 2.032 ms, p95 12.458 ms,
p99 14.836 ms and maximum 93.465 ms. This is an offscreen native replay on this
Mac, not a physical-display or audio-device latency measurement. It does not
close DP-24.

Evidence is in
[`tools/media-qualification/evidence/2026-10-20-scoped-gain-macros`](../../tools/media-qualification/evidence/2026-10-20-scoped-gain-macros).
It retains source/binary hashes, exact commands, baseline and test-fixture
failures, passing results, reviews, both replay generations, the initially stale
footer capture and the two final compact captures. Cargo cache maintenance
reduced measured profile artifacts from 563.4 GiB to 79.5 GiB before the builds;
that maintenance is not product behavior or qualification evidence.

## Scope limits

This is a bounded authoring increment. Broader occurrence macros, Visual-range
gain under scoped macro authority, mixed/multiple-play macro authority and other
audio-change families remain open. Existing gain PCM/export qualifications are
not represented as new emitted-file evidence here. Physical keyboard delivery,
VoiceOver speech and listening remain on the owner list under specification
§29.1. DP-06, DP-09 and DP-24 remain partial.
