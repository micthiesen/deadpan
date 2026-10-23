# Complete frozen-layout reuse prototype (2026-10-23)

A private one-file prototype avoids rebuilding an immutable frozen timing
table when slicing retains every node. All 123 selected Core checks and ten
fresh native first-pause runs pass. Five quiet alternating pairs measure
input-to-commit medians of 257.887084 /244.544917 ms and offscreen composition
260.609875 /247.164042 ms. The composition difference is 13.445833 ms, about
5.16%, diagnostic only. No percentile or 100 ms/50 ms target PASS follows.

This is prototype evidence. The subsequent
[production qualification](full-timing-closure-reuse-2026-10-23.md) is complete;
do not pool this batch with earlier profiles or production comparisons.

## Change and safety

`FrozenAudioLayout::sliced` still completes its entire required-alias/ancestor
closure and existing derived-clock branch first. In local mode, a complete,
nonprovisional table with every node retained then runs the existing
`wire_bytes()` guard and returns its immutable Arc clone. Other cases and the
reference-command-work path retain the existing rebuild.

The private fields cannot mutate after admission. With full closure, the old
rebuild preserves every node, child, override, gap override and lineage entry,
so its wire value is identical. Keeping the complete alias loop preserves a
late missing-alias refusal. Excluding provisional tables prevents one escaping
its command. Keeping the exact wire guard preserves canonical-size refusal
for values admitted without checking canonical size. No public API, schema,
work charge, document validation, history, Store authority or publication path
changes. Core 58 /SQLite 87 remain unchanged.

## Tests and build

The isolated source is based on
`594dd38f969c996a07117374ec1b4949ab75273f`, containing the qualified
`ca40995b64fc4acdc0a5e046660a36a1faa7cee5` implementation. Only
`crates/deadpan-core/src/audio_reference.rs` changes; all other 2,049 Rust files
and six qualified inputs remain byte-identical. A fresh independent review
found no defects, including after a test-only explicit collection type change.

All selected locked release checks pass, with zero ignored:

- Five new tests compare local/reference full-closure bytes and Arc sharing,
  partial/provisional behavior, late unknown aliases and derived-clock order,
  the real canonical wire limit, and a 10,000-Hold interior pause's exact
  documents, transactions, forward/inverse behavior and table closure.
- Eleven scoped-capture tests pass.
- Three command-work tests pass, including the randomized reference comparison.
- All 104 InsertTime tests pass.

The canonical-limit test uses a structurally valid private admission with
canonical-size checking disabled and the real production ceiling. Both slicing
paths refuse its oversized canonical wire with the same error. It does not
claim an oversized public JSON input passes ingress bounds.

Formatting and the locked release app build pass. The first Core unit-test
build takes 714.358 seconds, including compilation; the app build takes
266.909 seconds. Those are build times, not edit latency. No build or test
failure occurred. Executed test copies and hashes are retained separately.

## Native comparison

Control app SHA-256:
`c36b894466eb4f16fe976924b05c02ada87b0e8872f7f9a7fc6028a60a342037`.
Prototype app SHA-256:
`f7cf2d6b7fdf416e115cd0cb6c15b361e26a54d106cbbb8620aec084a3173af8`.

Each run uses a fresh `perf make-large --beats 10000` Background/Silence-Hold
package and the unchanged `large-first-pause` replay. All seven pause checks
and one Kestrel check pass per run. Source, input and executable guards pass;
authored paths and bytes remain unchanged and the root WAL stays empty.
Each run starts below one-minute load 3. The exact load, reports, retry warnings
and scope exclusions remain retained.

The comparison is scripted input through durable commit and new-revision
offscreen Background composition. Source decoding, PCM performance, physical
display, peak memory and formal percentiles remain unmeasured. The preceding
17.760625 ms two-call postprocessing profile was an inclusive group, not a
predicted saving or a direct timer around this helper.

## Evidence

The independently re-read
[package](../../tools/media-qualification/evidence/2026-10-23-full-timing-closure-reuse-prototype/)
contains 138 files /2,932,293 bytes. Inventory SHA-256:
`10e1516fae9e4fd44d36a8a73ec249e6784017470946dc3e229374f9808d5735`.
All paths, byte counts, hashes and compressed streams verify. It retains
baseline/proposed/formatted `.rs.txt` snapshots, exact patches, review and
source/input/executable identities, scripts, all build/test logs and receipts,
and all ten native reports with before/after authored-file maps. Executable
bytes, targets, full source archive and generated project databases are excluded.
