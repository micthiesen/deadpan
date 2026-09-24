# Owned audio binding qualification, 2026-09-23

This increment builds on `983de6cdfd97bcc8030cf593daec5add2faf7ddf` and adds
the [owned timing binding representation](../OWNED_AUDIO_BINDINGS.md), guarded
history, and an owned PointCeil audio operand. Core document schema 16 and
database schema 22 replace core 15/database 21. Retained-context schema 1 stays
unchanged and explicitly rejects nonempty bindings. The base commit's
[CI run](https://github.com/micthiesen/deadpan/actions/runs/35957011235) passed.

## Behavior exercised

Binding tests distinguish surviving default plays, new plays and newly exposed
defaults after clearing an override. They retain concrete old override identities,
require explicit definition exclusions, and resolve the innermost applicable
birth without expanding Repeat plays. Exact phase terms retain different rounded
sample counts for the same local cut in different fractional-rate plays.
Nested Preserve tests reject a reference clock that crosses an intermediate
opaque processing stage, including phase terms and definition roots.

Admission tests cover duplicate or unused records, aggregate timing inventory,
cumulative traversal work, strict wire grammar and individual binding size.
A near-limit binding round-trips through the pretty project serializer even
when formatting takes its raw JSON above the compact lexical byte cap.

Actual Split and occurrence-edit transactions copy live Repeat aliases in both
lattices and phase terms while preserving captured historical identities.
Deletion prunes unused owners and timing records; a timing record referenced
only by a surviving phase term remains. Inverse patches restore exact fixtures.
Binding patches reject stale before-values and missing guards; changed-owner
reporting excludes unaffected bindings and shared clocks.

Migration tests replay all 21 prior database schemas. The new schema-21 fixture
was migrated and validated with the preserved prior executable, SHA-256
`9478d4e076d6442560d049c74b21e5e4342918f2e01b51b73ad0a7ec928b9ade`,
which reported core 15/database 21. Chronology, lineage and undo survive upgrade.
Across all prior schemas, 210 ingress cases reject binding vocabulary in old
snapshots, patches and commands, including null and empty values. Failed replay
does not promote the copied database. Imported allocation names remain reserved
after every live binding owner is removed; preview, commit, undo and reopen
validation reject reuse.

The PointCeil operand preserves signed sample labels and a nonzero selected
origin, then rebases only integer storage labels. Actual decoded WAV tests
compare source phase and partitioned reads, including nested Preserve silence
that owns no input sample but gains output points. Recursive preparation shares
work, deadline and depth controls. Dependencies remain complete for cache hits,
already observed assets and halo reads; a warm cache cannot bypass the depth
limit.

## Repository gate and review

The full gate passed on macOS 26.5.2 (25F84), Apple M5 Max, Rust 1.97.1, using
the pinned compatible LGPL FFmpeg prefix: formatting, locked workspace/all-target
Clippy with warnings denied, locked workspace tests/build, CLI doctor and native
Metal initialization/shutdown. The suite reported **1,140 passed, zero failed,
zero ignored**. All 361 source and fixture hashes stayed unchanged throughout
the gate. No source edits followed it.

[Evidence](../../tools/audio-qualification/evidence/2026-09-23-owned-bindings/summary.json)
retains counts, [command results](../../tools/audio-qualification/evidence/2026-09-23-owned-bindings/report.json),
[source hashes](../../tools/audio-qualification/evidence/2026-09-23-owned-bindings/source-hashes.json),
six compressed logs and the gate script. Changed Markdown links, eight saved
design image/prompt records and five unchanged original spec archives passed
verification.

Three independent reviewers covered the complete change, clock/phase/Repeat
semantics and PointCeil PCM, and storage/migration safety. All returned no
findings on the final source.

## Limits

This is a representation and evaluation milestone. No existing command creates
bindings in an empty document, and rendering explicitly rejects nonempty binding
state. Complete binding-aware PCM and policy evaluation, raw-recipe and movement
lifecycle, Repeat gap ownership and arbitrary-boundary Hold insertion remain
open. The tests do not establish that inserting a pause now resumes speech.

No GUI behavior changed. The saved single-Original design boards and prior native
aesthetic/keyboard reviews remain the visual evidence. The smoke test qualifies
startup/shutdown only. Playback, listening, export and every open DP requirement
and delivery gate remain outside this increment's claims.
