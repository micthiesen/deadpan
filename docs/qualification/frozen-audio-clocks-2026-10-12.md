# Historical exact audio clock qualification, 2026-10-12

`FrozenAudioLayout` now captures effective fractional Retime geometry separately
from original integral processing counts. The [contract](../FROZEN_AUDIO_CLOCKS.md)
describes admission, projection, bounded indexing and the remaining consumer
guards. This is a prerequisite for retained output history, not complete Retime
insertion or a persisted owned output route.

The primary implemented and self-reviewed alone from base `60b6287f`, using
Rust 1.97.1 with locked dependencies on the Apple M5 Max Mac. No live window,
browser, physical audio output or listening was used.

Core cases cover complete and structural capture, bounded JSON round-trip,
separate recipe/effective counts, unclamped fractional crop origins and support,
stable Repeat paths, sparse play and owned gap branches, a billion-play index,
explicit work exhaustion, safe complete-layout slicing, missing aliases,
changed-clock correspondence and legacy binding refusal. Malformed windows,
nonpositive extents, non-Retime or missing owners, a fractional root, nonunity
Partitions, duplicate keys and unknown fields are rejected.

Plan cases compare historical FollowSpeed policy spans, complete allocations,
owner identities and extents with the production live plan at 30000/1001 fps.
Cuts at 1/2, 1/3, 17/31 and 5/4 frames preserve the global root sample phase;
three output frames allocate 4,805 samples. Partial queries crossing each seam
retain the same complete allocation. Processing-domain lookup preserves the
1/4-frame cropped input origin. A billion-play query selects the final stable
play with bounded node work. Legacy independent-sound paths and historical
Preserve preparation refuse exact layouts explicitly.

The initial focused core build caught two test-helper arguments typed `u64`
where Repeat identities use `u32`; no assertions ran in that attempt. Corrected
focused tests passed all seven core cases and the plan cases.

Final check, 2026-10-15: `cargo test -p deadpan-core -p deadpan-plan --locked`
passed with no failures, strict Clippy for both crates and `cargo fmt --check`
were clean, and `cargo check --workspace --all-targets --locked` compiled every
dependant. An independent review found no correctness defects. Alias renaming
of `derived_clocks` is implemented but not yet reachable: project timing tables
refuse exact layouts, so slice renaming only sees empty clock tables until
project-embedded admission lands, which must add a rename round-trip test.
Clocked layouts slice to a complete clone, so their compaction is bounded by
the existing JSON byte limit rather than spacer reduction.

No new PCM, mastered or encoded output qualification is claimed for this
geometry change. Retained physical output grids, resume history, aggregate
placement/policy, lifecycle transforms and atomic insertion remain open. No
full gate, live-native, physical-audio or performance qualification was run.
No requirement or gate is newly complete.
