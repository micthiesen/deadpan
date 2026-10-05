# Incremental commit and verified history open, 2026-10-05

[The commit path and history receipts](../TIMING_STORAGE.md#in-memory-head-and-validation-reuse)
remove the whole-document parse, repeated validation and serialization from
every commit, and replace replay-on-open with a hash-verified receipt. These are
engineering measurements on one machine, not release qualification.

## Environment and identity

| Item | Value |
| --- | --- |
| Hardware | Apple M5 Max (Mac17,7), 18 cores, 128 GiB, AC power, no thermal or performance warning |
| Toolchain | rustc 1.97.1, release, FFmpeg prefix `/private/tmp/deadpan-ui-ffmpeg/prefix` |
| Source | HEAD `048676ba` plus this change and another agent's concurrent preview-proxy work in the shared checkout (media, media worker, app preview and xtask seek stage; none on the edit or open path) |
| After | `cargo xtask perf --stages edit --generate`: `deadpan-cli` `9c2793d7…`, `perf` `8a669d86…`. Every edit stage started at 1-minute load 2.91–2.99 (`--max-load 3`), none flagged. Another agent's single-threaded benchmark ran during parts of the window. |
| Before | The HEAD `perf` and `deadpan-cli` (`048676ba`) on a fresh `large-10000` fixture the same morning, at load 4–7 (not gated); and the [timing-storage record](timing-storage-2026-10-05.md) for the generated fixtures |

## Edits (30 cycles each)

p50 / p95 ms of commit plus workspace refresh (`total_ms`), as in earlier
records. The refresh now takes the store's validated head and compiles its plan
in the retained validation scope, as the native workspace does.

| Package | Split | Pause | Wrap | Undo | Database |
| --- | --- | --- | --- | --- | --- |
| gen-1080p60, before ([timing-storage](timing-storage-2026-10-05.md)) | 7.5 / 16.1 | 16.1 / 24.9 | 7.0 / 11.0 | 6.9 / 10.1 | 9.2 MB |
| gen-1080p60, after | 4.3 / 4.6 | 14.9 / 16.1 | 4.0 / 4.9 | 4.1 / 4.2 | 7.1 MB |
| gen-4k30, before ([timing-storage](timing-storage-2026-10-05.md)) | 10.1 / 19.0 | 16.4 / 24.3 | 9.2 / 16.0 | 8.7 / 20.8 | 8.1 MB |
| gen-4k30, after | 4.5 / 5.1 | 12.9 / 14.9 | 4.1 / 5.2 | 3.9 / 4.9 | 6.1 MB |
| large-10000, before (HEAD binary) | 415 / 456 | 651 / 683 | 359 / 380 | 349 / 372 | 174 MB |
| large-10000, after | 47.0 / 52.6 | 135 / 163 | 47.5 / 52.8 | 24.7 / 28.0 | 102 MB |

All generated-fixture rows PASS the 50 ms edit and 100 ms Hold targets; most
of the remaining pause time on them is the harness's own pause-provider
preparation (7–11 ms), not the commit (5–7 ms). On 10,000 beats undo PASSes and
split and wrap FAIL narrowly (p95 52.6 and 52.8 ms); pause FAILs at 163 ms.

10,000-beat breakdown (p50 ms; the harness's own command preparation is the
rest of the total):

| Phase | Split | Pause | Wrap | Undo |
| --- | --- | --- | --- | --- |
| Store commit, before | 269 | 483 | 236 | 228 |
| Store commit, after | 37.5 | 124 | 39.8 | 16.7 |
| Workspace refresh, before | 122 | 122 | 121 | 120 |
| Workspace refresh, after | 8.6 | 9.0 | 7.6 | 7.8 |
| Command preparation, before | 22.5 | 44.6 | 0.06 | none |
| Command preparation, after | 0.5 | 0.7 | 0.06 | none |

The refresh was a head parse plus a validating plan compile; it is now the
cached head and a plan compiled in its validation scope. The harness's split
and pause preparation validated a copied document to find a target; it now
runs inside the head's validation scope, so that cost is no longer included.

What remains is whole-document work inside the core commands, not storage:
the result's complete validation (about 9 ms), Split and pause validating their
intermediate document again, audio lineage reconciliation, mark transforms and
the patch diff each walking all 10,000 nodes, the commit's `F_FULLFSYNC`
(about 5 ms) and, for a pause, capturing and serializing the complete binding
state.

## Open

| Package (123 revisions after 30 cycles) | Before | After |
| --- | --- | --- |
| large-10000, read-only open and head (`project dump`) | 23.0–24.0 s | 0.41–0.44 s |
| large-10000, writable reopen (`perf edit` `reopen_writable_ms`) | 28.9 s ([record](timing-storage-2026-10-05.md)) | 279 ms, plus 150 ms for the first head read |
| large-10000, `project validate --full` | 45.7 s (`project validate`, which replayed twice) | 7.2 s |
| gen-1080p60 / gen-4k30, writable reopen | not measured | 34 / 23 ms |

Every reopen reported `verified_by_receipt` equal to the revision count and
`replayed: 0`. Opening still hashes every stored row (102 MB on 10,000 beats);
the head read rebuilds it from its keyframe through at most 63 patches and
validates it once.

## Correctness evidence

- `deadpan-store/tests/revision_storage.rs`:
  - `keyframes_bound_every_chain_and_rebuild_every_revision`: 140 revisions of
    edits, pauses, undo and redo store documents only at depth 0, and every
    `json_bound` covers the real compact document. Every revision rebuilds
    exactly after reopening read-only and writable, with nothing replayed.
  - `random_commits_equal_full_validation_and_storage_rebuilds`: four seeded
    sequences of pauses, Splits, Repeat wraps with and without gaps, ripple
    deletes, Hold duration changes, undo and redo on bound Holds. After every
    commit the cached head equals the validating patch application, its
    inverse restores the previous revision, complete validation accepts it, an
    independent reader rebuilds the same document from storage without
    replaying, and the stored bound covers it; full replay accepts the end.
  - `receipts_replace_replay_and_commits_extend_them`: commits extend the
    receipt (nothing replayed); a deleted receipt replays all eight edits until
    a writer recertifies; another validator build's receipt proves nothing.
  - `a_receipt_never_hides_a_modified_row`: a changed edit description,
    command, navigation patch, revision kind, size bound or cursor, and a
    missing patch row, are rejected by read-only and writable opens; a
    reformatted but equal command breaks the chain and is replayed and accepted.
- `persistence.rs` `semantic_validation_rejects_forged_history_and_state` adds
  forged head, navigation patch, bound and depth rows to the existing cases.
- `deadpan-core` `validated_tests`: results of `apply_validated` through
  pauses, a gap Repeat wrap, a Hold duration change and a ripple delete have
  the same durations and binding proof as complete validation, with at least
  six owners' checks reused per edit; changed bindings, removed tables and a
  retyped owner are rejected by both paths. Debug builds compare the
  patch-derived changed owners with the complete comparison on every commit.
- Suites: core, store and plan 1,647 passed; audio and playback 530 passed.
  After the final navigation change: core and store 1,307 passed, and the CLI
  and app (base features) 1,296 passed, including three tests updated because
  they read head documents directly or expected schema 63. Strict Clippy is
  clean on the changed code; it reports findings only in the concurrent proxy
  work (`deadpan-cli/src/proxy.rs`, store `proxy_cache.rs` tests).

## Review fixes after the measured build

An independent review found the head cache and binding-proof reuse correct.
The fixes below followed the measured build and do not touch the measured
commit path except for debug-only assertions:

- The validator identity is a SHA-256 over every path crate the store reaches
  in the `Cargo.lock` graph (including media, analysis, jobs and source), the
  lockfile, workspace manifest and toolchain; an unreadable input fails the
  build. A unit test checks the reached crate list.
- Recovery checkpoints bound every stored value and replay the complete
  history of the copy instead of trusting the receipt, so a checkpoint of the
  10,000-beat package now spends about 7 s validating. Receipt hashing reads
  every value through bounded selects.
- Debug builds assert on every commit that the adopted result equals the
  stored forward patch applied to the head, and that retained validation
  equals complete validation; navigation asserts the latter.
- Initialization records the new single-original profile, so new projects
  reopen without replay; the head has its own cache slot; the binding patch's
  changed tables are read as a set.
- `project validate` replays fully by default; `--quick` checks what opening
  checks. `project migrate` and the qualification examples replay fully.
- New tests: qualified Roll, combined Trim, Slip, root sound, pause and Hold
  RoomTone audio commits with undo and redo, each followed by complete
  validation, full replay and an independent read-only rebuild; tamper cases
  that pass every structural check and are caught only by replay; a modified
  non-initial keyframe; qualification-row and profile tampering after
  certification.

## Not covered

- No real-media project: the interview package is schema 63 and is refused by
  this build; the generated fixtures stand in.
- Native-window refresh, the preview worker's own plan compile and the UI
  replays were not run.
- No full `cargo xtask gate` or ui-harness Clippy.
