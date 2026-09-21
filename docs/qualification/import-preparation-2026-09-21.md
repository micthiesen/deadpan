# Background import preparation qualification, 2026-09-21

This change separates original-file preparation from inventory and source
registration commits. A connection-free handle can move to an import worker;
the project service keeps its writable store and can continue applying typed
edits. [Import preparation](../IMPORT_PREPARATION.md) describes the APIs and
their limits. Native queue and project controls are subsequent integration work.

The run used Apple M5 Max, 128 GiB, macOS 26.5.2, Rust 1.97.1 and pinned LGPL
FFmpeg 8.0.3 at `/tmp/deadpan-media-compatible-xyhilms4/prefix`. The
[source manifest](../../tools/media-qualification/evidence/2026-09-21-import-preparation/source-manifest.json)
records source and fixture hashes against presentation-basis revision
`077c6791dfd6feacf62e6a3951458a2f082ce76d`, which passed
[CI run 35649258406](https://github.com/micthiesen/deadpan/actions/runs/35649258406).
Core schema 10 and database schema 15 are unchanged.

## Original retention and lifetime

Nine integration tests exercise worker preparation, inventory admission,
ownership merging, cancellation, session boundaries, changed files, unsafe
namespaces and rollback. Preparation completes while another SQLite connection
holds an immediate transaction. The writer then commits a Hold before admitting
the prepared original. Preparation itself adds neither inventory nor authored
history.

Managed and linked originals are checked against retained file identity and
metadata at admission. Deletion, equal-byte replacement, in-place modification,
writable managed objects, hard links and symlink substitutions are rejected.
Another project, a read-only store or a reopened writer cannot consume a prepared
token. Closing the writer does not keep its lock alive through a worker handle.
An already returned private snapshot remains readable after the source changes
and the store closes; it cannot authorize a new edit in a later session.

An injected inventory-write failure leaves the published original intact and no
inventory row. Removing the trigger permits retry with the same token. A
deterministic object-engine test raises closure or cancellation immediately before
publication: file and namespace durability complete, then verification stops and
the published bytes remain. An injected durability failure remains the reported
error even when cancellation is pending. Namespace replacement is rejected.
All promotion verification passes now observe the original operation's deadline
and cancellation controls. A closed session reports `OriginalImportClosed`,
including when caller cancellation is also set.

A delayed linked-retention token cannot overwrite a newer relink. Retention now
adds ownership or a missing link; replacing a stored location or bookmark uses
the explicit versioned relink operation. Its regression test failed against the
pre-fix implementation and passes with the same source bytes still available at
both paths.

## Source registration and current intent

Twelve prepared-registration tests use actual video and audio decoder sessions.
They cover concurrent typed edits, stale revision rejection and token reuse,
read-only preview behavior, session mismatch, changed originals, duplicate
receipts, corrupted receipt bytes and insertion timing.

Relinking equal bytes changes the inventory version without changing authored
history. A token from the old location is rejected even while its file remains
unchanged and available; preparing against the new record permits insertion.
Cancelling a prepared insertion creates no receipt or edit and allows a later
retry with the same token.

A CFR source prepared in an automatic project is inserted after an explicit
640×360 canvas edit. The insertion preserves that canvas and the locked 30 fps
rate, enclosing the exact source duration in 121 frames. The same source in an
untouched automatic project selects 30000/1001 and 120 frames. Preparation does
not freeze the project clock or insertion target.

The writer rechecks original availability before committing. It compares an
existing receipt with prepared canonical bytes inside SQLite instead of parsing
and hashing the index again. Undo, redo and complete store validation remain on
the existing persistence path. A compile-fail doctest rejects construction of a
prepared registration through JSON deserialization.

The source rollback test injects a failure at history insertion, after the
receipt write, and checks exact document restoration plus unchanged receipt,
revision and history counts. Removing the trigger permits insertion with the
same token and revision intent, followed by undo, redo and store validation.

Another test allocates a current generation request and supplies a typed
`RelevancePlan` resolved against the prepared edit. Without that plan, insertion
fails atomically. A cursor-update trigger confirms tentative receipt, revision,
history and relevance writes before deliberately failing. The failure restores
all four plus the cursor; the same token and plan then commit and survive reopen.
The test uses supplied context hashes and qualifies store reconciliation, not a
source-aware host context resolver.

## Verification scope

The final repository gate passed formatting, workspace Clippy with warnings
denied, **598 Rust tests**, zero failures or ignored tests, workspace build and
doctor. The [gate report](../../tools/media-qualification/evidence/2026-09-21-import-preparation/gate/report.json)
records commands and durations; compressed logs preserve the actual output. The
earlier 591-test gate is retained under `pre-review-gate`, before strengthening
the history failure test and adding linked-version and cancellation checks.
The 593-test run before review fixes is retained under `pre-fix-gate`.

Three independent reviews covered general correctness, freshness/session lifetime
and atomic admission. Their [dispositions](../../tools/media-qualification/evidence/2026-09-21-import-preparation/review.json)
record four applied findings: delayed linked retention could overwrite a newer
relink, cancellation could skip post-publication durability, closure could be
reported as user cancellation, and prepared registration lacked generation
relevance rollback coverage. The first three now have regression evidence,
including pre-fix failures; the fourth adds the cursor-trigger test described
above. Both focused reviewers checked the fixes and reported no remaining
findings. Nothing was dismissed or deferred.

No native adapter, application startup or control changed. Native
ASan/UBSan, lifecycle, GUI aesthetics, IME, accessibility and keyboard navigation
were not repeated for this storage/API change. Existing
[source-preview evidence](source-preview-2026-09-21.md) retains its limited visual
and keyboard scope. Linux execution and the separate Python media qualification
suites were not run locally.

An initial test compile typo and a Clippy warning about making a test fixture
world-writable were corrected. The fixture now enables only owner read/write
permissions for its deliberate in-place modification test.

## Remaining work

The native single-writer project service, bounded import queue, source browser,
explicit insertion controls, retry/relink workflow and basis previews remain
open. Complete-file copying and hashing have moved out of writer commits, but
SQLite writes, document validation and source timing analysis still have costs.
These small fixtures establish behavior, not full-size import latency, memory
budgets or queue scheduling. Full product requirements and delivery gates remain
open or partial.
