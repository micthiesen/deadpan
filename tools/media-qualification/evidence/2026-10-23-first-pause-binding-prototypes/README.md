# First-pause binding prototypes, 2026-10-23

Diagnostic experiments against qualified shipping commit
6916ee0d578a11c37e6e66a470eb430b02773764, with isolated sources and target
directories. No production or target qualification is claimed. See
docs/qualification/first-pause-binding-prototypes-2026-10-23.md.

The five-pair batches retain raw command JSON, load reports, timings and
source/input/executable hashes. Binaries, Cargo targets and generated 10,000-beat
SQLite packages are deliberately excluded. Reproduce using retained procedure
scripts and overlays against the recorded source snapshot. Rust overlays use
.rs.txt to avoid rustfmt discovery.

Binding v1 omitted only four fields and was not measured. v2 fixes that
preparation error. Both v2 and combined measured prototypes still lack the
standalone reader preflight correction. Their timing runs do not prove
reopening or codec correctness. The initial direct test probe first failed at
Apple LLVM linking; Rust thin LTO linked it and exposed missing arguments in
the standalone reader (five controls passed, one roundtrip failed). The draft
source was later refined before retention; the probe receipts retain its
original source hash, but its exact original source copy is unavailable.
No claim is made that the current refined draft was executed by that probe.

The production candidate is separate: Core58/SQLite87, bounded optional raw
path preflight, historical fixture adapter and stronger tests. Its verification
does not belong to this diagnostic inventory.
