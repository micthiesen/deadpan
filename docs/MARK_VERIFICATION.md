# Persistent marks and schema-3 verification

This report covers `Persist marks through structural edits and project history`,
based on `63e4d6258f03aef335f890b3b75a7e01de10cb13`. It extends the
[exact boundary resolver](ANCHOR_VERIFICATION.md) with authored marks and atomic
edit transforms. DP-04 and Gate B remain partial.

## Implemented contract

Schema 3 adds a bounded map of typed mark identities. Each mark stores its owner,
label, exact boundary, insertion bias, loss policy, and bound/unresolved state.
Ownership is independent of coordinate space. `set_mark` creates, replaces, or
explicitly reattaches a mark; `delete_mark` removes it. Forward and inverse
patches carry the mark changes with the structural edit. Dry runs expose those
same patches; persisted undo/redo uses fresh revisions and restores exact mark
states.

Local marks follow stable content inside their retained host. A moved host carries
its marks. At internal boundaries, left/right bias selects preceding/following
content; outside leading/trailing edges retain their side. Concrete occurrence
marks preserve complete Repeat ancestry and stable iteration IDs. A gap belongs
to its preceding play and is lost if that play becomes last. New plays do not
clone occurrence marks. Repeat wrapping explicitly selects the first new play or
marks affected concrete anchors unresolved.

Nested Retime transforms preserve exact fractions and reject removed intervals.
Source marks stay in the original asset clock, including before any timeline use
exists. Sequence marks stay at their absolute project boundary. When ownership,
content, host, play, gap, or mapped region is lost, the declared policy either
deletes the mark or retains its last bound coordinate with a typed reason.
Unresolved marks never automatically attach to a replacement ID or timestamp.
Arithmetic overflow rejects the whole edit instead of converting a mark to an
unresolved approximation.

Named point/range selectors share revision-aware `AnchorIndex` resolution.
Ambiguous Local/Source marks require explicit occurrence scope. Missing and
unresolved marks return distinct errors. This is a headless engineering surface;
the native editor's mark-setting/jump bindings are still required.

## Migration evidence

Both schema 1 and schema 2 migrate directly to schema 3 on a consistent SQLite
copy. Frozen legacy wire adapters exclude mark commands, fields, and wrapping
policies. Complete chronological replay compares every old snapshot and edit,
including abandoned branches and pending redo. Legacy projections explicitly
require empty mark maps, so an erroneous replay cannot hide newly created marks.
Only a validated candidate is promoted through the existing SQLite backup
transaction. The retained backup uses `before-schema-3-*.sqlite`.

The schema-2 fixture was produced by the unmodified old binary from
`b2c8314b44f2a2de53f61a1b158029ba5ce36f6e`.
It includes 14 revisions, eight edit records, two pending redo entries, inserted
and reordered compact repeat identities, shrink/regrowth, an abandoned branch,
and an inserted Repeat subtree. Its committed SQL SHA-256 is
`a54e53d6982a2a8b651bf3ee53e236cd456ea471227fce5e800b87d260882230`.
The original schema-1 fixture continues to exercise direct migration.

[Migration tests](../crates/deadpan-store/tests/migration.rs) compare legacy snapshots and transactions, backup contents,
identity runs, reopened redo/undo, and idempotent current-schema migration.
Injected new vocabulary and overlapping iteration identities fail before
promotion while preserving the source and backup. Existing tests continue to
exercise disk-full promotion and abrupt process death during backup promotion.

## Automated evidence

On 2026-09-20, the complete repository gate passed on Apple M5 Max, 128 GiB RAM,
macOS 26.5.2 (25F84), with Rust 1.97.1 and locked dependencies:
`cargo fmt --all -- --check`, workspace Clippy with warnings denied, workspace
tests, workspace build, and `deadpan-cli doctor`. All **123 Rust tests** passed,
with none ignored: 65 core, 29 store, 16 plan, 11 CLI, and two native headless
tests. The six Python audio-measurement regression tests also passed. Diagnostics
reports document schema 3 and still identifies media, keyboard editing, AI,
export, and distribution as unimplemented.

Independent storage review found that a redundant size/schema preflight ran
before backup creation. A readable legacy database missing a table could then
fail without a recovery backup. That preflight was removed from the original;
bounded validation still runs first on the isolated candidate. The regression
test covers schema 1 and 2 with a missing history table and a 64 MiB + 1-byte TEXT
document. All cases return `MigrationFailed` with the retained backup, preserve
the original database byte-for-byte, and preserve the old schema and corrupt
content in the backup. All nine migration tests pass after the fix.

Independent core review found no actionable defects in bias/ownership transforms,
repeat and gap identities, wrapping, nested retiming, unresolved-state behavior,
named query scope, inverse patches, or bounds. That review also checked the
migration backup fix and regression test. It was a read-only review; the parent
session ran the integrated checks recorded above.

The [core mark tests](../crates/deadpan-core/tests/marks.rs) cover insertion bias, empty hosts, moves, grouping, ownership loss,
explicit reattachment, Repeat shrink/growth/reorder, gap loss, wrapping policy,
nested Retime crops, source clocks, named ranges and scoped queries, stale
revisions, exact inverses, hostile wire shapes, patch preconditions, mark limits,
and checked overflow. A `u32::MAX`-play fixture retains compact storage.
A property test compares marked offsets across random Repeat play reorders.
The existing boundary suite retains its exact nested-retiming property test.

[Store integration](../crates/deadpan-store/tests/persistence.rs) checks preview isolation, atomic loss transforms, reopen,
undo/redo, deletion, and refusal to reattach a reused node identity. The [CLI
process suite](../crates/deadpan-cli/tests/project_commands.rs) exercises the same commands and named-mark queries with an open
writer, structured errors, and no mutation from read-only queries.

## Limits and remaining work

This change does not implement temporal attachments, sparse occurrence overrides,
copy/duplicate/explode transforms, semantic range-edit operators, or incremental
plan rebuilding. It validates existing structural operations, not the complete
future editing catalogue. Index construction still walks the authored document;
transforms share one old and one new index across marks, and identity lookup scans
compact runs. No product latency budget is claimed.

No source is decoded by these tests. Original audio sample clocks are still
caller-supplied until measured import metadata is integrated. Picture/audio
render equivalence remains an acceptance obligation.

No GUI changes were made, so live computer use adds no evidence for this slice.
The welcome shell remains the only UI. GUI aesthetics, native focus and IME,
accessibility, and natural keyboard navigation still require implementation and
focused interactive review under the full-project goal.
