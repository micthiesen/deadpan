# Semantic refresh duration reuse prototype (2026-10-23)

A private prototype reuses one freshly validated duration map while refreshing
the staged semantic scope. Ten selected integration checks and ten guarded
native first-pause trials pass. Five quiet alternating pairs measure commit
medians 245.353334 /240.185084 ms and offscreen composition
248.012042 /242.880417 ms. The composition difference is 5.131625 ms, about
2.07%, diagnostic only. Production qualification, formal percentiles and the
100 ms/50 ms targets remain open.

## Change and review

Only `semantic/planner/selection.rs` changes in production code. In local mode
with no current derived clocks, `refresh_children` obtains `durations_shared`
once and runs its original bounds, child ends and child indices assignments
inside the existing `with_proved_durations` scope. The same current document
is borrowed immutably; separate mutable borrows touch only the three derived
fields. Original query order and partial updates on failure remain. Reference
and derived-clock paths retain the original code.

Both boundary queries and both AnchorIndex constructions remain, as do all
validation, byte/work charges, provider/allocation calls, edits, history and
Store authority. No public API, proof type, global cache or schema changes.
The first complete validation remains before parent/index/ancestry errors.
The duration proof cannot escape the borrow or cross a document replacement.

Independent source review found no defects. A focused sealed-clock follow-up
also found no reachable difference: historical captured-layout validation has
its own clock checks, and the empty current-clock guard preserves live
structural admission. Both records are retained. Debug assertion behavior still
needs the full production suite; this first probe uses locked release tests.

## Source and checks

The isolated archive is based on
`56f748565189dd2c759f927f18c845faec3d1160`, containing qualified implementation
`ae5eedd065b851242917b38a428ee40a3588978d`. One source file changes and the
new standalone `tests/semantic_refresh.rs` target is added, taking Rust paths
from 2,050 to 2,051. All other sources and six qualified inputs remain fixed.

Four new tests compare all six public SemanticPlan fields, exact documents and
requests, trace, register writes, replay and inverse behavior. They cover
repeated pauses/motions over a bound marked document, nested scopes with zero
groups and outside siblings, late provider/allocator failure order with
unchanged inputs, and nested FollowSpeed fallback. Three semantic-trim and
three command-work tests also pass, all with zero ignored. Formatting and the
locked release app build pass.

The initial integration compile fails with E0624 because its injected-error
helper calls crate-private `EditError::new`; no test executes. Root changes
only that helper to the identical public code/message/current_revision fields.
Exact comparison confirms every other test byte is unchanged; production code
is unchanged. The failed source, logs, maps and receipt remain separate from
the successful five-stage resumed build. No assertion or fixture is weakened.

## Native comparison

Qualified control app SHA-256:
`2761f275c70d471b202ffdf2faa9e047724a169c9607775ff4b4ee6208942608`.
Prototype app SHA-256:
`9a5951c9a74e8f4e7f9536e20ec6036b56a2b631ef716871685c56c761f87ce6`.

Each run starts a fresh process and fresh `perf make-large --beats 10000`
Background/Silence-Hold package under one-minute load 3. All seven pause and
one Kestrel check pass per run. Exact source/input/executable guards and
authored-path/byte checks pass; only exact root SQLite sidecars are excluded,
and WAL remains empty. Retry warnings and scope exclusions remain in reports.

These results are separate from earlier production/prototype batches. They
do not attribute the old 30.641416 ms semantic-execution residual to refresh,
calibrate overhead or qualify physical display, source decode, PCM performance
or peak memory. Core 58 /SQLite 87 remain unchanged.

## Evidence

The independently re-read
[package](../../tools/media-qualification/evidence/2026-10-23-semantic-refresh-duration-reuse-prototype/)
contains 161 payload files /2,865,045 bytes. Inventory SHA-256:
`a08e6b54e60cc159945cc28136d4393876138caba980913fa9d42f24e30b6c70`.
Every path, byte count, hash and compressed stream verifies. Exact original,
proposed, failed-test and final sources are `.rs.txt`, alongside patches,
reviews, identities, scripts, initial/resumed receipts, all logs and all ten
replay reports and input maps. Executable bytes, targets, source archive and
generated project databases are excluded. No production adoption is claimed
by this record.
