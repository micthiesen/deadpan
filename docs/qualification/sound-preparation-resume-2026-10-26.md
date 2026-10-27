# Sound preparation resume, 2026-10-26

This is an **incomplete development checkpoint**. The owner resumed the full
product goal after the build-tooling diversion. General sound-bearing Split
remains unqualified; its production candidate is still uncommitted. No
acceptance criterion, requirement or delivery gate closes here.

## Current candidate and inventory

The prior [delivery continuation](general-split-sound-delivery-2026-10-26.md)
reported three passing and four failing real-WAV tests. Source inspection now
finds full constant route-transfer programs and request-local permission-family
interning newer than those receipts. Their presence was not evidence that the
matrix had improved. The fresh runs below qualify their actual current state.

RenderPlan now compiles the complete sorted, deduplicated asset union for every
referenced historical sound timing. All events sharing a timing contribute to
that union. Inventory membership insertion is charged before allocation, within
the existing 100,000-unit allowance shared by clock proofs and delivery forests.
Its aggregate membership cap is also 100,000. No layout or scope is copied into
the inventory.

Audio preparation borrows this inventory only for timings selected by actual
current occurrence/remapping proofs. It keeps the existing 64-asset per-read
limit, historical-plan cache replacement and per-read source admission. Empty
current support performs one clock-count lookup, rather than rebuilding asset
sets across its journal. A strengthened 80-clock regression proves that query
fits its smaller measured allowance while an audible query still refuses.
A shared-clock regression proves that both event assets survive cached reads.

Independent reviews found no supported correctness issue in the constant
transfer programs or the integrated inventory. Neither review substitutes for
the failing complete-PCM matrix.

## Verification and failures

All runs use locked dependencies, Rust 1.97.1, the qualified FFmpeg/ORT prefixes
and the optimized Cargo test profile. This is Audio/Plan work, outside the
experimental Blaze Core-library loop. Every retained run's source guard is
unchanged during execution.

| Run | Actual result |
| --- | --- |
| `inventory-audio-units-first` | Five sound-clock tests pass, zero ignored. |
| `inventory-plan-full-first` | 87 library tests pass; two new fixtures fail validation because an unused timing record is forbidden. No production admission was relaxed. |
| `inventory-plan-full-second` | Corrected fixture: 514 tests pass across 27 nonempty suites, zero ignored. |
| `inventory-audio-regression-first` | 190 library and 201 existing stage tests pass. The seven Split tests are explicitly filtered and separately recorded below. |
| `inventory-full-split-first` | Three pass, four fail, zero ignored. All four stop at preparation-work limits before complete PCM comparison. |
| `inventory-strict-first` | Clippy rejects two complex return signatures in the new inventory compiler. |
| `inventory-strict-second` | Named internal type aliases resolve those findings; strict Core/Plan/Audio all-target Clippy passes without suppressions. |
| `inventory-pinned-split-final` | After tracing is removed and the clean executable rebuilt/copied: three pass, four fail, zero ignored; source and executed-binary guards pass. |

The final failures are:

- Selected play, `prefix-two`, limited `0+4096`: plan work per read.
- Nested selected play, `prefix-two`, limited `0+4096`: audio binding work
  exhausted under the remaining query allowance.
- Processed root sound, `root-sound-refinement`, limited `0+511`: plan work per read.
- All-owner Source/Sequence refinement, `cut-three`, limited `0+511`: plan work per read.

The passing matrix cases retain processing-depth PCM equivalence, an audible
attached-sound/nonlinear/EQ/wet-return/limiter witness and pristine read
partitioning. They do not prove the four unfinished comparisons.

The final executed stage binary is 24,355,944 bytes, SHA-256
`1178b084e0c90ba728a2eafb5b4a0e6ee60dbe26bcc3bc113b8505886eb2205c`.
It remains at `/tmp/deadpan-split-resume-2026-10-26/stages-final`. The first
matrix's Cargo execution did not separately pin its binary; the final copied
execution supplies that stronger evidence. Run durations are not a controlled
performance comparison.

## Measured remaining work

Temporary call-site tracing reproduces the selected-play baseline failure with
the current transfer programs and inventory. At the 16,777,216-unit limit,
constant transfer applications account for 5,980,514 charged units, and source
occurrence construction for 1,282,906. Clock-scope retrieval still occurs in
both preparation passes, with 303,860 units at each site. These are executed
charges from this candidate, not attribution from the older traces.

The first tracing build contained an accidental extra `+` in the temporary
patch and failed compilation; that run is retained as a diagnostic setup error.
The corrected trace fails at the expected work limit. Temporary tracing is
removed byte-for-byte: `stages.rs` is 164,819 bytes with SHA-256
`87b62882b84f510e96a94801da3e1febdeb894110fd4cee15a1de099f89c6d78`.

Simply omitting identity-transfer charges would retain per-checkpoint dispatch
work and is not an accepted fix. Further reuse must remove repeated evaluation
while retaining exact nonlinear contribution order, source checks and history
authority. Carrying all event queries into a new unaccounted cache or replacing
exact source-support filtering with conservative owner candidates is not
qualified. Prepared historical queries borrow their retained RenderPlans; a
local Arc clone alone cannot solve that ownership boundary.

## Retained evidence

[The collection](../../tools/media-qualification/evidence/2026-10-26-sound-preparation-resume/inventory.json)
contains 66 gzip payloads, 2,843,908 stored bytes and 10,847,452 expanded bytes:
commands, streams, 2,239 guarded input identities, the source overlay against
`415d0cc3d7f60437a5204cde776fb85225ec9637`, new source files, diagnostic patch,
restoration proof and final binary identity. Executables are retained locally,
not embedded. Reconstructing the overlay matches all 869 guarded inputs in the
four changed crates. Foreign `fuzz/Cargo.lock` and `.claude/` changes are excluded.
Inventory SHA-256:
`7e24bb1126fc42b8a5c21a491a12d98f818ddf5a0fe6aed2369f03fd31acb237`.

Store/history, complete sound-bearing lifecycle, native commands/receipts/macros,
emitted media and performance remain owed. Acceptance coverage stays about 85%,
strict completion 10/24 and delivery gates 0/7. The product goal remains active.
