# Audio query groundwork, 2026-10-26

Routed PCM preparation now accepts borrowed queries without cloning their
processing graphs. Existing owned entrypoints forward to the same loops.
Core and Plan also expose checked, bounded visitors for the owned heap storage
of routed roots, source occurrences/batches and processing queries.

This is preparation for removing repeated sound-query construction. It adds
no query cache, dependency, source authority or increased limit. It does not
resolve the remaining Split work-limit failures or close a requirement, scored
criterion or delivery gate. The [memory contract](../AUDIO_QUERY_MEMORY.md)
distinguishes retained storage from transient-construction and process-memory
qualification that are still required.

## Verification

Apple M5 Max, macOS 26.5.2, Rust 1.97.1, locked dependencies and qualified
FFmpeg. Audio deadline tests use optimization level 2 with debug/overflow checks.
Independent source review finds no issue in field coverage, arithmetic/work
bounds, source admission or borrowed processing. A separate scope audit finds
no dependency on the excluded, unfinished Split work.

| Check | Result |
| --- | --- |
| Current Core library | 795 pass, zero ignored. |
| Current Plan library | 95 pass, zero ignored. |
| Current Audio library after fixture correction | 194 pass, zero ignored. |
| Existing real-WAV Audio stages | 201 pass, zero ignored; the seven unfinished general Split tests are explicitly filtered out. |
| Focused Plan heap tests after lint correction | Six pass, zero ignored. |
| Final `cargo xtask check --no-test` | Formatting and strict all-target Core/Plan/Audio/Store lint pass; tests are the separate runs above. |
| Isolated 23-file overlay on HEAD | Offline all-target Core/Plan/Audio check passes with a separate Cargo target directory. |
| Isolated overlay library tests | 180 Audio and 82 Plan pass, zero ignored, without the unfinished Split candidate. |

The borrowed-reader witness decodes the real mono 44.1 kHz WAV. It covers raw
Source, ordinary occurrence and Preserve occurrence inputs, retained route gaps
and repeated reads of one prepared query. Bitwise PCM, suppression, exhaustion,
dependencies, relative depth, work counters and provider-call counts match the
owned entrypoint. Revoking the provider still refuses a reused query.

Heap tests cover spare capacity, identifier text, nested Repeat/Preserve graphs,
mixed shared projections, route/ripple private indexes, overflow, traversal
exhaustion and exclusion of borrowed plan data. Visitors enumerate the actual
owned fields rather than multiplying query-work estimates.

The first borrowed test fails during fixture setup because its asset lacks
source qualification. The repair supplies a qualified asset in the test while
preserving the production admission guard. Core/Plan inputs stay byte-identical
through this Audio-test-only correction. Two later test-only Clippy findings
replace manual slice-size calculations with `size_of_val`; the affected heap
tests and strict checks pass afterward. Original failures remain recorded.

Raw records are in `/tmp/deadpan-query-groundwork-2026-10-26`. Copied executables,
source guards, commands, outputs and the exact independently scoped patch are
retained in the [evidence inventory](../../tools/media-qualification/evidence/2026-10-26-audio-query-groundwork/inventory.json).
All 65 compressed payloads verify their stored and expanded hashes: 2,256,911
stored bytes and 9,171,976 expanded bytes. Inventory SHA-256:
`8304ddfd5f82f74dadf7b49ede911f31d0082ccf7e3c663320f1edb76197940f`.
Executables remain local. This qualification makes no speedup or full-product claim.
