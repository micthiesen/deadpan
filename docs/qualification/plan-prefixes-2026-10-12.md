# Exact prefixes in production plans, 2026-10-12

Production picture and audio plans now carry Sequence prefixes and Repeat
allocations as exact ratios. Definition indexing, Hold boundary queries,
audio owner enumeration, independent source-voice ancestry and retained audio
contexts use those coordinates without converting them back to integers.
This connects the [exact Repeat index](exact-repeat-2026-10-12.md) to the
existing compiler and walkers.

`ExactRepeatLayout::from(RepeatLayout)` embeds the checked integer index without
rebuilding its runs or making an exact-duration map. Existing documents keep
integer Repeat storage and lookup arithmetic behind the exact interface.
The independent rational implementation remains available for derived clocks.
Comparison budgets and stable play/gap identities cross that interface unchanged.

Implicit gap recipes retain their authored duration separately from exact
allocation coordinates. This preserves the existing DSP operand and avoids
deriving a processing duration from rounded allocation. Compiled node extents,
Retime mappings and Preserve descriptors still retain their existing contracts.

`single_occurrence_exact_range` returns exact Sequence ancestry offsets and
reports arithmetic exhaustion separately from absent or remapped occurrences.
The existing integer query delegates to it and returns no range when either
endpoint is fractional or exceeds i64; it never rounds.

## Verification

Base `c99fd890b99138b52f0ae7af87c442b64a39a06f`; locked dependencies,
Rust 1.97.1, Apple M5 Max, 128 GiB, macOS 26.5.2 (25F84).
The primary implemented and self-reviewed alone.

The two new plan tests pass. A validated synthetic document contains Source
picture/audio recipes, nested Sequence groups, a three-play Repeat, independent
play/gap overrides, implicit silence and an outer nonunity Preserve stage. Recompiling
its Repeat index through the rational implementation produces the same picture
samples, serialized audio spans and processing descriptors, owner occurrences,
query work and Hold context as the embedded integer implementation. Cold queries
use 1-, 31- and 1,601-sample partitions at 30000/1001 fps. Direct definition
queries also compare the PointCeil signal walker below the outer Preserve stage,
both ordinarily and with processing flattened for policy inspection.

The occurrence-query test injects fractional and overflowing compiled offsets
to verify exact output, integer refusal and explicit arithmetic failure. That
injection tests the coordinate query only; it is not a persisted fractional
project or a qualified fractional render. The existing core parity test also
compares the embedded index, its stable IDs and its comparison budget with both
other interfaces.

All 2,910 core, plan, audio, playback and store tests pass, with none failed or
ignored. After that run, only the parity test gained the direct PointCeil
assertions above; both focused tests pass again. Production Rust is unchanged
between those runs, and the evidence retains both source snapshots.
Strict all-target Clippy for those five crates and workspace formatting pass
against the final sources.

The first type check found four mistaken method calls on `ExactFrameRange`
fields and an obsolete import. Those are corrected. The
[evidence](../../tools/media-qualification/evidence/2026-10-12-plan-prefixes/metadata.json)
retains that failed check, complete later logs and source identities. The broad
and final focused runs use different plan test executables through feature
unification; both are hashed. The initial focused executable was replaced by
the test-only extension and was not separately hashed.

## Remaining work

No saved format or native control changes. Persisted effective extents, exact
node-duration consumers, mark transforms, the owned Preserve splice route and
the atomic insertion/lifecycle integration remain required. Retime insertion
remains guarded. This increment does not qualify a new GUI, physical audio
path or performance target, and no DP requirement or gate is newly complete.
