# Single-output Components suppression ownership, 2026-10-31

The production single-output Components path now records actual suppression
decisions in four bounded inline words and returns a funded AudioSample mask.
The existing source reservation stays live through numeric callbacks, the depth
check and destination projection. This closes one producer, with no requirement
or Gate acceptance change. General sound-bearing Split remains disabled.

## Production behavior and bounded work

read_shared_program already limits each program to 1..=256 frames and checks
start+frames. Components retains that exact start and frame count. Its private
marker initializes four u64 words through real WorkControl, then debits Mark1
before setting each actual bit at the old post-finite-validation, post-PCM-write
sites. The marker moves through callbacks without borrowing StageAudio.

After the existing depth check, a billed word scan counts actual set bits m.
Zero returns an empty mask without destination storage or account initialization
from this producer. Positive output admits exactly m range slots plus the future
Storage Box while the existing source reservation and inline marks remain live.
Ordered bit-run extraction coalesces adjacent runs across word boundaries and
finishes the same allocated vector directly. It performs no generic sort,
second vector, shrink or refund. Capacity remains m even when fewer ranges emit.
The existing opaque owner then survives independently of work and renderer life.

Inline words have no heap-source allocation or stable moved-stack pointer claim.
Receipts describe their real word contents and logical scope closure. They do
not reuse the rejected prototype's heap-flag billing, identity or disposal claims.
Existing scalar residency, canonical query weights, limits, validation order,
numeric behavior and ordinary multi-output Components stay unchanged.

The earlier unpublished heap-flag prototype failed nine unchanged wet/event tests
with Limit("plan work per read"). Repeated per-frame initialization/scans and
generic sorting exceeded the actual 16,777,216-unit default budget. Direct range
allocation with the generic sort retained was ruled out by a static work lower
bound, without executing that alternative. The replacement
removes those operations instead of repricing or raising the limit. All nine
original regressions pass in both copies under the existing requests and caps.

## Before and complete results

The 34th oracle was frozen before implementation from published SharedMix base
69698b678fd8a75dd644993d7d2a9846986d6911: 35,593,716 bytes, SHA-256
9d6f3c67d9d611f46e75360006845a1b9419b30168a983db1ca9e4cf79c8be0a.
All 34 payloads remain fixed. Five historical complete bodies reproduce exactly
in both variants through explicit literal replay composition.

All six complete canonical bodies and their individual hashes are retained:

| Copy / run | Canonical bytes | SHA-256 |
| --- | ---: | --- |
| main / active | 38,529,544 | 5f2ddd4df01c0a012da87c23ab7953b3285787fddf8d2ce38850162df44b8c4e |
| main / active2 | 38,529,544 | 4656ec16ebc20f8e570b4f924dbf0cefcc66bf8627e616809965f1d8422e3a74 |
| main / active3 | 38,529,544 | 3cac845a1d7f509178a893e7383ec6418dfa734e3b1a319cf1e9573a190b995b |
| scoped / active | 38,529,544 | 7a18d4416386c617bc835bec033ee497fe06c725dde038ddc3c23eb313c8bcab |
| scoped / active2 | 38,529,544 | 66f7130b55b4b4f96aada54bc67ec6a47539f5c032a6e021aff4139f4235099a |
| scoped / active3 | 38,529,544 | 3eb16f318a76582887f66d6b1dc3232c542cbc0c9314783fb12fe9ba3ea249e8 |

Three current captures per build preserve identical classic sections across Main
and the isolated shipping copy. All 744 complete outputs, 264 successes and 480
errors, and all 42
internal/transfer output fields remain exact against the SharedMix before body,
including PCM bits, ranges, capacities, exhausted data, dependencies and errors.
The classic account fields also remain exact. Those classic cases do not reach
Components; their compatibility result is separate from the new producer proof.

The four additive producer cases retain complete old/current bodies and actual
receipts and are checked independently in every current capture. Positive
constant and dynamic outputs match literally, including
capacities. At the measured native Mark-refusal seed the active path returns
the actual Limit("plan work per read"); legacy at that same seed returns
the same typed Limit("plan work per read") error, recorded independently. Both bodies stay literal and independent because the native
producer adds real work. The two wet positive cases have actual absolute plan-work
checkpoint differences between captures at 95 reviewed paths. The audit records each path, value and
delta, permits only those named integer counter leaves in those identified cases,
and rejects every other difference. Outputs, capacities, errors, account/lifetime
data, source/mark structure and the zero/refusal cases stay exact. No full-body
byte-identity claim or normalized replacement is made.

## Actual ownership and failure proof

The checked WAV/provider constant request spans 514 frames in actual 256/256/2
programs. The dynamic request crosses the event boundary at sample 8192 in a
real 8128..8256 read and executes the dynamic driver and equalizer callbacks.
Tests identify the depth-zero request, successful actual marks, source reservation
live at destination admission, exact raw-m destination capacity and destination
pointer custody. Returned mask payloads survive controller/work disposal and
release to the separately measured retained account shell after mask drop.

A checked Source-only request reaches the actual zero-emission root. Its own
destination reservation/storage are absent, and its result is unowned with
capacity zero. A separate checked Root gain-envelope/saturation fixture has no
wet/EQ callbacks and uses only Root Original-input cache keys. It measures an
ample first-Mark prefix, seeds prior plan_work at the next request's entry,
then requires that same actual root Mark to refuse exactly at the unchanged
ceiling, before any accepted root mark or destination admission. Both observed
first-attempt records and the initial seed remain explicit. No ledger is changed
at the Mark boundary. The failed read preserves actual inline
words, reports the typed error and cleans request/source state. The pre-count
failure receipt retains raw_mark_count:null and omits result mask_capacity;
neither is interpreted as a measured zero count or capacity.

All four serialized cases reject unfinished, truncated or dropped trace rows.
The observers are test-only and default off. Source evidence records bounded
inline contents, while destination pointer identity remains private and only its
comparison serializes. Producer work is not inferred from whole-read residuals.
The 477 completed inclusive query intervals reconcile separately; the one real
resident-quota partial remains partial with its existing five-unit unclaimed
prefix residual. No missing query charge is inferred from that partial, and no
inclusive rows are summed globally.

Seven separate real-context algorithm tests cover 90 combinations of frame
boundaries 1/63/64/65/127/128/129/255/256, mark patterns and positive/negative
sample starts. They cover full-word shifts, cross-word merging, invalid bounds,
real work refusal/cancellation, zero-account emission, retained raw capacity,
held ownership and every measured small finish-work prefix. They use actual
StageAudio, WorkControl and mask admission, not fake tickets or counters. This
constructor coverage does not substitute for public driver/provider reachability.

## Verification and scope

All 21 Audio targets pass on final build13 in both copies: 1,136 Main and
1,130 isolated tests, 24 ignored manual entrypoints. Libraries pass
555/549. Main's seven known unfinished Split tests remain filtered.
Every final binary is pinned and executed. The final library bytes match the
fresh build12 library under the same complete source guard; build13 retains the
actual compiler-artifact reuse record. Main's strict check, isolated
Audio all-target Clippy with warnings denied, formatting and hygiene pass.
Audio rustdoc contains zero executable examples. Native builds retain optimized
dev level 2, debug assertions and overflow checks, incremental off, qualified
FFmpeg 8.0.3 and ORT 1.31.0 prefixes and actual artifact/source identities.

Retained failed runs include the rejected flags matrix, observer row truncation,
unqualified dynamic fixture, an early Mark-prefix reachability miss whose cause
remains unknown, strict style
findings, the absent-capacity assertion and drop_non_drop on an AtomicBool in a
test. The first attempted final matrix also retained a Scope library failure:
the wet fixture's two fresh graphs did not reach the same first-Mark prefix.
Eight fresh focused reruns and four diagnostic full-library runs passed, but
were not accepted as a fix. Pointer-hashed Domain cache comparisons are real
billed work that can vary with allocation addresses; the exact missed-prefix
delta was not captured. Only the refusal fixture was replaced with the checked
gain/saturation case above. Wet constant/dynamic/zero fixtures and the nine
original regression requests remain unchanged.

The first probe of that replacement exposed another test classification error:
Original routes also receive event_gain, and varying gain prevents a constant
program checkpoint. The actual callback-free Components driver is dynamic.
The assertion and additive case label were corrected to that observed branch;
the failed build11 probe and corrected source review remain retained.

Corrections preserve earlier evidence and do not claim that an earlier
passing run executed later assertions. The final lifetime test uses a lexical
scope so controller, work and cancellation locals leave scope before held-result
assertions. Independent production, observer, boundary, adapter and final capture
reviews are retained.

The source audit preserves Main's 2,920 and the isolated copy's 2,592 other
initial named inputs, with all inherited Main differences retained. The isolated
copy is the shipping authority. No unrelated Main WIP is published.

Evidence: [compressed inventory](../../tools/media-qualification/evidence/2026-10-31-components-mask-ownership/inventory.json).
Inventory SHA-256: d2d474a50615ca667b35bcf1ee380d800082d25c16341ff33f25e80b45eb3867.
Its 1,633 records retain 5,549,878,123 expanded bytes in 562,231,326
stored bytes. It includes immutable-before proof, failed and successful runs,
all capture bodies, independent reviews, source guards and shipping source;
pinned executables and full Main source snapshots/patch plans are excluded.

Ordinary multi-output Components, projected PreserveTape policy, exhausted
vectors, PCM/dependency storage and whole preparation remain owed. Destination
residency refusal and callback/depth/numeric failure coverage for this producer
are not established by these fixtures. Projected Tape's grid/provider semantics
remain independent of the existing Point policy API. No DP requirement or Gate
acceptance changes.
