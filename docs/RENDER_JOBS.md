# Durable render attempts

Database schema 42 stores immutable engineering or automatic render intents,
per-encoding decisions and operational attempt history. Core document schema
remains 33. These library APIs support
retaining a completed encode and retrying verification after restart. A shared
workflow coordinator connects these stages to the native project service.
Native controls and headless commands use this coordinator. Commands for an
open native project route through its [authenticated owner](LIVE_PROJECT.md).
Native persisted-job recovery and scheduling remain open.
The [publication journal](RENDER_PUBLICATION.md#durable-publication-journal)
uses these checkpoints for explicit destination reconciliation.

## Captured intent and attempts

`deadpan_jobs::render::RenderIntent` binds a job ID, project, historical revision,
canonical document SHA-256, nonempty half-open range and versioned policy.
Intent version 1 retains the original explicit engineering choice. Version 2
selects `AutomaticSdrV1`; the user supplies no encoder or B-frame choice.
The canonical hash is the bounded compact
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
   geometry, clocks, document hash and policy. Commit that intent.
2. An automatic attempt qualifies while `Queued`, then atomically records its
   decision and enters `Encoding`. The stage worker consumes the retained live
   admission only after that transaction succeeds. An engineering attempt
   commits `Encoding`, then calls `encode_and_retain` with its exact identity.
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

## Shared workflow and native ownership

`deadpan_cli::encoded_render::workflow::RenderWorkflow` owns one bounded stage
worker, one command slot, one reliable completion slot and coalesced progress.
The project service owns the SQLite connection. Its nonblocking `poll` admits
at most one completion and advances the corresponding journal transactions.
Capture, media preparation, encoding, hashing, verification, destination copies
and release of heavy values run on the stage worker. Intent admission still
validates the bounded complete authored document on the writer; this is not a
constant-time operation.

Start captures an exact current revision and immutable operation identities.
Later edits do not retarget the render. Retry can reuse a retained checkpoint
but always creates a new verification attempt. Reconciliation retains its
recovered destination locks until the final journal transaction. Optional
progress belongs to the exact workflow identity and active stage; it cannot
substitute for a reliable completion.

A session-bound lease prevents overlapping workflows from preflight through
publication and final release, including across coordinator instances. Equal
project IDs cannot transfer writer authority between separate writable opens.
Dropping an unreleased lease fences that writer session. Reopening initializes
new admission state and interrupts persisted attempts; it does not prove an
unknown old process has stopped.

Cancellation persists the request and revokes publication permits before
signaling worker work. A failed journal write still requests a stop and retains
an unresolved status. A racing movie commit remains observed even if the final
database write fails. `can_release_writer` requires returned stage cleanup and
release of worker-owned candidates and locks. Journal recovery can remain
necessary after execution cleanup completes. A worker panic, lost channel or
unconfirmed process cleanup cannot supply that release evidence.

Encoder and verifier hosts explicitly finish owned subprocess work on every
post-spawn return, including partial setup failures. The stopped receipt records
group membership scope, leader reaping and joined I/O pumps. A failed group
check stays unresolved after a successful leader fallback. A joined pump panic
is a terminal failure, never successful media. Darwin's membership evidence is
required for these hosts; Linux's signal-and-leader observation is insufficient.
Destructors provide fallback cleanup and cannot authorize a terminal attempt.

The native `ProjectService` routes typed Start, Retry, Reconcile and Cancel
requests through this coordinator. It retains render status independently of
editor command feedback and continues accepting ordinary edits. Close, switch
and shutdown keep the old writer until worker release; unresolved cleanup
retains the session and exposes its diagnostic. The runtime is the current
native executable's direct private worker dispatch. The same executable still
accepts ordinary public `--headless` commands. Native and public headless Render
use the entrypoints below.

## Native and public Render

The native **Render · ⌘E** control and `:render` capture the current project
session and committed revision. A Camera, Gain or Room tone preview first offers
Commit preview and render, Discard preview and render, or Keep editing. Native
field input is processed before capturing the proposal, including a Render click
in the same input batch. Cancelling the destination picker preserves the draft.
The native save sheet suggests a fresh MP4 name under the neighboring `Exports`
directory and remembers the selected directory for this project session.

`CommitAndStart` accepts only the three existing preview edit types. It validates
the exact session, revision, cursor and scope before editing. Only the durable
commit receipt supplies the render revision; an idle writer or refreshed view
cannot establish a commit. The service retains this receipt even when workspace
refresh or render admission fails, so the UI truthfully reports an already saved
preview. Unchanged previews use the existing committed revision. Render progress,
cancel, errors and final movie/report paths remain separate from editor feedback.
Later editing or Undo cannot change the captured render.

The shared public adapter derives the full range and `AutomaticSdrV1` policy,
allocates fresh operation identities and chooses the current executable's private
worker dispatch. [Headless Render](HEADLESS.md#automatic-render) exposes start,
bounded stored status, checkpoint retry, fresh encoding and reconciliation. A
closed-project invocation owns its writer through cleanup. SIGINT/SIGTERM and
output backpressure request cancellation; the owner continues draining worker
replies. Terminal/recovery events make a bounded stderr fallback attempt if
stdout fails. Neither output failure nor unconfirmed cleanup authorizes release.

When the native app owns the writer, the CLI captures that owner's authenticated
endpoint and observes the exact admitted workflow. Remote admission refuses an
unresolved temporary preview. A separate cancel command must name the full
workflow target and cannot discard the initiating client's retained result.
Owner loss cannot trigger local fallback or replay. See
[open-project commands](LIVE_PROJECT.md) for transport and observation limits.

The native UI currently exposes new rendering and live cancellation. Listing and
recovering persisted jobs in the native UI, full mastering/effects, HDR and
release qualification remain open.

## Automatic admission and recovery

`render_encoding_decisions` holds one immutable observation per original
encoding attempt. `begin_render_encoding` inserts a selected decision and
advances the exact queued attempt in one transaction. Automatic attempts cannot
enter `Encoding` through the engineering transition. A cleanly stopped failed
qualification can retain its rejected or aborted observation with the terminal
transition. Unconfirmed process cleanup keeps the execution slot fenced.

The pure decision is bounded to 64 KiB and four probes. It retains the committed
output contract, frozen algorithm, exact resolved controls, ordered typed
failures, loaded-runtime facts, and the selected probe's full encoder,
verification and content observations. Synthetic probe movie hashes identify
historical measurements; the workflow does not retain those probe bytes as
durable media. They cannot authorize later execution. The project movie and
manifest remain independently retained and freshly hashed on every retry.

Retained manifest version 1 keeps its frozen engineering grammar. Version 2
embeds the exact original decision and encoding binding. Store, manifest,
historical output, resolved controls and original encoding owner must agree
before verification. The verifier may run on a different current runtime while
publication provenance preserves the original encoder's observations.

A checkpoint retry or destination reconciliation never selects an encoder.
It resolves the decision through `checkpoint.encoding_attempt_id` and runs
fresh file verification. A cold retry allocates a new attempt and qualifies
again. Neither serialized decisions nor stored verification results recreate
the live capabilities required for encoding or publication.

## Migration and remaining work

Schemas 39 and 40 already contain core 33. Their migration validates existing history
without replaying or rewriting authored JSON or patches, then adds empty render
tables where absent. Schema 40 keeps every existing render job, attempt and
checkpoint cell and gains empty publication tables. Schema 41 preserves its
publication and operation cells too. Every source schema gains an empty decision
table; migration never invents automatic qualification. Frozen version-1 adapters
reject automatic vocabulary in old job intents and nested publication intents,
and a preexisting decision table is a collision even when empty.
Earlier schemas retain their strict replay adapters. Migration keeps a
consistent pre-upgrade SQLite backup. Authentic
schema-39 fixtures cover a qualified single-Original baseline with pending redo,
an edited Source project and an accepted Generated project.

Destination publication has a separate durable journal and explicit reconciliation
protocol. It requires a new live verifier result; a stored Verified row alone
never authorizes adoption of destination bytes. Native recovery controls,
full mastering/effects, HDR and release qualification
remain required; no DP requirement or delivery gate is completed here.
