# Macro copy and paste qualification, 2026-10-02

## Scope

Native recording and headless Macros now support selected-beat yank and
before/after register paste. Original and Edited copies share the staged
planner, independent explicit child selection and ordinary Compound admission.
A counted authored run commits once. A copy-only run saves its register bank
without changing the document revision, Undo or Redo.

The planner keeps child selection separate from the absolute Edit cursor,
including missing selections and zero-duration siblings. Motion uses native
right-biased selection; a terminal motion selects the final child. Paste selects
a fresh imported root, including the wrapper around an empty copied group.
Later instructions resolve against that updated selection and document.

Original mappings are derived from the saved measured qualification, then
independently checked by the store. Small per-request caches retain only the
derived Source mapping, not the potentially large receipt. Each pasted leaf is
still checked against that exact mapping. The cache has no authority across
requests or transactions.

Core schema remains 43 and database schema remains 55. This advances DP-06 and
DP-21; neither requirement nor any product gate is complete.

## Environment and identities

- Apple M5 Max, 128 GiB RAM, macOS 26.5.2 (25F84), arm64.
- Rust 1.97.1 (`8bab26f4f`), Cargo 1.97.1 (`c980f4866`), locked dependencies.
- FFmpeg development prefix: `/tmp/deadpan-ui-ffmpeg/prefix`.
- Base commit: `62ccfacdfdb392a5de8ce84ecc547a53c1fcfd66`.
- Final complete source inventory SHA-256: `163264c94906e2ce43cf178fcb1493f05342871744d333d4d21ca83f6a9d8cca`.
- Final `ui-harness` binary SHA-256: `e5e6850d32e0aab81b06d8c366b4a614cae142876d4c7eb45ecb5361fb34d927`.

The [evidence directory](../../tools/media-qualification/evidence/2026-10-02-macro-reuse)
retains command metadata, complete compressed logs, before/after source
inventories, replay results, selected captures and peer review notes.
`SHA256SUMS` covers retained evidence, excluding itself. Scratch projects and
authenticated discovery secrets are excluded.

## Checks

| Check | Result |
| --- | --- |
| Core, store, CLI and default-feature app tests | 2,481 passed across 101 suites; none failed or ignored |
| Final default-feature app checks after test synchronization fix | 659 unit and 4 headless tests passed |
| Final app checks with `ui-harness` after test synchronization fix | 695 unit and 4 headless tests passed |
| Strict all-target Clippy across affected crates | Passed |
| Final app Clippy with default and UI features | Passed |
| Workspace formatting and final `ui-harness` build | Passed |
| Rendered Macro workflow | 188 checks passed across 721 steps |
| Production Kestrel compatibility audit | 487,568 routing cases, 62 reserved bindings, no conflicts or source drift |

The Kestrel audit compared live source SHA-256
`368c01df72ae4fab2efa4d38b235b56c02251f8b895f7e6402c77f6a151723c2`.
The replay used the qualified `cfr-bframes.mp4` fixture and real Metal pipeline.

Coverage includes:

- Explicit absent, wrong and empty-child selections; terminal empty siblings
  and all-empty groups; before/after placement and empty destination slot zero.
- Staged selected-beat captures, intermediate historical provenance, fresh
  authored identities and exact wrapper/descendant graphs after repeated paste.
- Complete document Undo/Redo comparisons, fresh outer revisions, checkpoint
  and reopen; copy-only execution preserves an existing Redo branch.
- Counted and nested calls with frozen active bodies; staged register writes,
  strict typed decoding, fuel/step bounds and exact allocation pools.
- Original qualification and mapping admission, including a forged second
  paste rejected on a cache hit without writes.
- Bank-only refresh failure retains the exact copy and receipt. Pure paste
  reports its authored revision without claiming an unchanged bank was saved.
- Late native replies cannot take over a changed project, cursor, selection or
  bank. Command-entry recording ownership survives automatic cancellation.
- Recording a selected-beat copy and both paste directions; failed empty-slot
  paste does not enter the body; counted Edited and Original reuse; exact
  displayed source frames, bank state, selected roots and one native Undo.

The main agent inspected these 960 by 640 captures:

- [Recording copy and both paste directions](../../tools/media-qualification/evidence/2026-10-02-macro-reuse/screenshots/macros-124.png):
  three instructions are recorded, the new group is selected at Edit 120 and
  the picture shows Original frame 0. Recording controls and footer are visible.
- [Counted Edited reuse](../../tools/media-qualification/evidence/2026-10-02-macro-reuse/screenshots/macros-125.png):
  five editable beats span 600 frames, the new group is selected at Edit 240,
  and completion reports one Undo.
- [Counted Original reuse](../../tools/media-qualification/evidence/2026-10-02-macro-reuse/screenshots/macros-126.png):
  three three-frame copies follow the baseline. The final selected copy starts
  at Edit 126 and displays Original frame 10; both clocks remain labelled.
- [Reopened inventory](../../tools/media-qualification/evidence/2026-10-02-macro-reuse/screenshots/macros-127.png):
  the saved `e` and `o` Macros retain three and one instructions respectively.

The replay reached its intermediate screenshot allowance; semantic checks and
named captures continued. Debug linking reported the existing `__eh_frame`
size warning. Neither warning was suppressed. No release-build or performance
claim follows from this run. The final process check found no `deadpan-app`
process running.

## Review and corrected failures

Cross-layer peers reviewed the core planner, native input/service, CLI planning,
store mapping cache and rendered assertions. This was review within the existing
agent team, not a fresh-context full-branch review.

Review found that pure paste incorrectly reported a bank-write receipt even
though the bank had not changed. The host now reports that receipt only when
the saved bank version changes; CLI and live-owner tests cover the distinction.

An initial development check rejected use of the private `EditError::new`
constructor. The host now constructs the existing public fields; visibility
was not widened. That diagnostic is transcribed in the review record.

The first recorded affected-crate run passed 657 app unit tests and failed two
new assertions. Both expected an inner copied beat where paste intentionally
selects a new `Copied contents` Sequence wrapper. The corrected tests verify
the exact wrapper and descendants, historical selection, fresh identities and
preserved empty structure. Production paste behavior did not change for this
correction. The rejected run is retained.

The first UI-feature run passed 694 unit tests and failed the existing
interrupted-initialization test. Its generic wait accepted an unrelated cancelled
worker publication after the actor cleared its busy flag. The test now waits
for the reopened session and revision's explicit stale-session rejection,
then checks that no new worker job was queued before retrying. The production
guard still rejects the stale session before starting work and was unchanged.
The final app checks were rerun after this test-only correction. Core, CLI and
store sources stayed identical to the earlier passing full run, whose source
inventory was `0292077b245bb0613ea4991cf63e448762657dbeda7e352512b6713c902d866a`.

The first isolated retry used a short test name with `--exact` and ran zero
tests; it is not counted as verification. The corrected fully qualified retry
passed the one test before the synchronization change. Both logs are retained.

The first rendered replay expected register `z` to be empty, but an earlier
scenario had saved a one-frame Edited slice there. Deadpan correctly pasted
that slice. The replay now asserts that unused register `w` is empty before
testing refusal. Only the replay fixture changed after the final app unit and
headless tests, whose source inventory was
`56ee38fd85754540e564eaf03bec3ea5410242638b4035d6d85c1156bcd9b7df`.
Strict UI lint, formatting, the UI build and rendered replay were repeated.

Before strict lint, the planner's coherent register map and observed version
were grouped in `SemanticRegisterBank`, leaving seven arguments. No lint
suppression was added. The retained source inventories identify each checked version.

## Limits

Yank/Paste byte-budget refusal paths were reviewed and reuse the existing
Compound accounting; direct byte-budget tests currently exercise Cut. New tests
exercise Yank/Paste fuel, step and identity refusal paths.

Rendered replay uses scripted picker paths and offscreen native event routing
with the real project service and Metal pipeline. It does not qualify physical
keyboard layouts, OS IME, VoiceOver, physical display color or acoustic output.
No new live native window session was needed for this increment. Broader Macro
instructions, text/range selectors, temporal occurrence scopes, full dot-repeat
and all remaining product requirements stay open.
