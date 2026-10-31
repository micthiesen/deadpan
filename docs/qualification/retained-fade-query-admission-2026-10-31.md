# Standalone retained fade and native read admission, 2026-10-31

This checkpoint admits standalone retained fade metadata and keeps its owning
lease alive through native consumption. It does not close a requirement or Gate.
General sound-bearing Split remains disabled.

AudioOwnerRetainedInput::fades_admitted returns the existing fixed
AdmittedAudioFadeQuery. False mode calls the unchanged ordinary algorithm
without spend, reserve or merge callbacks. True mode uses one cumulative query
budget. Finite processing, Bound, Box and editorial copies preserve the actual
input's stored work, identities, enum variants and nested arrays.
History queries retain the complete signed range, including support beyond
current delivery. Shared geometry, envelopes, edge sides and origins retain
their overlapping leases through the final collection and checked seal.

The native retained reader holds fade and optional owner carriers through
validation, PCM room admission, source and nested reads, fades, gain and the
final control check. It drops them before returning PCM. The existing
input.work debit remains literal; completed metadata is not billed again.
Production limits, prior canonical weights, DSP and persisted formats are unchanged.

## Witnesses and provenance

Before changing the production algorithm, Main and an ordinary isolated
filesystem copy captured complete OLD results independently. Equality removes
only the declared variant, source digest and published-checkpoint provenance
fields. Ordinary, false-admitted, true-admitted and selected/shared origins
remain separate actual handles. True construction can retain augmented
resolved.work, which is not replaced or normalized.

| Immutable witness | Calls | Results | Bytes | SHA-256 |
| --- | ---: | --- | ---: | --- |
| retained fade v1 | 1,882 | 1,142 Ok / 740 Error | 2,206,347 | 44aecc27d5be86e880af03ac43d8a83e5861628638d857a5501da6b1abdc4f16 |
| owning supplement v1 | 1,882 | 1,167 Ok / 715 Error | 2,129,320 | 28ca861120d0733f6835b6edd3117c5a0fbd9fd95cc69a4cca8ede2e2a39fe8b |
| native retained read v1 | 744 | 306 Ok / 438 Error | 25,746,494 | 838a7165fe4fd9142450568158359b00b05d6cd867aceaef77bfbd49e2c52f75 |
| coincident retained edges v1 | 284 | 180 Ok / 104 Error | 501,603 | e5702798bd4ccfe98b54637d0b9dffb102d4c3c22b36b4fe7214068112b399a8 |

The first logical witness has 11 checked fixtures and 44 provenance trials:
23 Some, 20 None and one rejected Some. The supplement has seven checked setups,
27 Some and one rejected Some. They cover Root, PreservePoint, DefinitionPoint
and GapDefinitionPoint clocks, stored retimes and repeats, editorial constraints,
marked fragments, definition and gap ownership.

The coincident-edge supplement was captured **after implementation**, using the
current tree's unchanged ordinary algorithm, not a pre-implementation tree.
The ordinary method and complete ordinary fade module match published
ee9742711868fde9a78bcb945adfd36e67f504d6 byte for byte. A separate pin
manifest records that comparison. A genuine checked Split and marked resulting
Partition exercise both nonempty old and new origin arrays. The earlier invalid
Repeat-mark proposal was withheld. Main and isolated capture bodies agree
after removing only variant and source digest; algorithm pins and published
checkpoint stay in the comparison.

All 21 oracle bodies retain their exact hashes. No witness was regenerated to
match admitted behavior.

## Logical and allocation evidence

False mode compares all 4,048 complete OLD observations. True mode compares all
2,489 successful OLD outputs, preserving literal stored and returned nested
work. All 54 actual Some handles receive exact-work, one-short-work and
one-byte-short requested-Layout-peak trials. Every spend/reserve refusal and
spend/reserve/merge issuer rotation checks the exact accepted callback prefix,
last callback and released leases. Cancellation sweeps use callback ordinals
because zero-unit callbacks share work totals; independent budget policies
retain cancellation before zero and after literal prepayment.

Independent inventories check the actual processing payload, concrete Box shell
and editorial allocations at copy phase brackets. Combined overlap evidence
covers 638 unique nonempty default queries. It checks simultaneous retained
input, old and replacement origin arrays, side buffers and outer collectors,
including 36 genuine both-nonempty origin merges, spare-capacity pushes and
Repeat/editorial handles. Origin merge checks sum both sides' complete owning
layouts, rather than a largest-buffer lower bound.

## Native outcomes

Five real WAV cases, four actual retained-input construction origins and both
authored modes replay the frozen 744 requests. Main and isolated NEW bodies
agree completely after removing only variant and source digest. The final
canonical body is 26,575,199 bytes, SHA-256
2f57d0404dad1cb2d33054958d0578f67dd9e2ecc154a1d17fd2824d65700745.

- 264 retain complete PCM bits, masks, dependencies and depth.
- 410 retain the complete typed error.
- 42 newly refuse: 40 injected-prior-work cases and two one-resident-frame cases.
- 28 report an earlier structural-work error under injected prior work.
- No unexpected successful payload difference or new success occurs.

The 584 ordinary-budget pairs additionally compare complete non-budget
controller/cache/source fields. Their extra accepted work equals the recorded
additional constructor units, with host eviction recorded independently.
All 744 reads check receipt consistency, finite active depth and cleanup.

Of the 70 changed tight-budget outcomes, 58 stop before source callbacks and
reconcile accepted work exactly as input plus retained and existing Original
constructor receipts. Twelve reach precisely the same source events as OLD:
four treatment reads make one callback each, and eight NTSC enclosing reads
make 13 each. Eight then report Limit("plan work per read"); four authored
NTSC reads report Plan(AudioQueryLimit("recursive policy work")).
Accepted downstream work outside these constructor receipts remains separate:
539 units for treatment reads and 1,674/1,614 for NTSC unauthored/authored reads.
This is not complete whole-read work or allocation admission.

Focused native tests cover exact local and later-owner refusal, source failure,
cancellation, foreign-plan rejection, real cache eviction in both lanes, and
warm retained cache-hit source re-admission. Forty complete successful pairs
and 64 early/source-error pairs separately check frozen OLD behavior and
actual measured fees. Metadata leases stay live at every consumer and release
on success and failure.

## Verification and limits

Host: Apple M5 Max, Mac17,7, 128 GiB RAM, macOS 26.5.2 (25F84),
arm64, Rust/Cargo 1.97.1. Native Audio uses the qualified FFmpeg 8.0.3 and
ORT 1.31.0 prefixes, optimization level 2, with debug assertions and overflow
checks retained. Plan tests use the normal optimization level 0. Native
executables run serially, with separate Main/isolated targets and immutable
executable pins. Every qualified run records source and binary identities
before and after execution.

All 29 Plan executables pass in each root: Main 723 passed / 22 ignored,
isolated 710 / 22. All 21 native Audio executables pass in each root: Main
1,010 passed / nine ignored, isolated 1,004 / nine. This includes libraries
429/423, ordinary stages 203 each, definition/history 171 each and 207 smaller
integration tests each. Fourteen passes reuse earlier results only after
proving identical final executable bytes. Plan/Core inputs are unchanged since
their final Plan builds; the later instrumentation change affects Audio tests.

Main cargo xtask check --no-test passes. The isolated copy has no .git and
therefore cannot run that Git-based package-selection command; its explicit
Plan/Audio all-target Clippy with -D warnings and formatting pass. That initial
xtask failure is retained, not counted as a pass. No native UI or release
bundle is rebuilt for this backend unit.

The fresh seven-case Split research audit remains three pass/four fail in
252.60 seconds:
- Source/Sequence refinement, cut-three, 0+511: structural query work.
- Nested selected play without root sound, prefix-two, 0+4,096: frozen projection work.
- One selected play, prefix-two, 0+4,096: plan work per read.
- Processed root sound, root-sound-refinement, 0+511: frozen projection work.

General sound-bearing Split stays disabled. These are retained research
failures, not shipping-suite passes. Hygiene passes. The final audit verifies
every receipt is terminal, every run's source/binary/stream guards, serial
execution of all 91 recorded native invocations, the 14 byte-identical native
pass reuses, all 21 oracle hashes and exact staged source identity.
All 45 shipping source/fixture blobs equal the isolated candidate. Main's
2,792 and the isolated copy's 2,464 initial unowned inputs remain exact.
Main's inherited additions in original_inputs_tests.rs remain in the working
tree; the staged file includes only this unit's child declaration.

The [evidence inventory](../../tools/media-qualification/evidence/2026-10-31-retained-fade-query/inventory.json)
contains 1,193 verified compressed records (149,013,091 stored bytes;
1,026,466,760 expanded), SHA-256
b1040fe261d8454d767788c7b25c0f066e1c6e60e0934a7b341a2b67f19f96a1.
It links the preceding Original-fade inventory by its exact hash. Raw OLD/NEW
captures, source/binary/media identities, failed attempts, independent reviews,
complete commands, accepted-work audits and the shipping source patch remain
available. Executables are excluded; every archived payload is decompressed
and hash-checked before publication.

Independent reviews found test weaknesses in callback cancellation, combined
origin overlap and a weak refusal loop. The final tests check actual caller
ledgers, combined old/new leases, exact errors, no source callbacks for local
refusals and same-provenance constructor headroom. A separate cache-lane success
asserts its literal debit. Initial failed assertions, invalid setup attempts,
strict-check failures and the first incomplete work-attribution hypothesis
remain in the evidence with no acceptance credit.

Remaining work includes raw-domain reconstruction, recursive Signal/Tape/Mix/
Source/Occurrence/Projected products, complete preflight and policy preparation,
general Graph containers, the native workspace and the full specification.
Work and Layout counts are not speed, RSS or listening measurements.
