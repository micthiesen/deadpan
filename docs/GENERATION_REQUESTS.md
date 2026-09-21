# Persistent generation requests

[`deadpan-store`](../crates/deadpan-store/src/generation.rs) persists immutable
generation request inputs, per-Hold request clocks, and current/stale/detached
relevance. This implements the storage boundary for specification Sections 12.6
and 12.7. [Attempt storage](GENERATION_ATTEMPTS.md) adds durable worker state and
restart recovery. Neither module resolves source context, accepts candidates, or
promotes media into a project. The development MLX worker remains separate from
the native application.

## Authored history and operational state

Core documents remain schema 4. Database schema 5 introduced `hold_request_clocks` and
`generation_requests`. Requests retain their origin revision, project and Hold
identities, context SHA-256, typed video/conditioning/motion constraints, provider
pins, seed, and request version. The origin revision is provenance; an unrelated
document revision does not make a request stale.

`allocate_generation_request` takes an expected current revision and a fresh
caller-supplied request ID. It checks that the target is an existing Hold with the
requested frame duration and project rate. In one immediate SQLite transaction,
it advances that Hold's retained clock, marks a prior current request stale, and
inserts the new current request. Explicit regeneration advances the version even
when the context is identical. Versions use the positive signed SQLite integer
range and fail on exhaustion.

Allocation does not create an authored revision. It never changes the visible
picture provider, audio policy, duration, or undo cursor. Clocks survive deletion
and restoration of a Hold, including reuse of the same node ID. Request versions
and relevance are absent from core inverse patches, so history cannot restore
their earlier values.

## Atomic relevance reconciliation

The host previews the next authored document and resolves each current request's
generation dependencies against it. A `RelevancePlan` identifies the exact source
and destination revisions and covers every current request once, with its full
existing binding and either a resolved context hash or an unresolved result.

`commit_reconciled`, `undo_reconciled`, and `redo_reconciled` verify the plan inside
the same immediate transaction as the document revision, history, and cursor.
Coverage is checked against current stored requests, so allocating or replacing a
request after preview invalidates the older plan even if the document revision
did not change. Incorrect identities, duplicate or omitted observations, and
incorrect revision pairs fail atomically.

The store independently checks the next document:

- A missing target becomes detached.
- A target that is no longer a Hold, incompatible duration/rate, an unresolved
  context, or a changed context hash makes the request stale.
- A compatible Hold with the same resolved hash remains current.

Only current requests participate in reconciliation. Undoing an edit that made a
request stale does not revive it. A new request requires a new version. Existing
`commit`, `undo`, and `redo` calls reject current requests with
`GenerationRelevanceRequired`; they cannot bypass reconciliation. Preview calls
perform no allocation or relevance writes.

The host owns truthful media/context resolution. Its context must include source
content hashes, exact boundary frames/coordinates, requested duration, generation
constraints, and relevant preprocessing/planner/template versions. It excludes
global document revision, editorial gain/zoom/captions, group position, and the
current fallback/accepted provider. The store cannot establish those media facts
from a supplied hash. Worker cancellation follows a committed invalidation;
waiting for a GPU kernel is not part of the document transaction.

## Migration and verification

Schemas 1 through 3 replay and compare their complete chronology through the
existing strict legacy adapters before adding empty operational tables. Schema 4
validates the complete authored chronology without rewriting its JSON. Schema 5
also validates and retains all request rows and clocks before adding the schema-6
attempt tables. All paths retain a `Snapshots/before-schema-6-*.sqlite` backup and promote only the validated
candidate through SQLite's backup transaction.

The [schema-4 fixture](../crates/deadpan-store/tests/fixtures/v4-history.sql) was
emitted by the preserved executable from commit
`8b79526ca3ff9bca633aa9e9d795bcc9a0c0c2d1`. Its 11 revisions and six edits include
sparse override ownership, mark loss/restoration, an abandoned branch, and pending
redo. The [migration tests](../crates/deadpan-store/tests/migration.rs) compare the
original authored JSON and history byte-for-byte, exercise subsequent undo/redo,
and reject corrupt histories and operational table collisions before promotion.

The [request tests](../crates/deadpan-store/tests/generation.rs) cover clocks,
relevance, complete-plan validation, history navigation, reopen, and transaction
rollback. Integrity checks do not rely solely on database uniqueness indexes:
duplicate request IDs, Hold/version pairs, current Holds, and clocks are rejected
even when those keys are missing. A current request must use the latest allocated
version, and each immutable request must match the Hold in its origin revision.

Independent review covered history/relevance behavior and migration separately.
It found a missing duplicate-request-ID check under a malformed schema; the
explicit check and regression fixture address it. Candidate acceptance and
durable media ownership remain separate required work. No GUI behavior changed
in this slice, so native startup and interactive checks were not repeated.

The schema-5 request implementation passed formatting, workspace Clippy with warnings denied,
224 Rust tests (zero failed or ignored), workspace build, and `deadpan-cli doctor`.
The audio and model qualification suites also pass 20 and 47 Python tests.
The current diagnostics distinguish database schema 6 from core document schema 4
and continue to report application AI generation as unimplemented.
