# Persistent generation attempts

[`generation_attempts.rs`](../crates/deadpan-store/src/generation_attempts.rs)
persists the operational state of a worker attempt, host validation receipts,
and candidate selection. It builds on [immutable generation requests and
relevance](GENERATION_REQUESTS.md). A ready candidate never changes the authored
document. [Modern bundle qualification](GENERATION_BUNDLES.md) adds verified
three-object promotion and Ready persistence. Explicit acceptance and application
scheduling remain required work.

## Identity and transitions

Database schema 6 introduced `generation_attempts`, `generation_attempt_heads`, and
`generation_candidate_receipts`; schema 7 preserved them while upgrading authored
documents to core schema 5. Schema 8 adds optional immutable request plans and
separate `generation_bundle_receipts`. Requests without a plan remain strict
protocol 1; modern bridge requests and receipts use protocol 2. Attempts use the
full request/attempt identity, a cancellation token, a monotonically increasing
ordinal within the request, and a transition sequence. A retry allocates a new
attempt while retaining the request's exact constraints, provider, seed, and
context. Regeneration with different inputs allocates a new request instead.
Only a current request can start an attempt, and only one attempt per request
can be nonterminal. These are storage rules; the global one-generation-at-a-time
scheduler is still unimplemented.

The pure lifecycle exports validated checkpoints for queued, preflight, loading,
running, validating, ready, failed, cancelling, and cancelled states. Relevance
is read from the owning request rather than copied into the checkpoint. Worker
completion records an untrusted declaration and enters validation. Host failure,
cancellation, and readiness use separate typed calls. Wrong identities, tokens,
state combinations, or candidate declarations fail without partial writes.

Progress remains bounded live state. The persistence entrypoint explicitly
rejects progress messages rather than losing their monotonicity checks through
snapshot restoration. Exact duplicate current-stage or retained terminal
declarations can be recognized; the snapshot is not an event journal that
recognizes arbitrary past messages. A worker cancellation acknowledgement, or a
completion discarded during cancellation, leaves the attempt cancelling. Only
the host's confirmation after process teardown makes it cancelled.

## Recovery and variants

A writable open acquires the project writer lock and validates the database
before marking all abandoned nonterminal attempts failed with `Interrupted`.
Recovery is atomic. Read-only opens leave those attempts untouched. No stored
PID is used to find, resume, or kill a process, so opening a copied project cannot
claim a process belonging to another project or machine. Existing terminal
attempts and ready candidates survive recovery.

Starting a retry retains an earlier selected ready candidate. When the latest
attempt becomes ready, its receipt and selection commit together. Earlier ready
variants remain available for explicit selection. Stale or detached requests
cannot yield a selected candidate for use, and late messages from another
attempt cannot select it. Eviction marks availability separately, clears a
matching selection, and retains the attempt's terminal record. This metadata
operation does not delete a file or implement cache retention policy.

## Host validation boundary

A legacy receipt records a typed host-managed candidate reference, SHA-256, byte length,
exact frame count/rate/dimensions, provider selection, and validator identity and
version. Its metadata must agree with the immutable request and worker
declaration. The reference is relative to a host-selected candidate cache and
grants no filesystem authority.

Constructing a legacy receipt does not examine any bytes. An integrated host must
first contain, independently hash and decode, validate, and durably stage the
candidate. It must check the actual file again before accepting it. Selection
returns metadata only and is not proof that the file still exists or is safe to
render. Accepted masters need separate immutable ownership and history pinning;
an eviction API for candidate metadata must never govern those masters.

Modern bundle receipts retain immutable native, sampled and provenance BLAKE3
objects, the exact original bridge plan, both video contracts, original worker
declarations and validator identity. `record_generation_bundle_ready` verifies
all three actual objects before beginning its atomic metadata transaction.
Cross-kind completion, receipt and selection are rejected. The modern path does
not qualify legacy metadata retroactively. See [bundle admission](GENERATION_BUNDLES.md)
for the independent host decode/provenance boundary and its remaining limitations.

## Migration evidence

The [schema-5 fixture](../crates/deadpan-store/tests/fixtures/v5-generation.sql)
was generated from commit `2cb80633b9ed9458ccd0f36ffbf355187dc9bb49` in an isolated
checkout. It contains current, stale, and detached requests, retained request
clocks, and pending redo. Current migration replays its authored history into core
schema 5, preserves requests and clocks, and adds empty attempt tables. The
[schema-6 fixture](../crates/deadpan-store/tests/fixtures/v6-attempts.sql) verifies
unchanged attempts, receipts and selection across migration to database schema 9.
The [schema-7 fixture](../crates/deadpan-store/tests/fixtures/v7-selected.sql),
generated by commit `63531f78799d5329f5c9fa7f246a9144f9dc156f`, preserves current-core
history, pending redo and a selected legacy candidate. Its migrated plan remains
absent and its bundle receipt table is empty.
Schema-1 through schema-4 fixtures still exercise the complete historical paths.
The schema-8 fixture retains a selected bundle without inventing admission evidence.
Every migration retains a `Snapshots/before-schema-9-*.sqlite` backup and promotes
through SQLite's backup transaction only after validation. Corrupt request rows
and colliding new tables leave the source untouched.

## Verification

On 2026-09-21, the repository gate passed on Apple M5 Max, arm64, 128 GiB RAM,
macOS 26.5.2 (25F84), Rust 1.97.1: formatting, workspace Clippy with warnings
denied, 236 Rust tests with none failed or ignored, workspace build, and headless
diagnostics. The audio and model qualification suites passed 20 and 47 Python
tests. At that checkpoint, diagnostics reported database schema 6 and core schema 4,
and continued to
mark application inference, media playback, and export unimplemented.

The [attempt tests](../crates/deadpan-store/tests/generation_attempts.rs) cover
retained variants with identical media hashes, explicit selection and eviction,
duplicate/conflicting events, wrong identities and tokens, receipt mismatch,
transaction rollback, all nonterminal recovery states, read-only inspection,
late stale completion, missing indexes/foreign keys, invalid persisted values,
counter exhaustion, and copied-project isolation. The
[lifecycle tests](../crates/deadpan-jobs/tests/lifecycle.rs) verify cancellation
acknowledgements, host teardown, and valid checkpoint combinations. The 17
migration and 12 CLI tests exercise the integrated schema change and headless
history behavior.

Review tightened cancellation/reap ordering, rejection of conflicting terminal
responses, checkpoint stage validation, orphan receipt checks, query snapshot
consistency, and explicit invariant checks when database indexes are absent.
Invalid optional enum values cannot silently become absent values during reads.
Native GUI startup and interactive checks were not repeated because this slice
does not change app startup, views, focus, or keyboard behavior. These tests do
not qualify model quality, managed-media promotion, or application responsiveness.
