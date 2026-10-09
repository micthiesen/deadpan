# DP-01 project and recovery acceptance

DP-01 is complete under specification 29.1 at implementation commit
`0ebda1e9`. This audit reconciles the Documents library, one-Original baseline,
durable history, ownership, migration and recovery against normative sections
20.1 through 20.5. It does not establish completion of the other requirements
or release gates.

The verification environment is this Mac: Apple M5 Max, macOS 26.5.2 (25F84),
Rust 1.97.1 and pinned FFmpeg 8.0.3. Core format 47 and database schema 75 are
unchanged by the final creation work. Review and verification were performed
by the sole working agent.

## Acceptance mapping

| Requirement | Implementation and evidence |
| --- | --- |
| 20.1: directory packages in system Documents, independent of source and launch path; safe names; cancellation creates nothing; preparation off the UI | [Library](../../crates/deadpan-app/src/library.rs) tests cover injected Documents placement, exclusive collisions, hostile/bounded names and refusal without fallback. [Native linked creation](linked-original-2026-10-09.md) exercised actual system Documents, cancellation, full-media initialization and keyboard/menu entry. Its replay verifies the cancelled picker leaves no extra package. YouTube creation has collision-safe staging and a verified completion reply. The database remains authoritative; [ownership](../ORIGINAL_MEDIA.md) and [formats](../DEVELOPMENT_FORMATS.md) describe package and cache boundaries. |
| 20.2: durable semantic edits, reversible history, named takes and consistent copies | [Store history](../../crates/deadpan-store/tests/revision_storage.rs), [recovery](../../crates/deadpan-store/tests/recovery.rs) and [backups](../BACKUPS.md) cover committed state, WAL-consistent snapshots, restore and preserved identities. [Named takes](named-takes-2026-10-08.md) retain authored snapshots and accepted providers; restoring one is a reversible edit. [Storage errors](storage-errors-2026-10-08.md) preserve failed-save status and its affected session. [Native Quit](native-quit-2026-10-09.md) retains unsaved previews on cancellation and drains owned encoding work before exit. |
| 20.2: one immutable Original and protected full-source baseline; generic projects retain their own workflow | [Single-source tests](../../crates/deadpan-store/tests/single_source.rs) cover atomic initialization, the Undo floor through branching/reopen, rejection of a second picture after deleting every beat, rollback, changed media, forged history and profile/qualification tampering. Generic projects retain ordinary import and Undo. [Native and headless linked-creation tests](linked-original-2026-10-09.md) verify the same 120-frame baseline for both ownership choices and recovery of an incomplete package. |
| 20.3: managed media by default, explicit linked files, content-verified relinking and truthful missing-media handling | [Original ownership](../ORIGINAL_MEDIA.md) implements APFS cloning with copy fallback and immutable content identity. Linked native creation and retry retain an external path/bookmark. The native move/reopen check preserves the authored dump and displays the verified relocated picture. [Detached-volume relinking](relink-volumes-2026-10-08.md) refuses wrong bytes and preserves history and decoded pictures. A portable copy of the newly linked project renders with the external path unavailable. [Recovery](../RECOVERY.md#missing-and-moved-originals) documents missing placeholders and identical-byte managed restoration. |
| 20.4: unclean-exit recovery, worker reconciliation, rotating backups, truthful storage failures | [Process-kill campaigns](../BACKUPS.md#process-kills) exercise commits, AI attempts, backup/checkpoint, restore, migration and proxy publication. [Render-host kills](render-host-crash-2026-10-08.md) exercise real encoding and verification, worker exit, interrupted-attempt recovery and checkpoint-only retry. [Damaged database recovery](damaged-recovery-2026-10-08.md) verifies a backup before restoring it and retains old database files. Real APFS ENOSPC, EROFS and EACCES tests and [typed failures](storage-errors-2026-10-08.md) establish failed writes and successful retry without false Saved status. |
| 20.4: newer-schema inspection, copied/validated migration, model-independent accepted media | [Backups and migration](../BACKUPS.md#release-migration-policy) cover backup, copy, invariant validation and atomic promotion, including failure recovery and a real 66-to-67 migration. [Read-only controls](readonly-controls-2026-10-08.md) refuse edits without changing newer-package bytes. Obsolete development packages follow the owner's explicit breaking-format authorization; no new migration is required for this change. Current offline/portable tests render accepted AI providers without their model runtime. |
| 20.5: one writer, authenticated headless commands, concurrent inspection | [Concurrent inspection](project-inspection-2026-10-09.md) uses two native processes: the writer commits while the viewer retains its revision, explicit reopen refreshes it, owner exit does not promote it, and a later explicit reopen acquires ownership. The private database copy permits writer WAL truncation. [Headless commands](../HEADLESS.md) use the authenticated live endpoint; current service/CLI tests cover captured commands, stale refusals and immutable render snapshots. |

## Evidence freshness and limits

The immediately preceding store/inspection implementation passed 5,481 full
workspace tests, recorded in [inspection qualification](project-inspection-2026-10-09.md).
After linked creation, all 1,783 app/CLI tests passed. The subsequent explicit
Open-completion correction passed 38 focused tests and all 319 project-service
tests. Final workspace Clippy and formatting passed. The final release app
`bbec00017583e8d42d7bed4c89b5cd31a5c16e102f7787eec65fd1b7bad3a6d0`
passed 20 linked-creation and 63 YouTube replay checks, plus both shortcut
audits. The completion regression deliberately delays workspace delivery.
Earlier failures, isolated pipe-warning reruns, exact source/binary identities,
native Accessibility checks and retained files are in the
[creation qualification](linked-original-2026-10-09.md). This is cumulative
evidence, not a claim that the entire workspace suite was rerun after every edit.

Physical power interruption, actual File Provider eviction, Dock-menu Quit and
logout remain on the [owner verification list](../REQUIREMENTS.md#to-verify-owner)
with exact steps. Specification 29.1 explicitly assigns those unavailable or
physical checks to the owner without blocking local completion. Their software
boundaries have process-kill, filesystem-failure and native termination evidence;
those checks do not prove the unexercised physical events.
