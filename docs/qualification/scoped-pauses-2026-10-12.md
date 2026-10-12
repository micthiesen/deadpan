# Pauses in This play, 2026-10-12

The scoped Repeat inspector now inserts a freeze, black pause or AI fallback
through `,h`, `,a` and `:hold`. Every enclosing Repeat must name one concrete
play. The insertion keeps the root Edit cursor and selects its newly saved Hold
inside that play. One Undo restores the preceding structure and timing.

`InsertOccurrenceTime` accepts an optional `within: InstancePath`. The shared
`pause_insertion_within` query begins at that occurrence, so both its edges
belong to it. In particular, inserting at the start of play two cannot select
the end of play one. Outside-in isolation remaps the captured instance before
the final insertion query. Existing requests omit `within` and retain ordinary
timeline descent and its left bias at Repeat seams.

The native request captures session, revision, outer Sequence scope, inspected
root, concrete play, presentation and cursor. Both the UI and service validate
that capture. The provisional result must place the allocated Hold at the
captured cursor before the store commits. The exact receipt keeps the inspector
on the new Hold; unrelated history cannot retarget a command entered earlier.
AI insertion saves the deterministic fallback and durable preparation together.

Freeze capture still samples the preceding measured picture and retains only
composition below the actual insertion parent. That parent and its ancestors
stay live. If the preceding picture lacks the complete parent instance, the
operation refuses before reading media or writing. It does not substitute the
right-hand picture or flatten a different play's parent. Black pauses need no
picture capture.

## Verification

Base `d387788d`; Apple M5 Max, 18 cores, 128 GiB, macOS 26.5.2 (25F84),
Rust 1.97.1 and locked dependencies. The primary implemented and self-reviewed
alone. Tests use private packages, real media, and the offscreen production
UI/Metal harness. No native window, browser or audio device was used.

The core matrix checks 120 insertion boundaries across plain and nested Repeat
plays and selected children, including both edges, command/document JSON,
forward/inverse patches, isolation proofs, unchanged shared definitions and
exact root positions. Foreign boundaries, missing/stale instances and Retime
ancestors refuse. A real 44.1 kHz PCM matrix checks 21 start/interior/end cases
at 30000/1001 fps against independently rounded pre-edit allocations, including
room-tone gaps, later plays, the outer suffix, silence and chunk independence.
All 1,725 core/audio tests pass with zero failed or ignored.

The native service matrix performs 27 freeze/black/AI fallback insertions in
three plays at their start, interior and end, reopening SQLite and exercising
Undo/Redo for each. Broader, absent, stale and foreign captures leave history
unchanged. A real MP4 case verifies the preceding source PTS, captured lower
framing, live Repeat motion and unchanged surrounding pictures at play two's
start. It also covers the explicit cross-parent capture refusal. The final
13 scope/service tests pass.

The broad app run passed 1,124 tests and reported four failures and nine existing
ignored model, generated-media and stress tests. The failures were a stale
generated command reference, an obsolete assertion that Repeat pauses were
unavailable, and two linked-import timeouts. The reference was regenerated and
verified; the updated scope assertion passes. No reference or assertion was
changed to conceal an unsupported Retime operation.

Sampling a timed-out import located it in `CFBundleGetMainBundle`, scanning the
large Cargo output directory while creating a macOS bookmark. A direct
Foundation create/move/resolve check passed. The identical app test executable,
verified by SHA-256 and copied to a small temporary directory, passed both
bookmark tests in 0.36 seconds without code or timeout changes. The original
failures, native stack sample and copied-binary receipt are retained.

The final `scoped-plays` offscreen replay passes 78 checks, including real
keyboard routing, the new Hold's receipt and measured freeze, a compact
960×640 inspector, one Undo, broader-scope refusals and a delayed history
reply during command entry. The ordinary `ai-insertion` replay passes 56
checks covering counted insertion, durable fallback/intent, Retry/Discard,
history, dot and macros with scripted model unavailability. Both production
shortcut audits pass against the local Kestrel source. The primary inspected
the final compact rendered frame: all pause controls and key labels are visible,
the selected play remains clear and the footer stays anchored.

The first replay found the new pause actions below the compact inspector's
visible area. Edit controls now precede the detailed Hold settings. Its failing
frame and report are retained alongside the final replay. The first app build
also caught a return-type mismatch in UI submission; that was fixed before the
service tests. The command-reference generator intentionally exits with a
review reminder after rewriting; its subsequent verification passes. Debug
linking retains the existing `__eh_frame` size warning.

Strict locked workspace/all-target Clippy with `deadpan-app/ui-harness` passes
with `-D warnings`; final formatting and diff checks pass.
The [retained evidence](../../tools/media-qualification/evidence/2026-10-12-scoped-pauses/manifest.json)
includes every command receipt and log, source patch and hashes, executable
identities, complete replay checks and semantic snapshots, the inspected
before/after compact frames, the final private replay projects, bookmark
diagnostics and [self-review](../../tools/media-qualification/evidence/2026-10-12-scoped-pauses/self-review.md).
The [test counts](../../tools/media-qualification/evidence/2026-10-12-scoped-pauses/test-counts.json)
separate overlapping focused and broad runs rather than adding them as distinct
tests. Only the selected compact PNGs are copied from the full scratch replays.
Complete replay JSON is gzip-compressed; `replays.json` retains each original
uncompressed SHA-256.

## Remaining scope

All plays, multiple plays, authored Retime interiors, scoped dot/macros, other
temporal operations, unsupported cross-parent freeze boundaries and freezing
accepted footage remain required work. This increment does not rerun a real
AI model or establish physical keyboard, VoiceOver, acoustic or release
performance qualification. No DP requirement or delivery gate is newly complete.
