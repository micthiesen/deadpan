# Structural capture and cut register, 2026-10-01

This increment adds exact whole-child capture, cut-to-register and insertion of
empty structural groups at explicit Sequence slots. A cut captures privately,
saves one deletion, then publishes the historical copy to the session register.
Empty groups retain their structure and metadata without adding picture or audio
time. The [edited-slice contract](../EDITED_SLICES.md) defines the shared capture
and placement boundary.

The implementation base is `2ea675646abc24e8410616af74b4193ed4af51da`.
The workspace tests, focused rendered replays, native keyboard check, formatting,
Clippy, default-feature app tests and full release replay passed. Results below
belong to their recorded source manifests. The retained
[evidence package](../../tools/media-qualification/evidence/2026-10-01-structural-capture/README.md)
includes the corrected failures and exact source reconciliation.

## Exact structural capture

`SliceCaptureSelection` distinguishes a nonempty `Range { range }` from an exact
`Child { node }`. Child capture requires a direct child of the named ordinary
Sequence, with ordinary Sequence ancestry. It retains one complete subtree and
derives its historical range from that child. Equal-time empty siblings remain
distinct, and adjacent empty siblings do not become part of a positive child
capture. Recapture against the named immutable revision must reproduce the full
selector, bounds, structure, marks, assets and timing state.

A valid zero-duration child consists only of Sequences. Capture retains their
labels, framing, audio treatments, lineage and owned Bound/unresolved marks,
including an asset referenced by dormant Source intent. It proves that no
physical audio owner is present and performs no root-wide audio capture. Whole
positive composites retain the existing complete owner contexts and clocks.
Independent root sounds remain outside the copied contribution.

Zero-duration `SpliceSlice` inserts at an explicit pre-edit child index. Fresh
identities create the neutral wrapper and its copied subtree; no time is added.
The operation consumes no timing slot, including at ordinal `u32::MAX`, and
preserves all existing bindings, timing records, lineage, marks, sound recipes,
routes and allowances. It still creates one reversible authored transaction.
Copying the pasted empty wrapper works again. Aggregate identity, depth, mark,
asset and wire limits remain enforced.

Empty temporal Range capture remains invalid. Zero content rejects interior
insertion, replacement and range Move. The prior positive Range path, standalone
Split, Source placement and existing partial-composite admission remain intact.

## One saved cut and its register receipt

Native Visual `d` and captured `:delete` use a Range selector. Whole-beat `dd`
uses the exact Child selector, including a zero-duration group. The project
service validates the current session, project, revision and scope, then captures
the immutable contents without writing history. It commits one `DeleteRange` or
`DeleteRipple` request and publishes the copy only after that transaction succeeds.
There is no separate authored capture or intermediate delete-and-insert step.

The retained `CutReceipt` owns the same captured Arc, saved revision, cursor and
scope, plus any preview-refresh warning. Repeating the same successful request
returns its receipt without another edit; reuse of its identity for a different
selection rejects. Ordinary queries retain the saved cut independently of the
single pending reply. Undo uses a fresh revision and leaves the accepted historical
copy available within the session.

The register separates accepted content from pending Yank/Cut intent. A newer
Original yank, edited yank, cut or locally rejected copy/cut attempt supersedes
older pending intent. Failures preserve the accepted register. A late successful
cut cannot replace a newer copy, but its already committed deletion remains
authored. Session changes clear the register. This remains a session register;
no named or persistent clipboard is added.

### Saved-cut stale-workspace correction

Review found that a later yank could complete against the old visible revision
after a cut had saved but failed to refresh. If ordinary feedback had cleared
`committed`, that completion could consume the stale Visual selection and replace
the reopen warning with a copy-success message.

The service now supplies independent `saved_cut` state. `CutReceipt::needs_refresh`
identifies the same session/project still showing the captured pre-cut revision.
The UI may accept the newer historical copy, but retains that stale selection,
cursor, document and reopening warning. A receipt selects the saved join only
when its exact committed revision is visible, and duplicate delivery cannot
retarget later navigation. Undo's never-reused revision does not revive the old
warning. Service tests exercise an actual refresh fault; the new replay combines
genuine service capabilities with injected stale delivery ordering. The passing
`visual-cut-second` run includes this correction.

## Empty-source presentation and exact placement

The implemented Place slice flow gives an empty Child a structural source card
with its historical path, label, boundary and bounded nested group names. It
states that no pictures or audio are included. It schedules no source endpoint
decode, copied frame zero or zero-length audition. Source In/Out focus preserves
the accepted destination image and caption. An entirely empty destination has
an explicit empty-edit state.

`j/k` steps integer child slots even when several share one frame. The initial
slot can retain the selected empty child's identity, and controls name adjacent
children. `Prepared.empty_slot` validates the result against the retained request;
commit selects that exact fresh child without inventing a Visual range. Source
refinement, frame stepping, Move and Replace reject zero content with explicit
reasons. Positive whole-child Move remains available, including next to empty
siblings. An existing Move can always return to Copy, including after no-op
rejection.

Replay assertions cover a qualified Original beside three adjacent empty groups,
same-time slot changes, cancel, one zero-time commit and Undo. A second segment
removes the Original, repeats placement in the completely empty edit, and checks
that no picture or audition work is requested. Cut replays cover Visual `d`,
whole-beat `dd`, historical `p`, one Undo per edit, delayed and duplicate receipts,
newer register intent, stale requests, and the saved-cut correction above. These
checks pass in the focused rendered runs recorded below, with final captures
inspected at minimum and default sizes.

## Literal old Range compatibility

Core document schema stays **34** and database schema stays **43**. The closed
slice reader maps a missing selector only to the old Range interpretation. Range
serialization retains its old shape. Child uses an explicit tagged selector;
null, duplicate, unknown and inconsistent fields reject. Frozen historical
grammars are unchanged.

The checked-in [pre-selector fixture](../../crates/deadpan-store/tests/fixtures/edited_slice/README.md)
was emitted by the isolated unmodified implementation base above. Its 602,324-byte
JSON has SHA-256
`ea862010de94871087d282da55f982c0b967abb9f06d688af64f1dd7a5de6aca`.
It retains literal capture, request, complete transaction and output JSON for
seam, interior and replacement placement, plus the old writer's revision,
history, state and redo rows. Expected values were not regenerated by the new
implementation.

The compatibility test passes in both recorded store attempts and the completed
workspace run. It requires
byte-identical Range capture/request output, full transaction and result equality,
exact inverse, and deterministic recapture. It restores literal old rows into a
fresh package shell, validates and reopens read-only/read-write, then performs
both Undo/Redo steps across reopen. Existing history and revision strings must
remain unchanged. This is current-format compatibility evidence, not a migration
or permission to admit new commands through an older frozen grammar.

Other store checks reject coherent historical forgeries of child, parent, bounds,
label and media receipt without writes. A qualified asset retained only through
an empty group's unresolved Source mark is admitted from its exact historical
registration after that registration is undone. Zero source/proposed views keep
the existing opaque admission and session revocation boundaries.

## Independent media checks

The decoded audio regression uses the existing verified 44.1 kHz WAV at
30000/1001 fps. Its 16-frame destination includes Source, RoomTone, a cropped
nonunity Preserve stage, prior pause bindings, gain, marks and lineage, plus a
routed root sound with independent sample offset and a live Hold allowance.
Insertion at every root slot compares all **25,626 raw and authored PCM samples**
exactly with the original output. Separate source and RoomTone phase oracles
require real nonzero content. Cold first seeks into the Preserve crop, warm
repeats and shuffled reads exercise retained processing context. Both capture
and paste use maximum timing ordinal to expose unused allocation.

The picture-plan regression checks every frame across Sources, a Freeze with
captured geometry, framed Sequence content and a Repeat with Freeze gap.
Handwritten PTS, source ordinals, provider coordinates and owner-curve clocks
accompany complete before/after picture and framing equality. Every same-time
slot retains its structural position, and inverse application restores the
document. Both media regressions pass in the integrated diagnostic run below.
The picture test is synthetic plan evidence; it does not compare decoded or
composed GPU pixels.

## Diagnostic runs and corrected failures

Raw commands, logs, timing and source manifests are retained in the evidence
package's `checks/` directory. All listed commands use Rust
1.97.1 and locked dependencies. These runs precede final source freeze.

| Run | Recorded result |
| --- | --- |
| `core-first` | 31 core slice tests passed, none failed or ignored; 39.22 seconds including compilation. |
| `store-first` | Four focused store tests passed; one zero-tree fixture panicked on an omitted `audio_lineage` field. |
| `media-first` | Compilation failed before execution: the new audio fixture used `instance` where `Anchor::Local` requires `node`. |
| `integrated-first` | Selected app, audio, plan, CLI, playback and historical-media checks passed; the store zero-tree fixture still failed. Overall exit 101, 1,141.91 seconds. |
| `workspace-final` | Compilation failed on the new cut replay's `queried.clone()` because `ProjectUpdate` is not Clone. No tests executed; exit 101, 20.09 seconds. This label does not establish a final gate. |

`integrated-first` passed 21 selected app tests, 25 composite-audio tests, the
structural picture-plan test, two CLI tests, four playback tests and four
generation/source-registration tests. Four of the five new store tests passed,
including literal old Range replay. The missing-lineage fixture's first correction
used mutable JSON indexing, which inserted null rather than preserving omission;
the corrected test now uses `get_mut(...).and_then(Value::as_object_mut)`.
The audio fixture now names the proper local node. The replay now moves its
update into the existing Feedback delivery path instead of cloning it. These
fixture corrections pass in `workspace-second`, including the corrected
zero-tree store case. No production contract was weakened to satisfy them.

The core source manifest for `core-first` is
`55244b58b5a4f26912384b4424cd571095ddbab56cdf5f40a38622d3f453aee7`.
The integrated diagnostic manifest is
`471126dac5bd3e493823a59e138f9b13565cc28ec9e31e06bdae70323d0e8a9f`.
Later receipt and replay changes are outside those passing-source claims.

The first rendered cut run passed all 134 completed checks, including all 20
executions of the 14 new cut assertions. It then timed out because an older
nested-endpoint check expected `project_error`. The typed cut response correctly
reported the capture rejection through the register's public error. The corrected
check requires that exact error, an idle service/register, and unchanged document,
selection, accepted copy and durable history availability.

The first placement run passed 755 checks before its new no-op assertion timed
out. Its fixture contained empty siblings at frame zero, so destination slot zero
was a valid structural reorder, not the Source's original position. The replay
now selects the Source's exact sibling slot through production `d` and `j/k`
before refinement. The production unchanged-time label was also corrected:
it no longer claims every such move goes between groups. Both failed reports
remain retained; neither failure justified changing command admission.

The second placement run reached 763 checks and failed its empty-source card
assertion. Its lookup expected a control label, while the explanatory text was
painted text. The replay now uses `paint_text` and retains its exact content,
selected-slot and visibility checks. `visual-place-third` passes that assertion
and the complete placement scenario. The failed second report remains retained.

## Integrated and rendered verification

`workspace-second` passes **2,883 tests**, with **0 failed and 0 ignored**, in
**563.199 seconds**. It ran:

```sh
rustup run 1.97.1 cargo test --workspace --features deadpan-app/ui-harness --locked --no-fail-fast
```

Its source manifest is
`2ff5fa645251de51887575a0bafa8d71b59df3747a9b982e6617e2e844e68e1b`.
This includes the corrected store fixture and service saved-cut regressions.
Later source changes consist of harness corrections and one production control
label: “Timing unchanged…” replaces the incorrect claim that all unchanged-time
moves go between groups. The workspace result does not represent those later
inputs or the separate default-feature app configuration.

| Rendered run | Result | Source manifest SHA-256 |
| --- | --- | --- |
| `visual-cut-second` | 150 cut/deletion checks passed in 16.541 seconds | `ddb54d17e81a96be75533ec7b2c27645f06f7ab9d86ba4d2bea470a18f7f6436` |
| `visual-place-third` | 820 placement checks passed in 27.852 seconds | `55df165772764063e162c8b1eb397f8e691aff3f2a625bd39d4711355236d4c5` |

Each run also passes the Kestrel audit's **11,904 routing cases**. The placement
run covers existing Copy/Replace/Move behavior as well as empty structural
sources. Genuine qualified media decoding and offscreen Metal SDR rendering are
used; audio delivery is injected. The reports note that intermediate screenshot
capacity was reached while semantic frames continued and named checkpoints
retained reserved capture capacity. These debug timings do not qualify release
performance.

The root agent inspected cut captures 122/123 and placement captures 137–140 at
minimum and default sizes. The retained viewer is 281 and 461 points high,
respectively. Source-card text fits, destination pictures remain visible, and
the fully empty edit is shown explicitly. These retained offscreen images provide
the layout evidence.

## Native keyboard outcome

A temporary debug UI-harness bundle opened a private copy of the closed cut
replay package. Its executable SHA-256 is
`4c8329ccc9f2d698dd1e772b8a45d7695b103181c3cb3848d829e8213a9b7f02`,
matching `visual-place-third`.

Real keys entered the outer and inner Sequences and cut `[20,30)`. The retained
join at Edit 20 showed Original slate 030. Opening `:splice` showed copied
endpoints 020 and 029; Escape changed no authored state. `p` after the remaining
beat inserted `[110,120)` and showed slate 020. Two successive `u` commands
removed the paste and restored the cut. After `gg`, whole-child `y` captured the
100-frame child `[20,120)`; its placement endpoints showed 020 and 119. That
preview was cancelled, then Cmd+Q exited the application.

The evidence package's `native/verification.json` records that cancel and whole-child copy/cancel
preserved all **20 database tables**. Cut and paste each added exactly one
revision and one history entry. Each Undo restored every authored field except
the required fresh revision, and **16 unrelated tables** remained identical
throughout. The temporary process was absent afterward and its writer lock was
released.

The root agent inspected the live CUA screenshots, but those screenshots were
not retained. Their right side was partly clipped by the capture/window
placement, so this check makes no native full-layout claim. The retained
offscreen captures above cover layout. This manual check exercised positive
range cut and whole-child copy; zero-duration placement is covered by the
rendered replay and backend checks. It adds no device listening, native IME or
complete accessibility evidence.

## Final gates and source reconciliation

Final implementation source manifest:
`55df165772764063e162c8b1eb397f8e691aff3f2a625bd39d4711355236d4c5`.
The evidence collector rehashes every input against this manifest before
collection. `post-workspace-source-delta.json.gz` records all four files changed
after the full workspace test: three harness corrections and the one control
label described above. No core, store, media or service implementation changed
after that gate. The final-source gates and full replay cover the later app inputs.

| Evidence | Result |
| --- | --- |
| Final formatting and strict all-target workspace Clippy | Passed on final source: `cargo fmt --all -- --check` in 1.859 seconds and `cargo clippy --workspace --all-targets --features deadpan-app/ui-harness --locked -- -D warnings` in 675.638 seconds, both through Rust 1.97.1. |
| Default-feature app tests | Passed: 383 unit and 3 headless tests, none failed or ignored, in 133.146 seconds including compilation. `rustup run 1.97.1 cargo test -p deadpan-app --locked` uses final source. |
| Release build | Passed on final source in 240.204 seconds: `rustup run 1.97.1 cargo build -p deadpan-app --release --features ui-harness --locked`. |
| Full release replay | Passed all 3,434 checks across 20 active UI scenarios plus the 11,904-case Kestrel audit, counted as one check. No findings, failed timing samples or timeouts; command wall time 18.314 seconds. |

The release binary SHA-256 is
`7bf2985576410c9a13b46e497aeeaa2bcb156e1555fce17ddcba4549a1a0d470`.
On Apple M5 Max, macOS 26.5.2, warm navigation-to-picture completion p95 is
**1.517 ms** over 120 samples. Cached Repeat and deterministic Hold completion
p95 are **5.844 ms** and **5.785 ms**, each over 40 samples. These measurements
include offscreen GPU completion, with no readback during the timing run. File
cache, power and thermal state are uncontrolled. The complete report retains
all samples, cold startup, service timings and per-scenario limits. Accepted
Generated Hold picture replay remains explicitly skipped without its separate
real bundle fixture.

The collector retains commands, reports, native snapshots, inspected images and
source manifests under `tools/media-qualification/evidence/2026-10-01-structural-capture`.
`SHA256SUMS.json` binds every collected file except itself. The native application
was closed after testing and its writer lock was available.

No physical device listening, native IME, complete
accessibility, physical-display color or encoded-export qualification follows
from these tests. Persistent/named registers, role-only placement and temporal
occurrence interiors remain outside this increment. No DP requirement or
delivery gate is marked complete by this record.
