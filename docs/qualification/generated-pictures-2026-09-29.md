# Accepted Generated Hold pictures

Accepted schema-3 Generated Holds now prepare pictures through one shared reader
in native preview and captured-revision rendering. Playback uses the retained
sampled master and needs no generation model or worker files. It preserves the
accepted sampling allocation and captured framing through prefix resizing,
request staleness and historical reads. Full generation, export and release
acceptance remain open; no DP requirement or gate closes.

See the [picture contract](../PROJECT_PICTURES.md),
[store reader](../GENERATED_MEDIA.md) and
[retained evidence](../../tools/media-qualification/evidence/2026-09-29-generated-pictures/README.md).

## Implemented boundary

The plan carries the effective Hold's complete artifact identity through ordinary
Holds, Repeat gaps and sparse play overrides. A revocable store handle pins the
package namespace without retaining SQLite or the writer lock. Cold preparation
checks all six objects, strict schema-3 provenance, the retained capability and
sampling plan, both asset records and the freshly decoded sampled master. It
checks original millisecond PTS and the observed terminal duration. It does not
substitute the current candidate, request relevance or a nominal frame duration.

Both consumers use one retained decoder and private verified media. Native cache
identity includes project session, artifact, both asset records and color policy.
Revision/framing changes can reuse the same immutable input. Closing the owning
store revokes further cold and warm preparation. Already returned owned pixels
and snapshots retain their bytes. Missing or corrupt dependencies fail cold
admission; a warm private snapshot survives damage to the package copy.

Legacy Accepted providers without durable evidence, Still and HDR remain
explicit errors. The change adds no project/database schema and does not expose
an inference or acceptance control that lacks an implementation.

## Verification

Base commit: `32e0d4efa574117bf57940b54646cf03952b79ba`.
Host: Apple M5 Max, 128 GiB, macOS 26.5.2 (25F84), Rust 1.97.1, wgpu 30.0.1
and the pinned LGPL FFmpeg 8.0.3 development prefix. Power/thermal state and OS
file cache were uncontrolled. Source inventories and command journals bind
each result to the files tested.

The locked workspace run passes 2,094 tests across 156 target result records,
with zero failed or ignored, including actual conversion and decoded-media
integration. Optional app/harness tests pass 304; base-app and harness paths
are checked separately. Strict workspace/all-target Clippy and optional app
Clippy pass. Two earlier digest-format lint failures and their corrections
remain in the command logs.

The real bundle test converts the committed `rgb25_24.mp4` fixture into native
and sampled FFV1 masters, explicitly accepts it, relocates the package and removes
worker files. It independently checks all 30 sampled 4×2 RGBA frames at
30000/1001, source PTS and captured geometry. It exercises undo/redo, a 12-frame
prefix, re-extension to 30, fallback reversion, a cold historical read after
request staleness, and the original captured session after writer close. Each
of six objects is removed and corrupted in turn: cold reads reject it and the
existing private decoder retains the expected pixels.

Store tests cover independent reader revocation, worker-thread reads, released
writer locks, pinned namespaces after package-path replacement, cancellation,
byte/deadline bounds and immutable snapshots. Provenance tests reject duplicate
and unknown fields, legacy envelopes, rehashed semantic substitutions, mismatched
project/artifact/sampling, modified retained context and excessive nested data.
Plan tests cover accepted prefix identity, Repeat gaps and isolated plays. Native
cache tests cover changed asset interpretation and color policy.

## Native replay and visual review

The optional `generated-picture` scenario opens the integration test's retained
compatibility fixture through the production project service. It enters Sequence
through command mode, navigates every sampled frame with the normal keyboard
router, checks exact pixels/PTS and submitted picture identity, and resizes
between 1280×820 and 960×640. It verifies history is unchanged. The fixture is
explicitly a generic project with no Original, so this run does not establish
the single-Original creation or AI acceptance workflow.

The initial image review found that Picture and Sound were below secondary
treatments in the inspector. The final layout places them directly below
Duration and labels the qualified provider "Accepted AI". The duration action
also remains above secondary controls. Painted-text checks cover every label
and the duration action at both sizes, including actual clip visibility.

The first corrected release replay passed its 99 visual checks, and optional
app tests passed 304. Image inspection nevertheless found that the first footer
row was covered by later panes. Its text had a valid clip, so the old visibility
check incorrectly reported it visible. A stronger check now also examines the
solid interiors of later opaque rectangles in final paint order. With that
check, the unchanged layout fails because NORMAL is entirely covered.

The inactive Gain panel kept 24 pixels of margins and a separator despite
requesting zero height. Its response moved the remaining parent boundary down
25 pixels, letting Sources and Inspector cover the footer. Inactive Gain and
compact Sounds panels now keep their widget IDs with no frame or separator.
The corrected replay and actual captures show the mode, context and focus row.
The minimum compatibility fixture's picture is about 90 points high; improving
that legacy layout's picture priority remains open.

The ordinary visual run then exposed a room-tone harness assumption: it stopped
scrolling after revealing the sample range, before its two action buttons were
visible. The loop now checks that whole four-label group. Its next assertion
caught a real consequence of restoring the footer: the minimum copied-Original
picture fell to 117 points. The read-only Original/edit clocks now use a compact
14-point row there, and the reserved controls area shrinks accordingly. The
corrected replay retains 141 points of picture, visible footer/controls, stable
widget identities across scale changes, and the existing 140-point threshold.
Both failed runs remain in the evidence.

Only app layout, inspector labels and replay checks changed after the full
workspace gate. The retained backend results remain applicable. App checks and
the ordinary visual run with a scoped room-tone continuation and full release
performance replay cover the shared panel correction;
the explicit Generated fixture runs separately. Results are recorded below.

| Check | Actual result |
| --- | --- |
| Locked workspace | 2,094 passed, zero failed/ignored; backend source unchanged afterward |
| Final base app / optional harness | 268 / 304 passed, zero failed/ignored |
| Final formatting and strict app Clippy | Pass in both feature configurations; earlier strict workspace check retained |
| Generated visual | 108 checks plus shortcut audit pass; actual default/minimum images inspected |
| Ordinary visual | 1,221 checks with one room-tone failure; corrected room-tone continuation passes 220 plus audit |
| Final ordinary release replay | All 2,348 checks pass, including shortcut audit; Generated fixture explicitly skipped here |
| Final Generated release replay | 367 checks plus shortcut audit pass |

The final release fixture records one cold input-to-picture completion at
4.973083 ms. Over 120 warm inputs, CPU p50/p95/p99/max is
0.132667/0.142166/0.154375/0.155834 ms; input-to-picture completion is
0.902250/1.002125/1.119292/1.245416 ms. These are offscreen completion timings
for a 4×2 master, not full-resolution or physical display performance.

The workspace source inventory is `97a8c033…`; final app checks, room-tone
continuation and both release runs use `1803fb62…`. Only seven app layout and
harness files differ. The Generated visual capture uses `c434a299…`; subsequent
production changes affect only the single-Original compact clock row, absent
from that compatibility fixture. Complete hashes and exact source deltas are
retained in the evidence.

The [workspace target](../design/boards/single-source-workspace-v1.png) guides
picture dominance, separate policy fields, selection and focused-pane feedback.
Final captures retain the picture, distinct focused-pane status, selected Hold,
visible policy and duration action at both sizes. The compatibility fixture's
empty Sources column and synthetic color image do not reproduce the board's
single-Original subject, thumbnails or finished operation set. Those remain
separate UI requirements.

Independent review found no remaining defect in historical artifact identity,
retained-object admission, sampled timing, handle revocation or native cache
identity. It also reviewed the footer and room-tone corrections. The paint
heuristic can conservatively flag blank space within a text galley and does not
prove arbitrary mesh, rounded-edge or translucent occlusion. Actual image
inspection remains required; the final default/minimum captures were inspected.
See the retained `review.md` for scope and limits.

## Remaining acceptance

This is real canonical-media decoding and offscreen Metal presentation, using a
synthetic fixture. It does not establish AI generation quality, source-scene
continuity, an in-app model manager, candidate audition/acceptance, audio output,
full-resolution throughput, physical display latency or offline MP4 export.
VoiceOver, physical keyboard/IME and non-US-layout acceptance were not repeated
for this reader and inspector-order change. No native startup/shutdown callback,
text field or keybinding changed.

Final-render process isolation, automatic legal encoder geometry, the full shared
picture/effect and audio graphs, approved AAC timing metadata, emitted-file
verification, atomic publication and native one-action Render remain required.
The complete model corpus, hardware tiers, recovery/chaos suite and signed
clean-machine online/offline distribution also remain required.
