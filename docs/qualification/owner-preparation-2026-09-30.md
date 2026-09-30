# Open-project preparation qualification, 2026-09-30

CLI Original retention, relinking, source registration and database checkpoints
now run through the native app's authenticated writer. Heavy work uses its
bounded import worker; the service commits the exact captured operation.
The [owner contract](../LIVE_PROJECT.md#background-preparation) and
[preparation contract](../IMPORT_PREPARATION.md) describe bounds and failure
semantics. Core schema 33 and database schema 42 are unchanged.

This extends DP-01, DP-15, DP-18 and DP-21 groundwork. It does not complete a
requirement or delivery gate. Native relink/checkpoint controls, persisted Render
recovery, complete editing and the full media/performance/release matrix remain
open.

## Actual native owner

A disposable generic developer project stayed open in the native app through
26 CLI invocations, following two closed-project setup commands. Each open-owner
invocation checked the actual writer lock before and after execution. Its legacy
compatibility label was expected; this was not a new single-Original onboarding
qualification.

- Linked retention and A/V registration preserved the caller's revision, asset,
  insertion identity and labels. The app displayed the decoded synthetic video
  and one 120-frame beat without reopening.
- Managed retention used a different synthetic container. Nine requests with an
  invalid selected audio stream failed, leaving authored/history rows unchanged.
  Successful audio-only registration followed immediately, demonstrating that
  those failed preparations did not exhaust the eight-entry observation archive.
  The sound appeared in Sources without adding picture time.
- After moving the linked file, relinking advanced location version 1 to 2.
  Identical-path relinking retained version 2; a stale version-1 request failed
  without changing inventory. Preview and keyboard frame navigation continued
  without reopening.
- A published SQLite checkpoint passed `quick_check` and matched the current
  document, history, redo, state and source-qualification rows. A separate live
  SQLite backup matched those rows too. Current-schema migration returned its
  existing read-only no-op result; validation passed.

The browser-role operator inspected the actual native window. Original and Edit
frame motions advanced independently; Tab traversed focus; Sources `j`/`k`
selected the sound and video. The sound displayed Ready and 4.025 seconds, with
no acoustic claim. The Edit cursor remained at 1/120 through the final view
switch. Returning from sound to the same video reset the Original cursor to zero.
The existing `select_source` path resets it unconditionally; preserving that
cursor is an explicit follow-up, not a qualified keyboard behavior. No authoring
edits were made during native inspection.

The final inventory-only Quit check confirmed the app was no longer running.
Independent process, writer-lock and discovery checks agreed, and the saved
document was unchanged. Earlier app-specific post-Quit queries showed an empty
No project window. Inspection may have relaunched the app; that mechanism is an
inference. Both observations are retained, and no shutdown code was changed on
that evidence.

Native screenshots were inspected inline. The documented CUA surface exposed no
local screenshot-save path, so no image file is claimed. There are no new layout
controls in this milestone; full painted replay, VoiceOver, IME, non-US layouts
and representative latency/memory measurements were not repeated.

## Regression scope and review

Store tests cover full-byte relink verification off the writer, stale records and
locations, namespace replacement, owner closure, cancellation and transaction
rollback. Checkpoint tests exercise edits during a pinned WAL-inclusive backup,
the actual captured revision, byte/page/deadline limits, cancelled publication,
foreign/closed owners, changed namespaces/files, setup rollback and a directory
sync failure after rename. The latter preserves the published receipt.

Service tests hold real prepared results at controlled worker boundaries. They
cover concurrent edits, stale registration, exact cancellation, unread native
commit continuations, refresh failure, old replies after session replacement,
and New-project refusal before package allocation, including during Render.
Shared/client tests cover strict bounds, complete captured intent, valid asset
deduplication, inconsistent receipt detail, lost replies, no replay and archive
release. Actual CLI subprocess tests use the production preparation boundary.

Independent review led to fixes for staging cleanup, new-project allocation
during remote preparation, no-op relink receipt validation, and terminal archive
release. Parent review added detailed registration-receipt consistency while
preserving qualified-asset deduplication.

Initial checkpoint compile errors, the incorrect test expectation that an
already-current migration must use IPC, module-order formatting and two strict
lint findings are retained with their corrections. Media bounds and semantics
were not relaxed.

## Automated verification

The locked workspace run passed 2,572 tests, with no failures or ignored tests,
including doctests. Native Metal startup and its shutdown callback passed.
All 338 optional UI-feature tests, strict workspace and UI-feature Clippy, and
formatting passed. The final code inventory is
`4f1bffeee50885a4768883c8e219507ef24dc2b1b981273078878580c71f7b75`.
The full workspace run and native binary captured the source before one later
help-text correction; preparation, store, service and client logic were unchanged.
The correction removes the obsolete claim that headless Render needs a closed
project. Final source inventories distinguish those scopes.

## Environment and retained evidence

Host: Apple M5 Max, 128 GiB, macOS 26.5.2 (25F84), Rust 1.97.1, Metal and the
pinned LGPL FFmpeg developer prefix `/tmp/deadpan-ui-ffmpeg/prefix`.
The staged native binary was SHA-256
`74fb9f28e2fbb27891b3e6975e0899cf0aed916f639e6e2be52196d82297f847`.
Its source inventory was
`0d5b629c41b5839307f58322c46ecff0dee8addeaacfea44d38fcc132d8de7a7`.
The native wrapper is a development fixture, not release packaging.

[Retained evidence](../../tools/media-qualification/evidence/2026-09-30-owner-preparation/README.md)
includes journals, original failures, source inventories, exact command output,
synthetic media, database backups/checkpoint and native observations. Discovery
secrets, runtime sockets, compiled binaries and direct copies of live main
databases are excluded. Cancellation deadlines remain cooperative; these runs
do not establish process-crash or physical power-loss recovery.
