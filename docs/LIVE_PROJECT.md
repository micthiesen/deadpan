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
| Render, retry, re-encode and reconcile | Admit through the native service's shared render coordinator and observe the exact workflow. |
| Render cancellation | Require the exact job, attempt and cancellation token. |

Original retention, relinking, source registration and database checkpoints
still require a closed writer through their CLI entrypoints. Routing their
preparation and completion through the service remains required by specification
Section 20.5. This milestone does not complete DP-21.

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
A remote mutation returns `HostBusy` while a native commit receipt remains
unread by the UI, preserving that edit's cursor and selection continuation.

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
