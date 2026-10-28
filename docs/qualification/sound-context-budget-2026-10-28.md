# Admit actual current-sound context construction, 2026-10-28 (UTC)

The numerical temporal-node path now admits its actual current-sound context
before allocating its bounded chain, edge and Repeat buffers, policy/context
Arc payloads and closed processing-Domain fallback. One constructor kernel
serves the admitted path and compatibility API. The numerical caller uses
real StageAudio.make_room; there is no separate planning renderer or provider.

The request-local lease remains live across fallible Original selection,
boundary resolution and full native EQ admission. Values drop before their
leases on refusal or cancellation. A successful node publication transfers
the same metadata charge into active/Graph residency, then disarms the pending
lease. Graph values drop before their saved residency is released, including
the existing Original cache on the main working-tree variant. Exact RefCell
pointer identity checks the issuer; equal ledger contents are insufficient.

The closed fallback checks every Domain/seed field in release builds before
its specific path copies and Arc allocation. New fields force source changes
through exhaustive destructuring. Requested Vec capacities are exact under
the pinned Rust 1.97.1 Global/RawVec behavior; Arc header/payload layouts include
alignment. Owning identity edges are conservatively charged even when Arc
clones share strings. Each allocation rounds up to the existing eight-byte
ledger unit. This is a bounded logical payload claim, not allocator overhead
or measured process RSS.

The actual fused Root/Domain query/capture function remains byte-identical.
The full 2,129,928-byte native EQ bound is still admitted before construction
and retained through graph lifetime. No limit, work weight, refund, retry,
cache identity, source dependency or PCM policy changes. Completed admitted
inspections are billed once; failure does not refund performed work.

## Verification

Rust 1.97.1, locked dependencies, qualified FFmpeg/ORT, opt-level 2, debug
assertions and overflow checks. Source guards cover 1,434 main and 1,205
isolated inputs. Main and isolated use independently pinned Cargo executables.
All 13 staged source files equal the isolated final source; ten also equal
main. The main-only shared_eq.rs, stages.rs and new graph-test initializers
retain inherited Original-cache/family work outside this task. The complete
overlay audit pins 32 differing common source inputs and keeps the other 29
differences outside the proposed commit. The fused original_context equals
HEAD byte for byte.

| Check | Result |
| --- | --- |
| Final main / isolated Audio library | 292 / 273 pass, none failed, ignored or filtered. |
| Final main / isolated complete Plan | 564 / 551 pass across 29 executables each, none failed, ignored or filtered. |
| Main enabled real-WAV stages | 203 pass; seven unfinished Split cases filtered. |
| Isolated full real-WAV stages | 203 pass, none failed, ignored or filtered. |
| Main complete definition / focused historical PCM | 171 / eight pass; focused historical run filters 163 other cases. |
| Required strict gate | cargo xtask check --no-test passes formatting and strict all-target Core/Plan/Audio/Store lint. |
| Independent review | No supported correctness findings. |
| Seven unfinished Split cases | Four pass / three work-budget failures, none ignored; 392.51 seconds. |

Seven new Plan tests exercise exact/one-short reservation, Root/Repeat/Preserve
contexts, value-before-lease Drop observations, distinct/unstable issuers,
failure after sharing, cancellation, fallback shape refusal and handoff.
Six Audio tests cover real prepared-cache eviction/re-admission, real-WAV
PCM/dependency/suppression/exhaustion/depth/source-call parity, request identity,
full native admission while metadata remains pending, successful graph handoff
and cancellation before or after publication.

The first strict run rejected one useless conversion on AudioSampleGrid::new,
which already returns PlanError. Removing that conversion passes strict lint;
the separate ExactRatio TimeError conversion is retained. Final positive
library/media binaries are rebuilt from that corrected source. The failed
lint receipt and correction remain in the evidence.

The unfinished failures now report Source/Sequence cut-three limited 0+511:
Stage(Limit("plan work per read")); nested selected without root sound,
prefix-two limited 0+4096: the same read-work limit; one selected play,
prefix-two limited 0+4096: Plan(AudioQueryLimit("structural work")).
These changed subphases are retained. Processed-root, partition independence,
each-depth and audible-witness cases pass. None fails memory admission.
The new accounting does not resolve the existing preparation-work failures.

## Remaining work and evidence

This delivery covers only actual closed current-sound construction, its two
Arc wrappers and specific fallback clone. Original Root/Domain/Retained
query/capture and Core binding construction, node containers, full transient
overlap and recursive history closure remain unadmitted. General sound-bearing
Split stays disabled; cold batching is not activated. Product coverage stays
about 85%, ten Complete requirements and all seven gates open.

All 16 build/check/test receipts are terminal with unchanged source/binary
guards. Final main/isolated builds take 55.50/59.15 seconds, main
stage/definition runs 87.10/73.66 seconds and isolated stages 70.46 seconds.
These durations are verification costs, not performance acceptance. No GUI,
ABI, packaging or release qualification is repeated for this internal change.

The [inventory](../../tools/media-qualification/evidence/2026-10-28-sound-context-budget/inventory.json)
retains 396 gzip payloads: exact source/patches, the bounded allocator design
and Rust references, all 32-target pins, 16 terminal commands/outputs, Plan
per-target logs, source/index/overlay audits and independent review.
Stored/expanded hashes verify: 3,469,580 / 14,644,261 bytes. Inventory SHA-256:
f6ed4675b8e8482961ff984da162c1848a620005014db989358478131fe9d18c.

Raw task work is in /tmp/deadpan-sound-context-budget-2026-10-28.
