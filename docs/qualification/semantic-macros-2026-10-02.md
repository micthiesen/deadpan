# Semantic macro qualification, 2026-10-02

This increment adds native recording and execution of relative frame motions,
frame cuts and named Macro calls. `qa` starts recording into `a`, `q` saves,
`@a` runs it and `3@a` runs it three times as one undoable edit. Escape discards
the recording draft while keeping edits already performed during recording.
The [contract](../SEMANTIC_MACROS.md) records exact scope and limits.

## Correctness boundaries

The pure planner resolves each instruction against the preceding staged
document and register bank. Counted calls freeze their body at entry. Cuts
retain their requested count and destination register; later calls observe
earlier register writes. Recursive calls, exhausted fuel, invalid types,
unsupported scope and a failure late in the program reject the entire run.
One admitted Compound commits the edit, history and final register bank.
Undo/Redo retains its resolved instructions and intermediate copy provenance.

Macro recording captures the session, revision, register version, ordinary
Sequence and cursor before a prefix or command can complete. Absence is also
captured. Asynchronous cuts and calls enter a recording only after their exact
successful receipt. Saving a Macro preserves the unnamed copy and Undo/Redo.
Motion-only programs change no authored state. Delayed results cannot reclaim
the cursor after observed navigation away and back. Durable success survives
workspace refresh failure with explicit reopening guidance.

SQLite schema 55 adds typed Macro contents without media capture provenance.
Unused schema-54 packages require recreation under the authorized development
policy. Core document schema remains 43. The CLI diagnostic reports the new
schema and partial native macro capability. Dedicated headless Macro commands
remain open.

## Native keyboard and visual inspection

The developer bundle ran the retained 120-frame `cfr-bframes` project. Native
Shift+2 produced the `@` prefix and visible next-key guidance. Running `@a`
produced one undoable edit. Recording `qZ`, `2l`, `x`, `q` saved the normalized
register `z` with exactly two typed instructions. After Undo, `@z` moved two
frames and cut one frame. One Undo restored all 120 frames while retaining the
copy. Read-only SQLite inspection confirmed the exact program and both null
capture-provenance columns.

Root inspected the native 2560×1704 capture after Undo: picture 005, separate
Original/Edit clocks, copied range `[5..6)` and visible footer controls. The
recorded native observations retain the exact accessibility labels. Rendered
replay captures inspect recording controls and the counted result at 960×640
and 1280×820 logical points.

Cmd+Q closed the test process. A later observation through the bound CUA app
handle reopened an empty window; that window was also closed. Process and CUA
app inventories then contained no Deadpan instance. Native interaction occurred
while workspace binaries compiled, so it supplies no latency measurement.

The native check predates the final CLI diagnostic correction, boxing of the
optional native commit receipt and four strengthened test files. Source
inventories retain those exact changes. Final app suites and rendered replays
exercise the boxed receipt; native keyboard routing remained unchanged.

## Review and retained failures

Cross-layer peer review covered the native recording/receipt path, separately
authored service/store boundary and pure planner. Review found that delayed
cursor ownership could return after navigation away and back. Root changed it
to revoke ownership permanently for that pending operation and retained a
deferred-service replay witness. No further concrete findings remained.
Fresh reviewer creation hit the harness thread limit; these were peer reviews
across separately authored layers. A further test review led to full-document
Undo/Redo comparisons at the store/service boundary and specific size-limit
errors that cannot pass through unrelated schema rejection. Root also reviewed
the source diff.

The initial compile found one store test using the removed `revision()` method;
it now checks `capture_revision()`. Unused native wrapper and receipt fields
were removed. The first workspace run reached a stale diagnostic assertion
expecting database schema 54; it now expects 55 and checks the new capability
labels. Strict lint then found an oversized macro-result enum; its optional
commit receipt is now boxed. The first boxed build exposed two test comparisons
still using `as_ref`; those now use `as_deref`. All failed runs and corrected
results are retained.

The macOS debug linker reports an oversized `__eh_frame` section and warns that
exception handling performance may be affected. This warning is retained;
release packaging and performance qualification remain open.

## Verification evidence

The [evidence bundle](../../tools/media-qualification/evidence/2026-10-02-semantic-macros)
retains commands, compressed logs and replay reports, complete source inventories
including untracked files, three reviewed images, native observations, host
details, review notes and SHA-256 checksums. The host was an Apple M5 Max with
128 GiB RAM, macOS 26.5.2, Rust 1.97.1 and the explicitly selected
`/tmp/deadpan-ui-ffmpeg/prefix`.

| Check | Observed result |
| --- | --- |
| Full workspace, locked dependencies | 3,633 tests passed; none failed or ignored. |
| Final default app | 645 unit tests and 3 headless-entrypoint tests passed. |
| Final app with UI harness | 681 unit tests and 3 headless-entrypoint tests passed. |
| Strengthened core/store macro assertions | 9 filtered macro tests and the separate wire-budget test passed. |
| Strict Clippy | Changed crates with UI harness, all targets, and default app all targets passed with `-D warnings`. |
| Formatting | `cargo fmt --all -- --check` passed. |
| Rendered macros | 126 workflow checks passed. |
| Rendered configurable keymap | 57 workflow checks passed. |
| Rendered dot-repeat | 160 workflow checks passed. |
| Rendered named registers | 170 workflow checks passed. |
| Rendered selected-range deletion | 310 workflow checks passed. |
| Kestrel production routing | 487,568 cases across 62 globals passed; live source and fixture digests matched. |

The five final replays total 823 workflow checks, plus their repeated shortcut
audit assertions. They used binary SHA-256
`97de2105433d8954246bae162d81f3c0ca99a1e393f703b78d799350857e0e43`.
All final checks retained unchanged source manifest
`e0457860ceed5bf3db6e85efc72a575567fdbe69ee5f35193c08615157faae02`.
The earlier full workspace run used
`1bf6258fd629150117eab85f007f66e7059180af683b291b8378f430faea7acc`;
only the two receipt-boxing files and four strengthened test files changed
afterward. Both final app suites, focused assertions, lint and replays cover
those changes. Counts from overlapping suites are reported separately.

Macros, dot-repeat, named registers and deletion reached the intermediate
screenshot allowance. Semantic checks continued, and the reserved named
checkpoints were retained and reviewed. Replay scripts select picker paths;
they do not exercise the OS picker, physical display presentation, VoiceOver,
OS IME delivery or audio device output. Native input observations supplement
the current-layout macro path only.

The retained [recording image](../../tools/media-qualification/evidence/2026-10-02-semantic-macros/images/macros-027.png)
shows the four-instruction body and save/cancel controls. The
[counted result](../../tools/media-qualification/evidence/2026-10-02-semantic-macros/images/macros-045.png)
shows eight removed frames, Edit boundary 42 and displayed Original picture 50.
Final process inventory found no running Deadpan app.

## Remaining product work

DP-06 still needs broader editing instructions, semantic text/range selectors,
temporal occurrence contexts, dedicated headless Macro management/execution and
dot-repeat beyond frame cuts. A macro edit currently clears the older frame-cut
dot candidate. The current native keyboard layout passed; the full physical
layout and IME matrix remains open. This increment completes no DP requirement
or delivery gate and makes no full-editor, mastering, model, export, packaging
or release acceptance claim.
