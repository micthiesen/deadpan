# Frozen owner-query baseline v1

snapshot.json is the exact published-variant SnapshotV1 produced BEFORE owner
query admission changes. Do not regenerate it from a changed query kernel.

The baseline contains five real fixture families and 14 cases: ordinary and
captured Domains, stable Repeat plays/gaps/overrides, a hidden retained Domain
whose result differs from the live root, fractional PointCeil Shift with a
44.1 kHz source clock, and Source Split with an explicitly captured pre-Split
lattice. It records ordered clocks, provenance, sampling, complete captured
Domain/seed fields, old work, and exact/one-short errors.

The main variant's fixture and work arrays are exactly equal. Source/executable
pins, the initial corrected fixture failure, and both variants are retained in
docs/qualification/owner-query-baseline-2026-10-28.md and its evidence inventory.

These are logical planning fixtures. They grant no source/media authority and
do not prove allocation admission. Future regression tests must compare exact
semantics and separately reconcile old canonical work plus newly performed
admission work; never rewrite the old work fields to make a new result pass.
