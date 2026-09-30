# Durable render checkpoint qualification, 2026-09-29

The [render job boundary](../RENDER_JOBS.md) retains completed encodes across
project reopen and requires fresh verification before publication. Two real
project cases passed encoding, durable retention, writer restart, explicit retry,
isolated verification and publication. Independent readers then checked their
final published files. This does not qualify a complete product Render workflow.

## Native evidence

| Case | Captured revision and range | Pictures | Authored sample frames | Complete GOPs |
| --- | --- | ---: | ---: | ---: |
| Structural Original edit | `background`, `[20,128)` | 108 | 172,973 | 7 |
| Accepted Generated Hold | `ui-generated-ready`, `[0,30)` | 30 | 48,048 | 2 |
| Total | | 138 | 221,021 | 9 |

Each job encoded once. The writer checkpointed its complete movie and strict
manifest, then closed. Read-only reopen left the attempt unchanged; writer
reopen recorded `Interrupted`. A fresh attempt and cancellation token selected
the original checkpoint, freshly hashed both objects and ran the production
verifier. Persisting `Verified` did not replace that live verification step.
The live candidate then passed destination publication and byte readback.

During structural encoding, a framing edit, undo, redo and final undo ran while
the child continued its captured revision. The final undo restored authored
content under a fresh revision ID. Exact SQLite authoring-cell hashes remained
unchanged throughout the subsequent checkpoint, recovery, verification and
publication operations. The captured historical document remained unchanged.

The native run took 77.694 seconds. Independent FFmpeg and AVFoundation readers
took 6.698 seconds and checked:

- All 414 picture planes, totaling 102,643,200 code values.
- Complete canonical PCM comparisons at fixed observed sample coordinates.
- Actual packet/container clocks, allowed encoder-delay edit lists and complete
  fresh-decoder GOP comparisons.
- Actual final movie filenames, rather than an earlier staging file.

Direct I420/PCM references came from the preceding publication qualification.
The new harness independently checked their retained SHA-256 identities, exact
historical document hashes and complete output contracts before reuse. It did
not regenerate those references. This corpus adds no new synthetic audio marker
measurement and performs no event-based alignment or gain adjustment.

A separate Python/SQLite audit verified coherent database backups, historical
document hashes, the two persisted attempt records per job, exact checkpoint and
verification-report agreement, and actual retained/published file hashes, sizes
and modes. Retained objects were `0444`; final movies and reports were `0600`.

## Persistence and failure coverage

Authentic schema-39 fixtures were retained before the schema change. They cover
a qualified single-Original baseline with pending redo, a structural Source
project, and an accepted Generated project. Schema 40 adds empty render tables
without changing any cell in the preexisting tables, including authored JSON,
patches, operational generation records and source receipts. Migration preserves
its schema-39 backup. The fixtures were not made by relabeling a current database.

Six focused host integration tests passed restart/reverification, stale
completion rejection, malformed and retargeted manifests, changed retained
movie bytes, and owner closure during verification. Deterministic live children
also proved that owner closure sends cancellation and completes supervision for
both encoding and verification without a user cancellation flag.

Independent review found and corrected three issues:

1. Routine operations reparsed all historical attempt bodies. Full audits now
   run on open/explicit validation; routine operations read bounded selected
   rows and scalar metadata under fixed capacity limits.
2. Open validation repeatedly hashed shared revisions under an aggregate
   30-second cutoff. It now hashes each distinct revision once and does not make
   validity depend on that cutoff.
3. Owner closure could leave a child running until its deadline. Both supervisor
   loops now poll ownership and request cancellation when it is revoked.

The general, recovery and migration reviewers reported no remaining findings.
The evidence retains their reports, applied-fix record and actual check logs.
The workspace has 2,284 passing tests and no remaining failed or ignored tests
across 165 targets, including doctests. The full invocation completed with three
failures. Passing targets were preserved; complete CLI command, render-store and
playback targets passed after the narrow corrections described below. The original
failed invocation is retained and is not reported as green. Final all-target
workspace Clippy and formatting passed.

## Environment and source binding

Apple M5 Max, 128 GiB RAM; macOS 26.5.2 (25F84), SDK 26.5, Apple Clang 21,
Rust 1.97.1 and the pinned LGPL FFmpeg 8.0.3 prefix. The report records loaded
libraries, headers, binaries, compiler invocation and source hashes. Hardware
encoding used explicit no-B-frame policy for both cases.

Native execution and the six host tests used source inventory
`694db72c44d8a15f687d2f317a4926e7af413187e1f52d7b78bde5a39667a475`.
Final source inventory is `306a884f2d4618700019f240e01fc007fb8d6c3127954b38f7e341e584959482`.
Later changes name an existing SQLite row tuple type for Clippy, update doctor's
migration capability string and schema assertion, allow intentional corruption of
a sealed test file, and make a playback test wait for the expected delivered-sample
update instead of an older Playing update. Production playback, native media and
render operations are unchanged. The retained exact patch and per-command source
inventories bind these results without repeating unchanged media work.

Initial failed runs are retained: an unsupported digest formatting expression in
a test; synthetic legacy fixtures retaining new operational tables; a corruption
test blocked by immutable file mode; and evidence helpers mishandling a SQLite
BLOB or a non-file admission entry. An initial native run rejected a copied
Original with writable permissions. The fresh qualification copies restored the
owned media objects' `0444` mode before admission. Failed runs are excluded from the
passing qualification totals. The full workspace also exposed a stale doctor
schema assertion, another corruption fixture blocked by 0444 mode, and the playback
fixture's stale-update race. Their corrected targets passed in full. The playback
fix retains its original bounded deadline and exact sample assertions; an independent
source review found no production clock defect.

## Limits

Restart was an orderly writer close, not injected process death, power loss or a
filesystem fault. The named namespace budget includes orphans and pending files;
private staging has separate per-operation bounds. It does not reserve physical
disk space or provide a process-wide scratch quota. Full historical audits still
scale with stored history and run off the editing path.

Durable publication intent/reconciliation, automatic platform policy, bounded
scheduling, native Render and public headless commands remain open. Complete
mastering/effects, HDR, a larger content/runtime corpus and release qualification
remain required. Native C was unchanged, so this increment did not rerun its
preceding sanitizer qualification. No UI changed or live GUI check was run.
Every DP requirement and Gate A through G remains open or partial.

Evidence: [retained artifacts and reports](../../tools/media-qualification/evidence/2026-09-29-render-jobs/README.md).
