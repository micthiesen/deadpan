# Compact audio bindings and complete-capture fallback

Working candidate, 2026-10-23. All 1,513 Core tests pass across 69 binaries,
with zero skipped. Production remains uncommitted; broader/UI, strict,
actual-media/native, portable reopening and current-format performance
qualification are still owed. No target or gate PASS is claimed.

Core 58 / SQLite 87 omits only six exact defaults: project-root clock, empty
argument and birth paths, absent resume, zero local resume boundary and zero
phase constant. Every nondefault remains explicit. The standalone reader's
bounded raw preflight counts missing paths as zero entries and retains its
limits for present paths. Tests cover fixed wire literals, all clock variants,
signed nonzero phase and boundary values, valid nested placements, malformed
paths, patch/inverse restoration and full state/document round trips. Historical
fixture projections retain their explicit old-header grammar. Separate frozen
audio-context and sound-clock formats are unchanged.

Scoped capture falls back to the existing complete capture after its prefix
checks when N−1 aliases cannot produce a smaller projection. Randomized
all-but-one scopes, empty roots, unknown aliases above and below the threshold,
precharge failures and the existing sparse/10,000-child slice properties cover
the fallback. The [disposable prototype study](first-pause-binding-prototypes-2026-10-23.md)
measures its possible gain; that is not current-format qualification.

Independent general and codec reviews found no production defect. Capture
review suggested a below-threshold unknown-alias case, which was added. Further
test review found a malformed nested rejection fixture. It now starts from a
validated resume-term/reanchor state, confirms an untouched round trip, then
mutates one field. Oversized opaque arrays must reach the raw sequence limit
before typed member decoding. All three duplicate-placement controls execute.

The first build failed on two borrow conflicts in new private tests. After
capturing root IDs before mutating nodes, 1,512/1,513 tests passed; the malformed
negative fixture failed its three-placement control. Only the new integration
test changed after that run, then all 1,513 passed. Production logic stayed
unchanged through the fixture correction. Source/input/executable hashes are
retained for each phase.

[Core checkpoint evidence](../../tools/media-qualification/evidence/2026-10-23-compact-bindings-core-checkpoint/README.md)
contains 91 payload files / 3,784,283 bytes. Independently verified inventory
SHA-256: `b1888ad393a84734a79d17f9a9fd64a617740078b38ac20b6aef6914b527a4f7`.
Both failures, source attribution, review dispositions and complete test output
are retained. Large nextest archives and executables stay in scratch storage;
their exact executed identities remain in the inventory.
