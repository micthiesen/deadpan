# Publication recovery qualification, 2026-09-30

## Result and scope

The durable publication library passes 20 real SIGKILL cases across a structural
Source project and an accepted Generated Hold project. Each case freshly verifies
the retained checkpoint before publication and again before reconciliation:
40 verifier runs, zero new encodes. Exact authored/history cells remain unchanged.
Database migration from genuine schema 40 to 41 preserves prior operational cells.

Per project, five cases remain definitely unpublished, two remain unresolved,
two confirm Published and one records PublishedUnconfirmed. The process exits
with signal 9 in every injected case. Reconciliation preserves all file bytes,
identities and names, including orphan reports, partials and foreign replacements.

Independent FFmpeg and AVFoundation readers pass the two confirmed final movies:
138 pictures, 221,021 authored sample frames, 414 complete planes and 102,643,200
image code values. The input I420 and canonical PCM references are rehashed and
bound to the same complete contract, historical document hash and encoder policy
as the earlier qualified renders. This run freshly decodes the recovered files;
it does not claim a new encoding or a larger content corpus.

The full locked workspace invocation passes 2,318 tests across 167 target results,
with zero failures and zero ignored tests. Strict all-target workspace Clippy and
formatting pass. Native qualification and all three gates use the same source
inventory; no source changes followed native execution.

## Environment and reproducibility

Host: Apple M5 Max, 128 GB memory, macOS 26.5.2, arm64. Use Rust 1.97.1 and the
locked workspace. FFmpeg uses the explicitly selected development prefix
`/tmp/deadpan-ui-ffmpeg/prefix`; its retained build metadata and the independent
reader reports identify the actual linked versions. Native checks use APFS.

`qualify_publication_recovery` takes a caller-prepared schema-40 scratch package,
job ID, retained encoding attempt, current worker executable and new output
directory. It migrates the package, runs the real publication host in child
processes, kills each at its chosen boundary through checked process-group
teardown, reaps once, and performs fresh verification/reconciliation.

```text
qualify_publication_recovery PACKAGE JOB_ID ENCODING_ATTEMPT WORKER OUTPUT_DIRECTORY
```

Structural fixture: `durable-structural` / `structural-encode-1`, revision
`background`, range `[20,128)`, 320×180, 30000/1001 fps. Generated fixture:
`durable-generated` / `generated-encode-1`, revision `ui-generated-ready`,
range `[0,30)`, 1920×1080, 30000/1001 fps. Both use explicit engineering hardware
encoding with no B-frames, retained from the prior qualification.

## Injected cases

| Process stops after | Expected recovery |
| --- | --- |
| Intent commit | NotPublished |
| Prepared files and evidence commit | NotPublished |
| ReportCommitting commit | NotPublished |
| Report rename, before ReportCommitted | NotPublished |
| ReportCommitted commit | NotPublished |
| MovieCommitting commit, before rename | Unresolved |
| Movie commit returns, before terminal record | Published |
| Terminal Published record | Published |
| Movie commit, then report moved aside | PublishedUnconfirmed |
| Movie commit, then identical bytes replace its inode with restored mtime | Unresolved |

Read-only reopen must preserve the exact pre-crash record. Writer reopen must
interrupt active work while retaining phase/evidence, or preserve a terminal
commit. A second writer reopen must preserve the reconciliation result. Every
case retains both verification attempts and both publication operations.

## Additional checks and failures

The native descriptor adapter checks an actual APFS file and directory UUID,
keeps its borrowed descriptor usable, and rejects unsupported or malformed
identities. Filesystem tests cover ancestor/symlink replacement, file replacement,
links, mode/type changes, concurrent locks, mutation during readback and revoked
authority immediately before rename.

Store tests inject failure before identity checks, database and WAL full sync,
directory sync, and final identity checks. A committed row remains visible,
all earlier permits are revoked, and further publication writes require reopen.
Migration tests compare every preexisting cell against a genuine schema-40 dump
and its retained pre-upgrade backup. Host tests require the exact fresh verifier
identity even when two verification reports contain identical observations.

Initial checks exposed three SQL bound conversions using unsupported `usize`
parameters, a foreign-key ordering problem in the authentic dump test setup,
an inconsistent changed-mode diagnostic, and an assertion comparing selected
and canonical macOS paths. These were corrected and the affected checks passed.
The evidence retains the failed invocations, including two lockfile refusals
while integrating the example dependency and one wrong binary target name.

## Evidence and limits

[Retained evidence](../../tools/media-qualification/evidence/2026-09-30-publication-recovery/README.md)
includes every invocation and source inventory, exact native binary identities,
crash reports, coherent SQLite backups, movie/report/partial bytes, independent
decoded pictures and PCM, bound references, archive member hashes and reviews.

SIGKILL occurs between completed host calls. It does not test death inside a
rename, SQLite commit or sync syscall, nor physical power loss or hardware flush
honesty. Strict direct full-sync barriers are tested as OS operations and through
failure injection. Cooperative replacement checks do not authenticate against a
malicious same-user process. Durable destination recovery is APFS-only; this does
not qualify other filesystems. No new sanitizer or GUI run is claimed.

Native Render, public headless render commands, automatic platform policy,
complete mastering/effects, HDR, scheduling and release qualification remain
open. DP-17 and every other requirement/gate retain their existing open or
partial status.
