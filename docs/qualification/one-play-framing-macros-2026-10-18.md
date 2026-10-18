# One-play framing macros, 2026-10-18

Zoom and Creep record intent and apply it to the explicitly chosen Repeat play
at invocation. A direct eligible framing Apply also installs that intent for
dot. This advances DP-06 and DP-21; both remain partial.

Base revision `51bf003d22d0092bee597607807e2af28672dfa2`, Apple M5 Max,
macOS 26.5.2 (25F84), Rust 1.97.1 and the qualified FFmpeg 8.0.3 prefix.

## Invocation and isolation

Saved programs retain scale, curve and Keep or OriginalCenter intent. They
retain no node, iteration, presentation or recording-time pose. Runtime
`SemanticScopedContext` carries a direct inspected Repeat/Retime root, exact
`ScopedNodeTarget` and separate concrete `InstancePath`. Every Repeat ancestor
must name one explicit stable Play. The ordinary Sequence parent, selected
child and Edit cursor remain independent.

The pure planner admits only Zoom and Call while scoped, including every
expanded nested instruction. It asks the host for the exact matching picture
layer and intrinsic owner duration, then uses the existing shared builder and
`EditScoped` isolation path. No-op checks precede identity allocation. Exact
fresh node, mark and revision pools remain checked. Each changed leaf maps both
addresses for subsequent instructions; the complete run has one Compound and
one Undo. Existing owned branches need no further isolation.

The host resolves qualified metadata and render-plan geometry without source
decoding or proxy access. A retained Follow keeps its target, clock and fallback
center while the live target center supplies the current pose. Ancestor Retime
projection does not replace intrinsic owner duration.

Native capture retains the full scoped Target alongside ordinary context.
The service validates the reconstructed native authority and returns one
original-to-final scoped receipt. A delayed successful edit remains durable,
but scope navigation permanently revokes its inspector continuation, even if
the user returns to the same address in one event batch. Scope changes cancel
recording; navigation is never saved as hidden occurrence authority.

## Tests and review

Core tests exercise real nested 3-by-2 Repeats, repeated staged Zooms and named
calls, exact isolation and mark pools, mapped target and presentation,
unchanged other plays, complete inverse, existing overrides, no-op allocation,
strict wire forms and atomic late refusal.

Qualified host tests distinguish two concrete plays sharing a node, reject a
wrong occurrence, retain a 120-frame owner under a 48-frame projected play,
and preserve a live Follow through isolation and Retime with linked media
unavailable. Public CLI tests compare dry-run/commit semantic traces after
normalizing only fresh identities, replay on another play, preserve all
unselected picture semantics, and verify history/reopen and strict refusals.
Native service tests cover durable mapped receipts, one Undo/Redo, no-ops, dot
intent, atomic failure, refresh failure and idempotent retry.

Independent review identified a real request-envelope limit bug: legal
maximum-depth play authority exceeded the old program-plus-4-KiB bound. The
new 170,707-byte authority allowance derives from shared depth/identity limits;
the complete request remains bounded to 305,875 bytes. Parser regressions
exercise legal maximum depth, widest actual Apply/Run shapes, exact raw byte
boundary, oversized raw input and the unchanged independent 128 KiB program
limit. The native host transport's separate bound is larger.

Review also caught the same-batch navigation ownership bug described above and
an incorrect replay oracle that expected a curve endpoint at the last
half-open picture sample. The oracle now distinguishes that picture from the
exact authored endpoint. Follow-up read-only review confirms the fixes and
reports no further supported findings.

Two initial CLI failures were comparison-helper errors: presentation Repeat
steps use `node`, and a later resolved selection can name a freshly isolated
node. The helper now normalizes those fresh IDs consistently, retaining all
identity relationships, stable play IDs, poses and clocks. Failed logs remain.

## Repository checks

The scoped `cargo xtask check` passes 448 selected tests, including real-media
and emitted-file checks, plus strict normal and UI Clippy. One existing
counted-copy test reports an output-handle leak in the broad normal run; its
isolated normal and UI reruns pass cleanly. The UI runtime selection covers
474 tests: 473 pass in the main run and command-reference regeneration
intentionally fails after writing the generated file. The reviewed generated
reference passes its rerun, as does the counted-copy test. A final scoped gate
passes strict normal/UI lint and both targeted reruns. These overlapping
selections are not a full workspace gate.

Debug app linking retains the existing `__eh_frame section too large (max
16MB)` compact-unwind warning. No warning is suppressed.

## Native replay and responsiveness

The first release visual replay passes 149 scoped-play checks plus the
Kestrel audit. Inspection of its eight 960×640 captures found a misleading
"ranged zoom" refusal in a whole-play context and a Marks hint that the scoped
recording gate did not admit. The message now says "framing macro", and that
hint is hidden while recording. The inspector's Camera button now enters the
same recording gate as its key, refusing the unsupported draft before opening
it. A replay check clicks that real button and verifies no camera, pending
entry, instruction or authored edit. Follow-up review finds no regression from
the shared gate. Strict normal/UI lint and both focused checks pass again.

The final optimized visual and separate performance replays each pass 150
scoped-play checks plus the live Kestrel audit: 62 reserved bindings,
21,885,504 routing cases, no conflict or source drift. The replay records
Zoom/Creep in play 3, runs the saved intent on play 1, repeats a direct Zoom
with dot on another play, restores complete documents through Undo, refuses
sampled targets and All plays, and exercises both withheld-receipt navigation
cases. The existing scoped editing and pause cases still pass.

All eight final captures were inspected at 960×640. Play badges, independent
Original/Edit clocks, recording instruction counts, valid keys, refusal text
and delayed-result messages are visible. The keyboard and pointer paths use
production widgets, the project service, SQLite and offscreen Metal. The replay
opens no native window and does not establish physical keyboard, IME, display
or acoustic behavior.

The quiet performance run has no concurrent Cargo or agent work and performs
GPU submissions without PNG readback. Power is AC with battery at 80%; caches,
thermal state and unrelated system background activity are uncontrolled.

| Measurement | Completed | Median | p95 | Maximum | Failed / timed out |
| --- | ---: | ---: | ---: | ---: | ---: |
| Input to state | 153 | 0.514 ms | 1.086 ms | 1.699 ms | 0 / 0 |
| Input to completed picture | 50 | 2.127 ms | 9.663 ms | 87.968 ms | 0 / 0 |

These are short-fixture offscreen measurements, not full-resolution or physical
display latency. The picture tail remains recorded rather than hidden by the
median. The final app and CLI build without the debug compact-unwind warning.

## Retained evidence

The [evidence directory](../../tools/media-qualification/evidence/2026-10-18-one-play-framing-macros)
retains failed and passing logs, both visual reports, the final performance
report, final and pre-fix captures, source diffs including new files, source and
binary identities, fixture identity, independent review, collection scripts
and the checked SHA-256 inventory. The final app is
`51f554a1d49546813a2db62e795125cea8fe9f2f4f9543032f3d4102f10918c4`;
the CLI is `131b29e169b603a583c7ef09c590bce9d6bc79b2eb03d21708690bcdc1ed038a`.
Unchanged private media, tracking and transcription helpers retain the hashes
recorded in `binaries.json`. The source manifest binds this checkout to the
base revision plus its retained diff; no archived-source executable is reused
as a current app.

## Remaining scope

One-play semantic authority currently admits whole-owner Zoom and Call with
Keep or OriginalCenter, no Visual selection, and explicit Play branches at
every level. All/default, mixed and multiple-play choices, mismatched or absent
presentations and unsupported expanded instructions refuse without writes.
Sampled/named/face-target recording refuses before analysis; ordinary direct
interactive framing remains available. Pure Retime inspectors without Repeat
ancestry retain their direct value-edit path. Scoped ranges, general occurrence
macro edits, broader Place slice intent, selectors and parser acceptance remain
open. Named macro Run keeps the existing rule that it does not install new dot
intent. Physical input, IME, display, speech and human visual judgement remain
in the owner's qualification scope.
