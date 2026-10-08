# Durable generated-bundle acceptance

The store exposes `preview_generation_acceptance_contexts` and `accept_generation_bundle`
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

`BundleAdmissionEvidence` contains measured native/sample `SourceSpan` values
and operation-tagged `BundleInputObjects`, bound to the request's context
SHA-256. Bridge retains the manifest and two prepared frames. Extension retains
the manifest, 1–64 chronological context pictures, required continuity
signatures and an explicitly present or absent opposite picture. Checked
constructors reject cross-operation inputs and incompatible aliases. Repeated
context pictures may share an object only with the same exact length.

Host qualification derives the source spans from conversion report 2: the first
decoded output timestamp through the last timestamp plus its actual decoded
duration. These are container coordinates, separate from exact frame counts and
the original rational sampling map. For the measured 25-frame 24-fps master the
span is `[0, 1041)` milliseconds, with a 41-ms last frame. It must not be synthesized
as rounded `25/24` seconds. Envelope 3 retains both reports, spans and input receipt.

The caller supplies expected revision, new revision, attempt identity, exact
expected receipt and fresh asset IDs. It cannot supply asset metadata or a new
sampling map. Before opening the SQLite transaction, acceptance rehashes every retained
objects through verified snapshots. Inside the transaction it checks:

- The authored head, selected attempt and complete receipt still match.
- The request is current and the selected attempt is Ready and Present.
- The Hold duration, project rate and context match the persisted operation plan.
- Worker protocol, declaration, controls and input capture agree on Bridge or
  Extension and its direction; no parser fallback changes that operation.
- The request's current authoring address resolves every explicit Repeat
  Default or stable Play choice and the Hold's full intrinsic duration.
- Complete before/after context reconciliation preserves the accepted request,
  or independently derives the exact fallback required when this acceptance
  changes neighboring inputs.

Asset records come from the receipt's observed spans, exact frame counts and
content identities. One command registers the assets and captures the fallback
while changing the Hold's provider. A Play-scoped acceptance isolates the
selected branch atomically; a Default acceptance retains existing overrides.
Revision, forward/inverse history, scope address mappings, cursor and request
relevance commit together. A failed transaction leaves authored state unchanged;
previously published files remain available.

The same transaction records an immutable accepted-origin receipt and derives
the final providers of dependent Holds. A newly accepted candidate can itself
return to its captured fallback when the final boundary calculation requires
it; fresh replacement intent is retained. Native comparison and acceptance
feedback describe that actual final provider. Preview uses the same complete
prospective document and performs no writes.

Preview performs the same resolution and object checks without writes, including
on a read-only store. The host uses its resulting edit to resolve context before
commit, which repeats all checks. Ordinary commands and initial snapshots still
cannot introduce new generated artifacts. Undo removes newly registered assets;
redo restores them under a fresh revision. Reversion restores the captured
Background/Freeze provider. None of these operations deletes media bytes or
revives a stale or detached request.

## Authoring scope and presentation

Worker bindings retain the original Hold ID, origin revision and immutable
`ScopedNodeTarget`. Each persisted generation scope has an independent monotonic
request clock and a current address. That address follows isolation maps
rederived from the exact validated history command, including an edit to another
node that clones the Hold. Undo and Redo reverse or reapply those mappings
without reviving stale requests, attempts or discarded media. Reopen validates
the retained chronological proof; forged mappings and sibling retargeting fail.

Repeat and Retime ancestors do not change the generated picture count. Context
5 describes exact boundary samples in the local definition clock and host
schema 8 validates their origin and duration. A dormant Default can Generate
and Accept when both local boundaries exist. Preview and audition additionally
require a concrete visible occurrence and its complete root-frame window.
Changing the scoped selection revokes pending and cached previews. Several
selected plays currently refuse explicitly. See [scoped AI](AI_HOLDS.md#repeat-and-retime-scopes).

The store's preview APIs return the prospective edit and mapped current request
addresses together. Hosts reconcile those addresses against each request's
immutable origin, preparing one render plan for the exact borrowed prospective
document. The plan cannot leak across failed transactions that reuse a revision
ID. Ordinary store writes use the same resolver preparation contract.

Store schema 74 deliberately refuses earlier unused development packages under
the session's breaking-format authorization. Historical generated provenance
remains readable under its declared contract; it gains no invented evidence.

## Verification and limits

Store tests cover stale selection/revision/receipt/context, missing and corrupt
dependencies, aliases, rollback, read-only preview, history branches, scoped
isolation, dormant Defaults, mapped request addresses and unsupported old
development formats. Native integration
tests exercise actual media qualification, acceptance and complete dependency
readback after removing worker files and relocating the package. The
[Extension integration](qualification/extension-saved-2026-10-08.md) also covers
both directions, one-frame output at definition edges, missing signatures or
PNGs, exact sampled pixels and model-free undo/redo and reopening.

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
