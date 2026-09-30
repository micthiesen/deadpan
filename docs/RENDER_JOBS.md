# Durable render attempts

Database schema 41 stores an immutable engineering render intent and operational
attempt history. Core document schema remains 33. These library APIs support
retaining a completed encode and retrying verification after restart. Native
Render, public headless render commands, automatic encoder selection and scheduling
remain separate work. The [publication journal](RENDER_PUBLICATION.md#durable-publication-journal)
uses these checkpoints for explicit destination reconciliation.

## Captured intent and attempts

`deadpan_jobs::render::RenderIntent` binds a job ID, project, historical revision,
canonical document SHA-256, nonempty half-open range and versioned explicit
hardware/software and B-frame policy. The canonical hash is the bounded compact
`serde_json::to_writer(ProjectDocument)` representation shared by store, picture,
audio and encoding hosts. Edits, undo and redo do not retarget that intent.

`ProjectStore::create_render_job` checks the actual historical document before
committing the intent. `begin_render_attempt` allocates a fresh attempt ID,
cancellation token and monotonic ordinal. Each mutation requires the exact job,
attempt, token and transition sequence. One attempt may be active per project.
Duplicate identities, stale completions and exhausted counters fail explicitly.

The ordinary sequence is:

```text
Queued -> Encoding -> EncodedRetained -> Verifying -> Verified
```

A retry starts `Queued` with an existing checkpoint and enters `Verifying`
directly. Its checkpoint continues to name the original encoding attempt.
`Cancelling` preserves cancellation intent until the host confirms owned work
has stopped. `Cancelled` and `Failed` require completed teardown; unresolved
cleanup must remain active. Writer reopen validates the store, then records
nonterminal work as `Interrupted`. Read-only opens do not perform recovery.
No process ownership is inferred from persisted PIDs.

Reports and diagnostics are operational evidence outside authored undo/redo.
`Verified` records a past observation and cannot reconstruct a live verified
candidate. Terminal attempts, checkpoint identities and pending authored redo
survive reopen. No automatic retry or publication occurs on open.

## Retained bytes

`Media/RenderCandidates` holds BLAKE3-addressed movie and manifest objects.
`RenderObjectRef` is a distinct type; these objects are not Generated Hold media.
Each checkpoint also records the exact SHA-256 and byte length of both objects.
There is no automatic orphan eviction. Objects already published into this
namespace survive a later manifest or database failure.

Connection-free `RenderWriteHandle::prepare_retention` runs on an I/O worker.
It checks the expected movie length and SHA-256, retains both objects durably,
and returns an opaque token bound to the exact store session and attempt
transition. The SQLite writer checks the token's descriptor and namespace
freshness before the checkpoint transaction and immediately before commit.
It does not copy or hash the movie on the writer thread.

A descriptor-relative `.render-candidates.lock` serializes namespace accounting
and publication across sessions, including an old worker whose owning store has
closed. The fixed lock and directory identities are rechecked. Namespace scans
count regular owned files, including pending and unreferenced objects, and reject
unsafe entries. Existing identical objects receive deduplication credit only
after independent hash verification. New bytes and entries are reserved against
the caller's explicit limits before publication.

The hard per-object bounds are 64 GiB for a movie and 256 KiB for its manifest.
The caller also supplies combined bytes, namespace bytes and namespace entries
(at most 100,000). Logical namespace accounting does not reserve physical disk
space. Private staging and snapshots have the combined cap and consume additional
space; the host must bound concurrent snapshots. Filesystem allocation failures
are reported explicitly. Controls share one absolute deadline and are cooperative
around bounded I/O, not preemption of arbitrary reader or kernel calls.

Closing the store revokes its read/write handles and prepared tokens. A new
session must obtain new handles. `RenderReadHandle::snapshot` freshly checks both
objects into private bytes; it exposes no pathname or writable descriptor.
Encoder and verifier supervision also polls that liveness, requests cancellation
when ownership is revoked, and waits for the owned process to finish teardown.

## Host stages

`deadpan_cli::encoded_render::jobs` separates worker work from store transitions:

1. `capture_intent` opens the specified historical revision and captures its
   geometry, clocks, document hash and explicit policy. Commit that intent.
2. Commit `Encoding`, then call `encode_and_retain` with its exact identity.
   The existing isolated encoder supplies the movie. Commit the returned opaque
   retention token with `retain_render_checkpoint`.
3. Commit `Verifying`, then call `verify_checkpoint`. It freshly hashes the
   retained objects, parses the strict versioned manifest, reconstructs the
   historical contract and compares all captured fields, including the original
   encoding attempt, geometry, audio boundaries and encoder choice.
4. Run the existing isolated finished-file verifier against those private bytes.
   Recheck session liveness after clean verifier teardown. Only this live result
   supplies `VerifiedCandidate`. Record its bounded observation separately.
5. Pass the live candidate to the [publication host](RENDER_PUBLICATION.md).

No worker stage owns the writable SQLite connection. A malformed manifest,
changed movie, stale document, revoked session, cancelled attempt or verifier
failure cannot supply a new live verified candidate. Retained objects remain
available for an explicit retry. A stored report is never an input to the
recovery verifier.

## Migration and remaining work

Schemas 39 and 40 already contain core 33. Their migration validates existing history
without replaying or rewriting authored JSON or patches, then adds empty render
tables where absent. Schema 40 keeps every existing render job, attempt and
checkpoint cell and gains empty publication tables. Earlier schemas retain their
strict replay adapters and gain both operational boundaries. Migration keeps a
consistent pre-upgrade SQLite backup. Authentic
schema-39 fixtures cover a qualified single-Original baseline with pending redo,
an edited Source project and an accepted Generated project.

Destination publication has a separate durable journal and explicit reconciliation
protocol. It requires a new live verifier result; a stored Verified row alone
never authorizes adoption of destination bytes. Product controls, full
mastering/effects, HDR and release qualification
remain required; no DP requirement or delivery gate is completed here.
