# Exact occurrence-support discovery, 2026-10-27

Clock discovery now asks for exact ordered occurrence paths and processed sample
bounds without building ordinary voice graphs. Normal playback still constructs
and validates the full graph before media access. This removes provider Arcs,
gain-owner/path vectors and tapes from the first pass for ordinary transparent
paths. It adds no cache, dependency, permission or increased limit.

The full constructor and planning query share structural path resolution and the
scalar support walk. Preserve ancestors use the full constructor with the same
already resolved path and budget, retaining every projection guard without a
duplicate traversal. The ordinary path retains catalog/qualification, mapping,
offset and owner-fit validation, exact owner clocks, tape-remapping arithmetic,
PointCeil and RoundEven boundaries, and aggregate work/run bounds. Even empty or
out-of-scope queries validate the complete recipe. Audible source selection and
geometric owner visibility never replace processed allocation.

This reduces actual ordinary construction work in the oracle tests. It does not
resolve the remaining Split failures or close a requirement, criterion or gate.

## Verification

Apple M5 Max, macOS 26.5.2, Rust 1.97.1, locked dependencies and qualified
FFmpeg. Audio tests use optimization level 2 with debug/overflow checks.
Independent source review finds no material correctness or work-limit issue.

| Check | Result |
| --- | --- |
| Current Plan unit/integration suite | 526 pass across 27 executables; zero ignored. |
| Current Audio library | 195 pass, zero ignored. |
| Isolated 11-file patch without unfinished Split work | Compiles; 513 Plan, 181 Audio library and all 201 existing real-WAV stage tests pass, zero ignored. |
| Full unchanged real-WAV Split matrix | Four pass, three work-limit failures; 423.31 seconds. |
| Final strict check | `cargo xtask check --no-test` passes formatting and strict all-target Core/Plan/Audio/Store lint; tests are the separate runs above. |
| Final scoped oracle rerun | All six pass after the mechanical lint cleanup. |

The six Plan oracle tests compare exact ordered paths and samples with full
construction. Cases cover root/scoped queries, hidden Preserve history, stage
ownership, stable Repeat overrides and compact million-play lookup, fractional
44.1 kHz recipes/offsets, zero-sample seams, invalid empty recipes and limits.
Ordinary/scoped cases assert strictly lower construction work; Preserve fallback
asserts identical work. Audio additionally compares scoped search halos and
rejects a foreign context. Existing real-WAV tests preserve actual PCM behavior.

The incomplete Split comparisons remain all-owner Source/Sequence `cut-three`,
limited `0+511`, and selected/nested selected `prefix-two`, limited `0+4096`.
Nested selected fails with `sound plan binding work`; the other two hit
`plan work per read`. Processed root-sound refinement still passes its complete
authored/faded/limited comparison. The matrix overlaps an isolated build and
stage run; its wall time is not a calibrated performance comparison.

The first checks found internal visibility errors and a missing test `Range`
import, both corrected before the successful suites. Strict checking then found
one formatting hunk and two unnecessary `Ok(...?)` wrappers. Their exact
post-test cleanup patch reconstructs all three tested source files against the
matrix's hashes; it changes no operation, charge or behavior. Final strict and
the scoped six-test rerun pass. Original failures remain in the evidence.

The copied matrix executable has SHA-256
`4eb3829b8a2d3ea16b374c7adf6f6a5c2d69f3498b051af30bab0a1e2f2b4565`.
All 1,389 guarded source inputs and executable bytes remain stable during its run.
The [evidence inventory](../../tools/media-qualification/evidence/2026-10-27-occurrence-support-discovery/inventory.json)
retains the exact independent patch, tested/final source reconstruction, commands,
outputs, identities and review notes. All 227 payloads verify their stored and
expanded hashes: 9,444,137 stored bytes and 37,385,390 expanded bytes. Inventory
SHA-256:
`24a960aa2c84ed6363a08fc42d1b5b60eaf72cfb7244e7f42eb66bbba7bcc2a5`.
Raw records and executables remain in
`/tmp/deadpan-support-discovery-2026-10-27`.

The next bounded investigation removes the second full historical-plan binding
inside `complete_processing_history_after_birth` by supplying the binding already
validated for the same first applicable scope. It must retain exact live scope,
historical plan and recipe authority, plus all journal/context/route checks.
That proposed follow-up is not part of this qualification.
