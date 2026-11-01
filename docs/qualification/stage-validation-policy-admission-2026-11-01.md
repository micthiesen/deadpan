# Initial stage-output validation policy admission

The initial complete-output validation loop in StageAudio::build_stage now
constructs its selected policy through the existing native admission backend.
The query retains its content inventory, final range allocation and returned
admission guards through content checks and actual progress reporting. It drops
at the iteration boundary, before saved-key scanning, DSP and subsequent child
PCM preparation. This is discard-only validation, with no mask handoff.

The real prepare_stage path reaches Signal output; prepare_projected reaches
after-Preserve Tape output. Tape remains a closed dispatch arm without a reached
output witness. The existing stream reservation is already live at query entry.
Mix input behavior, input/output ranges, content order, control checks and
canonical fees stay intact. Forwarded query work is charged once. No cap,
canonical weight, refund or public Plan API changes.

This checkpoint does not admit read_signal policies, the other free
suppress_signal callers, placed Root or Mix policy, graph/PCM/dependency
construction, provider storage or whole preparation. It grants no DP requirement,
coverage-score or product Gate credit. General sound-bearing Split remains
disabled in shipping Scope.

## Ordinary reference and actual key proof

[Baseline 41](../../crates/deadpan-audio/tests/fixtures/stage-validation-policy-baseline-v1/corpus.json)
was captured before production selection changed: 4,617,173 bytes, SHA-256
3f3d626f064bab86af1102e878d5cdd3f0c3efd55edbe4dc0e10673a9cd8cf9e.
All 41 immutable payloads remain physically fixed. M-only entry wrappers select
the old stage-validation policy for replay; raw fixture helpers remain current
and L-active. Earlier L and J behavior is not disabled to obtain parity.

Three checked families use real persisted registration and the unchanged WAV.
Twelve normal actions distinguish cold public preparation, fresh-ReadWork warm
preparation, same-work memo reuse and actual persistent restore/nested scans.
Six controls remain separate. The ordinary source records actual policy values,
outer suppression/content Vec capacities and corresponding capacity Layouts,
controller/work,
progress, callbacks, PCM and provenance. These ordinary Layouts are calculated after construction from observed Vec
capacity; they are not observed allocator requests. Actual requested Layout
tokens appear only at native Admission reservation hooks. Nested owning heap
capacities and deep nested error payloads not exposed by the API are explicitly unexposed. Volatile
age and remaining-duration values are not sampled. Cache listings are sampled
as duplicate-preserving keyed metadata, without changing rendering or eviction
order.

The literal frozen Scope corpus and current engine keys need not be equal.
The normal regression checks ordered actual hash-update bytes, exactly one
actual Cargo ENGINE_PIN update per stream, BLAKE3 finish, raw SavedStage result,
references and exact cache-listing schema/roles. It never rewrites a capture,
forces an engine pin, normalizes counters or regenerates the oracle.
Actual 640/768-frame cache roles retain 5,200/6,224-byte file associations.

The first before build/captures 1 and 2 did not guard 12 Scope engine files and
remain historical only. Fresh before captures 3/4 and their complete guards are the freeze
authority. The corrected collector follows build.rs across all extensions and
excludes only its actual skipped directories. Current guards include 700 Main
and 693 Scope engine inputs within 2,989/2,673 named inputs. Five Scope .orig
backup files are actual guarded ENGINE_PIN salts, but will NOT be published.
The guarded local input count is not a count of committed shipping inputs. Source-preservation2 verifies
all sixteen owned paths byte-equal across roots, all 41 payloads fixed, and
Main 2,974/Scope 2,646 other initial inputs unchanged. The current 2,668 nonbackup
Scope guarded inputs are the prospective curated shipping set, not an already
committed M revision. The seven other document/vendor salt files remain
recorded separately; no historical guard is backdated. Main WIP stays excluded.

## Complete paired captures

All five affected normal tests pass in each root: frozen key-aware replay,
paired actions, Plan faults, Cache faults and residency. Strict4 and optimized
build2 pass in both roots after the historical-test isolation below. The build
produced 25 actual Cargo artifacts/root, with optimization level 2, incremental
disabled and debug/overflow checks on. The physical-artifact proof verifies all
50 actual Cargo originals against their pinned executable copies. Both fresh
historical focused2 runs pass the one affected test.

Four fresh complete after captures, repeats 4 and 5 in each root, pass their source,
binary and environment guards. Each root repeats its own raw and canonical
bytes exactly; cross-root bodies remain unequal:

| Body | Bytes | SHA-256 |
|---|---:|---|
| Main canonical | 133,614,503 | 0d7b4a63e4df98dc5e812c59af5099710ad20600aeb059ab3ca37b955889b703 |
| Scope canonical | 133,613,877 | 680b5cc6953a29af2ee2e74d015c4f850a4ddc7141f68ab03e6553d71c3b178d |
| Main raw | 504,781,552 | 47f25a090b058dec43a56365e38dd988f7396652470ff69f7d029713d78adcb9 |
| Scope raw | 504,780,928 | 2a6979f523190fe89929f3217a0996b96f234f1ce625ca2e77b97a7acba2d2d1 |

The fresh build2/capture4-5 independent whole-tree comparison derives all
8,156 scalar differences under 274 nodes: 54 engine updates, 153 key arrays,
60 key-hex references and seven rolewise listings. Their scalar counts are
3,186 engine bytes, 4,896 key-array bytes, 60 key-hex values and 14 listing leaves.
Unclassified differences are zero; every other value, field, type and array
length is exact. The earlier 8,318-leaf count belongs to build1 only.
All ordered nonengine bytes and real scan/caller/build metadata match.
Each capture has 50 actual hash streams: ten finalized and 40 actual error
prefixes. Across four captures, all 40 finishes replay to the actual raw key;
160 prefixes have no invented finish. Fresh after-key-stream-replay2 independently passes all four build2 captures.
The replay uses the same locked BLAKE3 primitive through a separate caller, not
an independent cryptographic implementation. Build1/captures 2-3 remain historical
evidence rather than being relabeled current-source authority. Fresh source/body
verification is joined with the matrix and auxiliary receipts below.

Each root retains 18 ordinary/current action pairs. All twelve normal complete
results match, including PCM, storage and literal capacities. All five default
error controls match their complete errors, including late failures. Only the
separately labeled old 2,048-cap action changes from success to the actual
resident limit. Actual caller/build/validation/chunk identity, parent chains,
ranges, provider and depth join policy values; changing attempted-row position
is not used as identity.

## Native work, lifetime and refusals

Each body contains 339 trace occurrences: 36 paired, 36 Plan-fault, 41 Cache-fault
and 226 residency. These are serialized occurrences, not 339 unique requests.
All 966 selected scopes are completed, nonnested, request-one and entry-balanced:
824 admitted successes, 48 ordinary successes, 93 factory errors and one
control-check scope before factory entry. There are 111,652 native rows.
No invalid, overflow, dropped, foreign or active-at-take state is accepted.

Every admitted success satisfies canonical = actual ordinary policy work and
canonical + additional = total = view.work = accepted Admission spend.
Actual make_room host work is bracketed separately. Aggregate admitted spend is
28,312 Plan and 884 Cache units; host work is 1,616 Plan and zero Cache units.
Evictions are zero in both lanes. Positive Cache admission work is reached
through the actual persistent nested scan, without a manually seeded lane.
Positive Cache host work or eviction is not claimed.

All 8,839 successful reservations use ceil(actual Layout bytes / 8) frames.
Actual ledger changes, 5,286 releases and 3,553 merges reconcile. All retained
views retain their local identity and full values through actual content and
progress uses. There are 823 explicit OwnerDropped marks among admitted
successes and 48 among ordinary successes; the separate
ContentBefore cancellation unwinds without a fabricated after-drop marker,
and real releases/scope balance prove cleanup. The maximum selected-scope
request-ledger peak is 334 frames. This is neither total controller residency,
physical memory nor per-lease allocation-origin attribution.

The selected PreserveTape/Plan query has 31 real refusal targets, 24 spend and
seven reserve; Signal/Cache has 38, 28 spend and ten reserve. Fresh requests
refuse every observed callback ordinal, retaining the full accepted prefix and
allowing no later acceptance. Refusals are explicitly injected protocol results.
Four callback cancellations cover the first spend and reserve in each lane;
two actual use-seam cancellations occur with the owner live. They set the real
cancellation token and stop at existing checks. Failed local canonical/additional
splits remain unexposed, not reconstructed from residual work.

Twenty Fault::None constructor make_room failures are observed: one old 2,048-cap
action and nineteen resident-search attempts. These natural limits are distinct
from injected refusal and close a constructor-pressure witness absent from L's
global refusal sweeps. No zero-byte Layout reserve is reached.

## Real resident boundaries

Fixed real warm states yield these public ceilings:

| State | Success | One below |
|---|---:|---:|
| Plain | 2,345 | 2,344 |
| Matched setup, holder dropped | 2,348 | 2,347 |
| Matched setup, separate J holder live | 2,355 | 2,354 |

The binding plain sum is 2,048 active + 105 sound-context + 192 pending.
Matched dropped adds the real three-frame owned-suppression shell; held adds
the real ten-frame J output owner instead. At 2,354, held fails with the actual
resident PCM or stage cache limit. Dropping that separate output changes its
account from ten to three; the same controller and unchanged 2,354 ceiling then
succeed. Every public call uses fresh ReadWork. No M final mask or artificial
parent-M-query overlap is implied. Successful boundary requests have selected
ledger peak 297; one-below requests peak 137 when the 192-frame reserve is
refused. These ledger peaks differ from the complete public resident sums.

## Preserved failures and final validation

The evidence retains the before strict1 E0277 conversion failure, strict2
argument-count lints, observer/reporting corrections, engine-input guard gap,
and the exact-schema key-comparator finding. After review also corrected late
error fallthrough, caller/build identity and the digest LowerHex pattern.
Active strict2 found an unused Spend-cancellation variant and a collapsible-if;
the fix adds real Spend and Reserve cancellation in both lanes, without an
allowance. Strict3 and subsequent strict4 pass. The first Main complete writer failed its
/private/tmp assertion before any scenario/body; only the scratch launcher
changed to literal /tmp, with unchanged Rust and binary. Failed receipts remain
failed and retained.

Matrix1 completed all 42 target runs. Only the old internal-mask B-lineage
held private Point/genuine Bound boundary test failed in both libraries; all
other 40 target runs passed. Its unchanged shell-to-512 search now reaches
earlier M inventory pressure through genuine Bound preparation. Quiet Hold does
not build a stage. Root isolated this historical entry with only a six-line
M-only selector wrapper, preserving the entire old helper/body and literal
512 ceiling. C Point policy, raw Bound, internal mask ownership and L/J remain
active; other current Bound tests remain active. This is not a Plan Tape I
failure or a production cap increase. The source changes from SHA prefix
928769b9 to 70ec15a6, with exact old-body/rest proof retained.

Independent complete-capture and accounting reviews give bounded GO for the
retained build1 evidence. Fresh build2 accounting follow-up independently gives
GO with zero inconsistencies and unchanged counts, boundaries and work totals.
These are bounded technical verdicts, not product Gate acceptance.

## Completed validation and evidence packaging

Matrix2 passes every one of 21 Audio executable targets/root, all 42 target
runs, using actual build2 binaries. Scope passes 1,151 with 37 ignored and no
filters, including 570 library tests. Main passes 1,157 with 37 ignored, including
576 library tests; only its seven inherited general_split_sound filters apply
to stages, which passes 203. No new test is waived. Both doc2 runs pass with
zero doctests; fmt2/hygiene2 pass and all execution guards remain exact.
Strict4/build2, all 50 actual Cargo-original/copy joins, the affected historical
focused2 tests, fresh complete captures 4/5 and literal hash replay2 are retained
with their real commands, source and binary receipts. Independent final2
execution/source/capture joins and the fresh accounting follow-up give GO within
their stated lenses.

The [archive inventory](../../tools/media-qualification/evidence/2026-11-01-stage-validation-policy-admission/inventory.json)
records curated retained source, receipts and proofs, plus exact raw capture aliases.
Packaging uses deterministic content-addressed gzip, with hashing, compression
and expanded readback in at most 1 MiB chunks. Exact repeat bodies share storage;
unequal cross-root bodies remain distinct. No native executables are included.
The v2 builder closes metadata TOCTOU gaps; eight small regression cases retain
the old acceptance of three changed-metadata cases and the new rejection of all
three, alongside static review. These are packaging tests, not render evidence.

Before publication, Root verifies the actual archive's stored/expanded hashes,
reference/source/acceptance joins and mirror, then curates Scope-origin changes
without Main WIP or the five local backup salts. The linked inventory is the
durable archive authority; no self-referential inventory hash is embedded in
these archived documents. This document does not assign a future commit SHA or
claim that unpublished local salts are shipping source.

The full goal remains active. Read_signal, other free suppression, placed Root,
Mix own policy, graph/PCM/dependencies and whole preparation remain later work.
