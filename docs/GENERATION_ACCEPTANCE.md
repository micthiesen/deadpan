# Durable generated-bundle acceptance

The store exposes `preview_generation_acceptance` and `accept_generation_bundle`
for an explicit host acceptance of the exact selected Ready bundle.
[`generation::acceptance::accept`](AI_HOLDS.md) uses them with a relevance plan
from the installed boundary resolver; `deadpan-cli accept-hold` (directly or
through an open app's live endpoint) and the native `:accept-ai` expose it.
It does not turn background completion, qualification, or selection into an edit.

A request's attempts are seeded variants: attempt `n` runs with
`ProviderSelection::for_attempt(n)`, and the bundle receipt and worker
declaration must carry exactly that provider (attempt 1 keeps the request's own
seed, so existing receipts are unchanged). Hosts choose a variant with
`select_generation_bundle_variant`; acceptance admits only the selected one.
The native Preview admits the acceptance's document for audition with
`deadpan_playback::Snapshot::proposed_generated`, which permits only the two
added video-only generated assets and keeps every committed asset and its
qualified sources, so the proposed pause plays its unchanged audio. A durable
Discard is the existing availability change (`mark_generation_bundle_evicted`);
it clears a matching selection and is not reversible.

## Evidence and transaction

Schema 9 adds optional `BundleAdmissionEvidence` to modern bundle receipts. It
contains measured native/sample `SourceSpan` values and `BundleInputObjects` for
the retained context manifest and two prepared frames, bound to the request's
context SHA-256. Checked constructors and deserialization reject incompatible
aliases. Legacy receipt JSON stays unchanged and gains no invented evidence.

Host qualification derives the source spans from conversion report 2: the first
decoded output timestamp through the last timestamp plus its actual decoded
duration. These are container coordinates, separate from exact frame counts and
the original rational sampling map. For the measured 25-frame 24-fps master the
span is `[0, 1041)` milliseconds, with a 41-ms last frame. It must not be synthesized
as rounded `25/24` seconds. Envelope 3 retains both reports, spans and input receipt.

The caller supplies expected revision, new revision, attempt identity, exact
expected receipt and fresh asset IDs. It cannot supply asset metadata or a new
sampling map. Before opening the SQLite transaction, acceptance rehashes all six
objects through verified snapshots. Inside the transaction it checks:

- The authored head, selected attempt and complete receipt still match.
- The request is current and the selected attempt is Ready and Present.
- The Hold duration, project rate and context match the persisted bridge plan.
- The Hold identifies one effective occurrence, without expanding Repeat plays.
- Complete before/after context reconciliation preserves the accepted request.

Asset records come from the receipt's observed spans, exact frame counts and
content identities. One command registers the assets and captures the fallback
while changing the Hold's provider. Revision, forward/inverse history, cursor and
request relevance commit together. A failed transaction leaves authored state
unchanged; previously published files remain available.

Preview performs the same resolution and object checks without writes, including
on a read-only store. The host uses its resulting edit to resolve context before
commit, which repeats all checks. Ordinary commands and initial snapshots still
cannot introduce new generated artifacts. Undo removes newly registered assets;
redo restores them under a fresh revision. Reversion restores the captured
Background/Freeze provider. None of these operations deletes media bytes or
revives a stale or detached request.

## Occurrences and migration

Request bindings currently name a structural Hold ID. A multiply repeated or
fully overridden default Hold is rejected. Isolate a concrete occurrence first;
an override owned by one effective play is supported. Retime ancestors are
conservatively rejected until crop visibility and occurrence context are resolved.
Legacy operational records remain inspectable and migratable under their original
contract; migration does not retroactively invalidate ambiguous old bindings.

Schemas 1 through 8 migrate through a consistent copy and retained backup. Schemas
7 and 8 already use core schema 5, so their histories are validated without JSON
rewriting. Schema-8 receipts retain their original bytes and have no admission
evidence. New admission fields, even null values, are rejected in old schemas.

## Verification and limits

Store tests cover stale selection/revision/receipt/context, missing and corrupt
dependencies, aliases, rollback, read-only preview, history branches, occurrence
isolation, and genuine schema-8 migration with pending redo. Native integration
tests exercise actual media qualification, acceptance and six-object readback
after removing worker files and relocating the package.

`deadpan-models/examples/qualify_generated_acceptance.rs` runs the same path against
a captured real model result in a fresh synthetic Hold project, then relocates,
reopens, undoes, redoes, reverts and reads back all dependencies. Its context
reconciler is fixture-specific. It is not an app context resolver or a user audition.

Prepared frames remain opaque retained bytes to the store. The offline test
re-derives them from the request's origin revision on a portable copy and
reproduces the bound manifest exactly ([source-clock evidence](STORAGE.md#source-clock-and-colour-evidence)).
Measured source colour metadata in the manifest, cross-build determinism,
installed-model attestation, useful motion and seam quality remain required.
The store trusts the host's qualification receipt; it does not independently
decode media or parse provenance. [Storage](STORAGE.md) adds the reference
inventory, cleanup of discarded and stale variants and portable copies.
[Source joins and speech preservation](GENERATED_HOLDS.md#source-joins) are
hard cuts at exact frames with unchanged audio, checked on real media with the
synthetic worker; an advisory endpoint-discontinuity measurement is reported per
Ready variant, and real-model seam quality remains unmeasured. The native app's scheduling, variant choice, audition and
interactive acceptance are described in [AI Holds](AI_HOLDS.md#native-app-workflow). The
headless chain, its measured real run and the symmetric conditioning and
presentation raster policy are in [AI Holds](AI_HOLDS.md).

[The measured acceptance run](qualification/acceptance-2026-09-21.md) records the
actual media/history probe, independent decode, repository gate and sanitizer scope.
