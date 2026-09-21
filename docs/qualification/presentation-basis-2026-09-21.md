# Presentation basis qualification, 2026-09-21

This slice adds automatic project basis state, first-primary picture selection,
timed-edit locking, explicit canvas changes and qualified primary geometry
adoption. It is authored/headless behavior; native project import and visible
canvas previews remain open. [Presentation basis](../PRESENTATION_BASIS.md)
defines the policy and its scope.

The run used Apple M5 Max, 128 GiB, macOS 26.5.2, Rust 1.97.1 and pinned LGPL
FFmpeg 8.0.3 at `/tmp/deadpan-media-compatible-xyhilms4/prefix`.
The [source manifest](../../tools/media-qualification/evidence/2026-09-21-presentation-basis/source-manifest.json)
records implementation and fixture hashes against source-registration revision
`14c75218b695e732773a4b247a45db3b7290faac`, which passed
[CI run 35645569091](https://github.com/micthiesen/deadpan/actions/runs/35645569091).

## Exact authored behavior

New automatic projects are provisionally 1920×1080 at 30 fps. Fourteen focused
core tests cover first-primary adoption, fixed-rate preservation, source-qualified
identity binding, explicit canvas changes, provisional-state validation,
frame/sample/mark preservation, patch preconditions and exact inverse restoration.
Labels, source registration, source-clock marks and true no-ops do not lock the
clock. Inserting audio, a Hold or secondary picture does lock it. Deletion does
not reopen automatic selection; undo restores the actual earlier policy.

This implements the first-time-based-edit rule in specification Section 22.2:
inserting an audio beat authors time and therefore locks the default rate. A later
first-primary picture records its identity without retiming that audio. Geometry
can then be adopted through an explicit, independently previewable transaction.

Five store integration tests use actual persistent source decoders and retained
originals; a sixth checks false origin claims in a stored initial snapshot.
Registering CFR media first does not choose the basis. Inserting the
rotated fixture as the first primary selects its measured 24 fps and 2×4 display
raster; source placement is derived at that final rate. Later primary insertions
retain the first source's basis and identity. Read-only preview coexists with
the writer, and undo/redo/reopen retain the expected policy.

The 44.1 kHz audio-only fixture locks the project at 30 fps. Later portrait
insertion and geometry adoption leave its Source node, marks, project rate and
sample boundary at frame 15 unchanged. Geometry remains previewable from the
retained qualification metadata when the original is offline; this does not
claim that offline media can play. Secondary picture insertion cannot choose
the basis. Generic commands cannot bypass host admission to claim source-derived
basis or geometry.

An injected SQLite history-write failure rolls back basis state, asset, receipt,
node, revision and history together, then permits retry with the uncommitted
revision. A tampered history with mutually consistent document/request/patch
geometry still fails validation against the immutable measured receipt.

The media geometry helper derives display raster independently of cadence.
Existing ambiguous-cadence tests now also verify that an otherwise valid raster
remains available for an explicit geometry decision at the fixed project rate.

Eight CLI source-registration subprocess tests include automatic creation,
primary/secondary intent and geometry preview/commit/undo. Geometry requests
require `protocol: 1`; unsupported or missing versions are rejected before
opening the project. Stale revisions and read-only writer coexistence are
covered. The host continues to require genuine generation relevance for writes
when current generation requests exist; preview does not fabricate that context.

## Historical preservation

The [fixture generator](../../tools/media-qualification/evidence/2026-09-21-presentation-basis/fixture/generate.py)
archives and compiles unchanged revision `14c7521`, whose doctor reports core
schema 9/database schema 14. That binary migrates the previous fixture, retains
and decodes CFR/VFR originals, and registers them using actual qualification
tokens. Undo and branch editing reuse an asset alias with distinct receipts.
The fixture retains 54 revisions, 28 history edits, two source qualifications,
three original records and one pending redo.

Two independent SQL dumps match byte for byte. The
[manifest](../../tools/media-qualification/evidence/2026-09-21-presentation-basis/fixture/manifest.json)
records SHA-256 `b1f91bcd279877ddd5c6b33d656ddd0faa55ed11e1bdc9e155191f40b4b3d112`,
old-binary validation, archive and harness hashes. Database schema 15 replays all
legacy chronology into core schema 10 and preserves every source receipt. Older
bases become explicit, including an empty default-sized project: no old field
proves automatic intent. Frozen adapters reject new fields, geometry commands
and presentation patches in legacy JSON, including unexpected null fields.

The 38-test migration suite covers chronology, branch/redo, receipt preservation,
corruption before promotion, backups and earlier schemas. The new core state
required synthetic legacy test fixtures to remove `basis_state` before parsing
them as old schemas. Those test-only setup mismatches were corrected before
the final gate; no separate raw failure log was retained.

## Verification scope

The repository gate passed formatting, workspace Clippy with warnings denied,
**572 Rust tests**, zero failures or ignored tests, workspace build and doctor.
The [gate report](../../tools/media-qualification/evidence/2026-09-21-presentation-basis/gate/report.json)
records commands and durations; compressed logs preserve the actual output.
[Independent review dispositions](../../tools/media-qualification/evidence/2026-09-21-presentation-basis/review.json)
are recorded alongside the evidence.

General and timing review found no defects. Storage review found that a supplied
initial snapshot could claim default geometry while storing a nondefault raster.
The core validator now checks that default geometry remains 1920×1080 under an
automatic project origin, and that a timed-edit rate remains 30 fps. Core and
store regression tests both failed against the pre-fix implementation; their
logs are retained. The reviewer checked the follow-up and reported no remaining
findings. The earlier 570-test passing gate is also retained under
`pre-review-gate` to distinguish it from final validation.

No native adapter, startup, focus or control changed. Native ASan/UBSan, lifecycle,
GUI aesthetics, IME, accessibility and keyboard navigation were not repeated for
this host/persistence slice. Existing [source-preview evidence](source-preview-2026-09-21.md)
retains its limited visual and keyboard scope. Python media qualification suites
were not rerun locally. Rust integration and subprocess tests decode actual media;
they do not establish listening, playback or export.

## Remaining work

Native import and basis/canvas preview controls, framing-effect reevaluation,
still-image import, legacy source requalification, the full format/color matrix,
source-aware generation context, audio output, playback and rendered-file
equivalence remain open. Full product requirements and delivery gates remain
open or partial.
