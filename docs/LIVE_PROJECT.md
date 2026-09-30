# Commands for an open project

The native project service advertises an authenticated local endpoint while it
owns the package's writable store. CLI commands use that owner when the package's
writer lock is already held. They parse request files once and send typed
operations. Read-only inspection and dry runs can still use independent SQLite
readers.

## Supported operations

| Operation | Open native project behavior |
|---|---|
| Structural `command` | Execute the submitted project, revision and typed command through the existing store transaction. |
| Undo and redo | Use the explicit expected revision and a fresh revision allocated by the caller. |
| Adopt primary geometry | Use the submitted revision and the store's existing qualified geometry boundary. |
| Retain or relink an Original | Prepare complete-byte verification on the import worker, then commit the inventory change on its writer. Relinking retains the caller's expected location version. |
| Register a source | Decode exactly the selected streams on the import worker, then commit the complete caller-supplied registration at its expected revision. |
| Database checkpoint | Prepare a consistent SQLite backup on the import worker, then publish it through the original writer. |
| `project migrate` | Current-schema packages use the existing read-only validation and no-op result, including beside a native owner. Only a migration writer-lock conflict takes the IPC fallback. |
| Render, retry, re-encode and reconcile | Admit through the native service's shared render coordinator and observe the exact workflow. |
| Render cancellation | Require the exact job, attempt and cancellation token. |

The CLI first attempts its existing closed-project operation. Only an actual
writer-lock conflict selects the live owner. A writer without an authenticated
endpoint returns `HostOwnerUnavailable`. The current-schema migration fast path
does not acquire a writer or contact the owner. The explicit IPC `Migrate`
operation can report an admitted owner's current schema without releasing its
writer; it does not migrate a legacy store. Older closed packages retain their
existing explicit migration path. This boundary does not complete DP-21 or add
CLI generation, analysis, macros or the remaining editor commands.

## Ownership and authentication

The store captures the actual package and held lock descriptor identities.
`WriterOwnerHandle` identifies one writable open and is revoked before the
writer lock is released. Copies of the handle do not retain the lock or store.
The service checks it immediately before dispatching a request.

Discovery is a bounded, current-user-owned, mode-0600 `.host.json` record in the
package. It binds the package directory identity to a fresh owner UUID, a random
secret and a private runtime socket directory. The runtime directory is mode
0700 and has a short path independent of the package path. Discovery contains a
secret and must not appear in logs, diagnostics or retained evidence.

This authenticates possession of a same-user filesystem capability. It makes no
macOS peer-credential, application-signature or hostile-same-user sandbox claim.
Unsafe file types, modes, hard links and replaced package, lock or registration
identities are rejected. Old endpoints cannot erase a successor's discovery or
runtime namespace. Close, reopen and project replacement revoke the old owner.
An Open prepared while a render drains does not advertise until installation.

## Bounded transport

Each connection carries one version-1, length-prefixed JSON request and reply.
Both envelopes bind the owner, package and request UUID. Unknown fields and
unsupported versions are rejected. A client retains its original discovery;
later requests never rediscover and silently retarget a new owner.

The endpoint admits four connections, at most 64 MiB plus a small envelope per
frame, and a 256 MiB transport reservation including estimated JSON allocations.
Socket I/O is nonblocking. One poll performs at most 256 KiB and 32 socket calls;
receive and write deadlines are ten seconds. Waiting for semantic dispatch is
bounded to 300 seconds; ordinary client requests use a 30-second deadline.
The service returns to render polling between admitted requests. Typed document
validation remains bounded service work and is not a hard real-time guarantee.

Socket creation and acceptance set CLOEXEC and nonblocking flags within the
same cooperative macOS guard used by owned subprocess launches. Connect, poll
and waits happen outside that guard. Accept skips a contended guard instead of
waiting on another launch. Foreign launches do not participate. Package aliases
are resolved once at client admission; retargeting an alias cannot retarget an
already admitted client.

## Receipts and failure

A successful edit reply retains its durable revision even if refreshing the
native workspace fails. If a detailed reply exceeds transport capacity, a small
failure reply preserves `committed_revision`. Neither failure authorizes replay.
Socket loss after a request begins can instead produce `HostOutcomeUnknown`:
the caller must inspect the project before repeating a mutation. Request UUIDs
correlate messages; they do not promise deduplication or exactly-once execution.

Replies have their own connection tickets and never consume the GUI's update
mailbox. A remote edit publishes the resulting workspace. Generic operations
take the same short admission slot as UI commands; status and exact cancellation
remain available while that slot is busy. They cannot clear another command's
admission flag.
A remote short edit returns `HostBusy` while a native commit receipt remains
unread by the UI, preserving that edit's cursor and selection continuation.

## Background preparation

Retention, relinking, registration and checkpoints use typed `prepare`,
`preparation_status`, `cancel_preparation` and `release_preparation_status`
operations. The client allocates an operation UUID and cancellation token before
admission. Every observation uses that exact target and retained owner through
a separate bounded connection; no socket remains open for copying or decoding.

One preparation shares the existing import worker with native imports. Its
connection-free handles carry the original store's authority and are revoked
when that session closes. The worker hashes, copies, decodes or backs up the
database. The project service alone commits inventory or authored changes and
publishes completed checkpoints. Ordinary edits and existing Render observations
continue while preparation runs. A new import or preparation is refused while
the shared worker or its pending commit is occupied.

The states are `preparing`, `awaiting_commit`, `cancelling`, `completed`, `failed`
and `cancelled`. `awaiting_commit` retains a completed worker result while a
queued native command or unread native commit receipt prevents safe publication.
Cancellation sets the exact operation's cooperative flag. A running worker must
return before the service reports cancellation; a timeout does not prove it
stopped. Close, project replacement and shutdown request cancellation and drain
the old worker without applying its result to a replacement session.

Preparation commands are limited to 64 KiB serialized JSON. Paths and bookmarks
are each bounded to 16 KiB and labels to 4 KiB. Paths retain the CLI's absolute
path and no-parent-traversal rules. Original preparation uses the decoder input
bound, currently 64 GiB, and a 300-second cooperative limit per preparation phase.
Checkpoints default to 1 GiB, 32 SQLite pages per backup step and five minutes.
SQLite and filesystem calls remain cooperative rather than preemptible.

The native owner requests cancellation after 15 minutes. The CLI polls at
50-millisecond intervals, requests cancellation on SIGINT, SIGTERM or its own
15-minute limit, and observes drain for up to five more minutes. An unconfirmed
drain or lost reply reports an unknown outcome with the captured target. It does
not release ownership, invent cancellation, rediscover an owner or repeat work.
The service retains at most eight preparation observations. Terminal entries
expire after ten minutes or an explicit release; active entries do not expire.

### Exact intent and receipts

Source registration retains the caller's complete `SourceRegistration`, including
expected/new revisions, asset identity, labels and optional insertion, plus the
explicit video/audio stream choice. It never borrows GUI selection, synthesizes
a new insertion or rebases a stale revision after decoding. A matching existing
registration reuses its qualified asset even if `new_asset_id` proposed a different
alias. Without insertion, it may return a successful no-op receipt without an
authored revision. Preserving caller intent does not disable that existing store
deduplication rule. Detailed output must agree with the resolved asset and
qualification in the retained receipt.
As in the closed CLI, registration supplies no invented generation relevance:
current requests requiring host context can return `GenerationRelevanceRequired`.

Every completed preparation retains a typed operational receipt independently
of workspace refresh and detailed output: retained/relinked content and location
version, registered asset and qualification, or checkpoint path and captured
project/revision. An identical relink can preserve the current location version.
A checkpoint identifies the worker's actual consistent read snapshot, which can
differ from the revision present at admission or publication. Its media remains
in the package.

`completed` means the operation has finished; its separate `completion_error`
can report a failure after publication. If checkpoint rename succeeds but the
directory sync fails, `CheckpointPublishedUnconfirmed` preserves the published
path and receipt. The CLI writes that receipt, reports the error and exits
nonzero. The caller must inspect the published file before repeating the command.
Refresh errors likewise preserve receipts and do not make a mutation retryable.
Compact replies retain these fields when full detail does not fit.

Preparation commands emit their existing final JSON shape, without intermediate
stdout events. A failed final stdout write leaves the completed observation
available until expiry. Successful output releases it; known failed or cancelled
operations also release their terminal entries. A failed release cannot reverse
an already observed result. These observations are process-local, so closing the
owner removes them; retained inventory, authored history and published checkpoints
remain the authoritative persistent state.

## Render observations

Remote Render uses the same qualified automatic policy, immutable revision,
worker teardown and publication journal as native Render. Temporary Camera,
Gain and Room tone editors publish their presence before becoming active.
Admission refuses to silently commit or discard them. The user resolves the
preview in the app before sending Render again.

The client retains the admitted project, captured revision and exact workflow
target through polling and cancellation. A terminal stage alone is insufficient:
the service explicitly reports whether the coordinator can release its writer.
A lost observation produces a recovery diagnostic, never an invented terminal
result. No transport error falls back to a new local writer or replays admission.

The service retains up to eight remote render observations, including completed
results after a later native render starts. Archived terminal observations expire
ten minutes after safe release or are explicitly released by the admitting
observer. A separate cancellation caller does not release another client's
observation. The current coordinator remains queryable until replacement; a
target with neither a current coordinator nor an archive fails explicitly.
Closing the project invalidates the entire owner; persisted job and publication
status remains independently inspectable.
Read-only `render status` continues to describe durable history, not live progress.

The CLI emits the existing admitted, progress, finished and recovery JSON events.
Signals and output failures request cancellation of the captured target. Losing
the client does not stop the native service's worker ownership or cleanup duty.

[Qualification](qualification/live-project-2026-09-30.md) records actual native
ownership, concurrent editing, cancellation and historical recovery. CLI-started
work does not automatically open a native status window; native persisted-job
recovery and easier access to these jobs remain open.
