# Requirements and delivery gates

All DP-01 through DP-24 requirements in [specification Section 29](spec/DEADPAN_SPEC.md#29-requirements-traceability) remain in scope. Their detailed sections are normative. This tracker records the current implementation and measured evidence, not a reduced release scope.

## Progress reassessment, 2026-10-25

[The requirements audit](PROGRESS_ASSESSMENT.md) replaces the inherited 89%
estimate with about **83% assessed acceptance coverage** from 162 explicit
criteria, equally weighted within each of the 24 requirements. The old number
first appeared October 8 and persisted through 137 observations, while strict
completion rose from four requirements to eight. It concealed substantial
remaining integration and verification effort. No product scope is removed.

A later source audit reopened DP-22's helper launch trust. Native-worker and
private-AI admission are now qualified below, restoring DP-22. The unchanged
162-criterion ledger now rounds to **85% acceptance coverage**.
Strict completion is ten Complete/fourteen Partial after the DP-08 and DP-12 reconciliations below; all seven delivery
gates remain open, and picture effects remain five of thirteen. The percentage
is neither remaining engineering effort nor release readiness. Historical
estimates stay intact; the dashboard removes their misleading daily rate and
shows the scored requirement ledger and gates. Future percentages are computed
from criterion evidence rather than manually carried forward. Earlier 89%
statements below describe their historical reporting state.

## Parallel analysis milestone, 2026-10-26

The [native range integration](qualification/incremental-ranges-2026-10-26.md)
runs real Whisper/Silero over bounded cursor-priority source ranges, retains
exact partial coverage, and resumes from a private BLAKE3-addressed cache.
Native shots now use the same scheduler/cache boundary with exact picture
range decoding and 50-picture halos. Only complete results enter Workspace
analysis. The sequential CLI shot scanner and whole-input CLI transcription
path remain available.

Eight actual-codec/pure CLI range tests, four private-cache tests, six app shot
regressions, four app partial-rail tests, Media range parity and strict
eight-crate all-target lint pass. The final native offscreen Metal replay passes
70 transcript assertions plus the live Kestrel audit, including keyboard jumps,
focus across incoming words, compact paint and correction-error authority.
The separate real-worker witness proves tail-first cancellation, retargeting,
resume, exact transcript/activity cache equality, zero-inference warm reuse and
unchanged authored revision. Original failures remain in hashed evidence.
Initial opening still decodes all audio; four cached ranges take 9,100 ms,
without a performance acceptance claim.

Broader service scheduling, model-slot coexistence under competing real jobs,
quality, long-media and package qualification remain. DP-10.7 already scores
0.5; this milestone updates its evidence without increasing coverage or closing
a requirement/gate. The useful ownership, build freezes, copied executable
receipts and acceptance joins are preserved in [Parallel work](PARALLEL_WORK.md),
linked from the mandatory handoff, for continuation after compaction.

## General Split sound transport in progress, 2026-10-26

The [current constant-block execution](qualification/constant-sound-blocks-2026-10-26.md)
removes complete unity-transfer
passes and retrieves constant gain checkpoints once per block. It keeps exact
per-sample contribution sums, EQ clocks/permissions, nonlinear and wet/send
boundaries, finite checks, dynamic fallback and all existing limits. Six focused
block tests and all 193 current Audio library tests pass; source-only independent
review finds no issue. The same six-file optimization on a clean copy of HEAD
passes all 179 independent Audio library and 201 existing stage tests. Final
formatting and strict Core/Plan/Audio/Store all-target lint pass.

The copied, hash-pinned full real-WAV Split matrix now has **four passes and
three work-limit failures**. Processed root-sound refinement newly passes
complete authored, faded and limited PCM. All-owner refinement still fails at
`cut-three`, limited `0+511`; selected and nested selected plays still fail at
`prefix-two`, limited `0+4096`. These remain incomplete comparisons. No
requirement, criterion or delivery gate closes from this optimization.

The later [owned-prefix PCM experiment](qualification/rejected-prefix-reuse-2026-10-26.md)
is rejected and removed: 214 Audio library tests and strict lint pass, but its
full real-WAV matrix regresses to three passes/four work-limit failures,
including processed root-sound refinement. Exact hash comparison restores all
1,018 preceding qualification inputs. The four-pass/three-failure constant-block
version above remains current; no acceptance credit changes.

The [query groundwork](qualification/audio-query-groundwork-2026-10-26.md)
adds borrowed routed readers and bounded retained-heap accounting without a
cache or increased limit. Core/Plan/Audio libraries pass 795/95/194 tests, and
all 201 existing stage tests pass with the seven unfinished Split tests filtered.
Strict four-crate lint passes; the scoped overlay compiles without the inherited
Split work. This does not resolve a Split failure or change acceptance credit.

The [call-local discovery memo](qualification/rejected-discovery-reuse-2026-10-27.md)
is rejected and removed. Eight focused tests, 202 Audio library tests and strict
lint pass, but the full matrix remains four pass/three work-limit failures. A
nested Tail Sends read can re-enable reuse during its residency retry. All 1,387
preceding source inputs are restored exactly and restored strict passes. No
acceptance credit changes.

The [exact occurrence-support query](qualification/occurrence-support-discovery-2026-10-27.md)
removes ordinary voice graphs from clock discovery, sharing the full constructor's
scalar bounds and retaining full Preserve fallback. Current Plan/Audio pass
526/195 tests; the independent 11-file patch passes 513 Plan, 181 Audio library
and all 201 existing stages. Final strict lint and six scoped oracle tests pass.
The full Split matrix remains four pass/three work-limit failures at the same
points. No acceptance credit changes.

The [historical binding optimization](qualification/sound-binding-reuse-2026-10-27.md)
reuses admission for exact shared immutable layouts and an already validated
after-birth binding, retaining asset, recipe, plan and complete-journal authority.
It removes actual traversal and charges new identity checks. Current Plan/Audio
library/definition suites pass 531/195/171 tests; the isolated five-file patch
passes 518 Plan and 181 Audio library tests. Strict lint passes. The unchanged
real-WAV Split matrix remains four pass/three work-limit failures; selected and
nested selected reads reach later checks. Two inherited Split-test failures are
corrected without widening authority. No acceptance credit changes.

The [post-treatment gate milestone](qualification/post-treatment-gates-2026-10-27.md)
removes an unused second query for ordinary events and fixes a reproduced
processed-root double fade through retained routes. A rejected coincidence
design is retained with its failing full-PCM invariant. Final Plan/Audio library
checks pass 534/198 tests; eight event-return PCM tests, root/Hold waveform and
strict lint pass. The isolated nine-file patch passes 521 Plan, 184 Audio library
and the same eight PCM tests. The preceding candidate passes all 171 definition
and 202 then-existing stage tests; these full suites were not repeated after the
small policy correction. The final unchanged Split matrix remains four pass and
three work-limit failures. No requirement, criterion or gate closes.

The [joined placement/context query](qualification/placement-context-2026-10-27.md)
reuses an exact checked scalar clock already computed by placement, charging
extraction and any required fallback projection. It adds no cache or limit.
Current Core/Plan/Audio library/definition checks pass 800/534/198/171 tests and
all 203 existing stages pass. The isolated four-file patch passes 775 Core,
521 Plan, 184 Audio library and eight historical processing tests. Final focused
tests and strict lint pass after a mechanical test-only cleanup. The unchanged
Split matrix remains four pass/three work-limit failures. No acceptance credit
changes; the full product goal remains active.

The [sealed placement-history milestone](qualification/placement-history-2026-10-27.md)
reuses checked scalar placements while preserving per-step cancellation/work
limits and complete journal, binding, context and route validation. Exact borrowed
inputs skip repeated field comparisons; equivalent handles retain full checks.
Final Plan/Audio library/definition pass 540/199/171 tests and all 203 existing
stages pass. The isolated ten-file patch passes 527 Plan, 185 Audio library and
eight processing tests. Strict lint passes. The unchanged Split matrix remains
four pass/three work-limit failures. No acceptance credit changes.

The [copied work attribution](qualification/sound-work-attribution-2026-10-27.md)
reproduces those failures with fully reconciled canonical charges. All-owner
spends 38.53% scanning/shifting the existing Original-input cache's sorted
index. The selected/nested cases spend 27.20%/21.04% on real transfer work.
The [resulting index experiment](qualification/sound-input-index-2026-10-27.md)
passes 202 Audio library tests, all 171 definition tests, strict lint and source
review. It remains uncommitted. All seven unchanged Split tests still total
four pass/three work failures; no acceptance credit changes.

The [shared Components outputs primitive](qualification/component-outputs-2026-10-27.md)
executes one common traversal into independently selected borrowed destinations.
Six new scalar-oracle tests, 208 Audio library tests, all 171 definition tests,
all 203 prior real-WAV stages, strict lint and independent review pass. Its isolated
six-file patch passes 191 Audio and eight historical processing PCM tests.
Runtime graph reads still request one output; recursive batching needs a cold
resource proof. The full Split matrix remains four pass/three work-limit failures,
at the same revisions/windows. No requirement or gate closes.

The [wet processor lifetime correction](qualification/wet-residency-2026-10-27.md)
keeps full constructor admission and the whole native bound, releasing only
dropped Rust constructor storage from the retained ledger. Main Audio/definition
pass 213/171 tests, all 203 prior real-WAV stages pass, and the isolated three-file
patch passes 196 Audio and eight historical PCM tests. Strict lint and independent
review pass. The test-only late-cleanup assertion failure and retired earlier
binary run are retained. Split remains four pass/three work failures at the same
revisions/windows. Runtime batching stays disabled; no acceptance credit changes.

The [Components structural helper extraction](qualification/component-policy-2026-10-27.md)
shares permission, clock, ordering and Tail decisions while retaining both
numerical traversal orders. Four new bridge/literal tests, 217 Audio library,
171 definition, 29 real-WAV wet/gain, strict lint and independent review pass;
the isolated patch passes 200 Audio and eight historical PCM tests. Full stages
and the Split matrix are not rerun for this extraction. A production policy
visitor, cold resource fold and runtime batching remain unimplemented. No credit
changes.

The [bounded intrinsic-processing query](qualification/intrinsic-capability-2026-10-27.md)
scans live and all owned historical recipes without compilation/allocation.
Complete negative, conservative positive and incomplete work-limit results all
retain spent work for future fallback billing. Main/isolated combined Plan
suites pass 552/539 tests across 29 pinned targets each, including library counts
102/89 and eleven new integration tests each. Strict lint/review pass. Setup
failures and an early combined-versus-library count correction are retained.
Audio does not call the query yet; a policy visitor, cold resource fold and
batching remain owed. No requirement or gate closes.

The [shared audio policy visits](qualification/audio-policy-visits-2026-10-27.md)
add an admitted permission carrier, production EQ/wet phase visitors and a
borrowed root final-policy adapter. Fourteen new tests preserve literal masks,
PCM bits, callback/error order, allocation and partial work. Final main
Audio/definition pass 231/171 tests; all 203 enabled real-WAV stages pass with
seven unfinished Split cases filtered. The isolated published runtime passes
214 Audio and eight historical PCM tests; all ten source files match it exactly.
Strict lint and independent review pass. A test-only lint correction and recorder
path correction are retained. Complete cold population, whole demand traversal,
resource admission and batching remain owed. The historical Split result of four
passes and three work-limit failures is not rerun; no criterion or gate closes.

The [current-chunk permission population](qualification/permission-population-2026-10-27.md)
now runs before contribution PCM allocation, with sealed Original query reuse
and independent complete-row comparison against successful reads. Unselected
rows avoid unnecessary metadata work while retaining actual source admission.
Twenty-three published tests and two inherited-cache tests cover policy parity,
identity, partial work, omitted observations, injected mismatches and residency.
Final main Audio/definition pass 256/171; main and isolated full enabled stages
each pass 203. Isolated Audio passes 237 plus eight historical PCM cases.
Strict lint and independent review pass; all nineteen published source files
match the isolated build and staged bytes. The fresh seven-case Split check has
four passes and three work-limit failures across two final-source invocations.
An initial additional processed-root regression is fixed by avoiding actual
unnecessary planning, without refunds or changed limits. Whole demand sequencing,
cold resource admission and batching remain open. No criterion or gate closes.

The [shared owner-phase demand driver](qualification/cold-demand-2026-10-28.md)
now executes both numerical orders and an authenticated cold metadata consumer
through the same owner body. Completion is explicitly requested after real
population; scalar reads avoid unused sealing. Seventeen new tests cover literal
EQ/wet order, actual Root/Beat population, inactive dynamic routes, borrowed
clocks, partial work and error prefixes. Main Audio/definition pass 273/171;
isolated Audio passes 254, both stage suites pass 203, and eight isolated
historical PCM cases pass. All sixteen published source files match isolated
and staged bytes. Strict lint/review pass. The final Split check stays four
pass/three work failures; a new scalar-sealing cost regression is fixed by
removing that unused operation, preserving requested cold validation/charges.
Exact raw-context ancestry, recursive heap/native admission and batching remain
open. Coverage and gate/requirement completion do not change.

The [native EQ admission fix](qualification/eq-admission-2026-10-28.md) now
admits and retains the complete published 2,129,928-byte filter bound in both
numerical graph builders. Four real graph checks fail the previous builders
and pass the fix, covering exact limits, reuse, taken filters and cleanup.
Final main/isolated Audio pass 277/258; both stage suites pass 203, main
definition passes 171 and eight isolated historical PCM cases pass. Strict
lint and independent review pass. All six staged source files equal the
isolated build; inherited main overlays remain separate. The fresh seven-case
Split run stays four pass/three planning-work failures, with the actual changed
subphases retained. Cold recursive ancestry/resource admission and batching
remain open; coverage and requirement/gate completion are unchanged.

The [numerical node-description extraction](qualification/node-description-2026-10-28.md)
now supplies the actual contexts, boundary, stop and kind to shared node
construction, preserving metadata statements and cache/native guard order.
Five new tests pass; main/isolated Audio pass 282/263, both stage suites pass
203, definition passes 171 and eight historical PCM cases pass. Strict lint
and independent review pass. A corrected test records 19 billed versus 28
required headroom, exposing an existing missing work receipt on retained
lookup None. Charging that probe, metadata constructor peak admission and
recursive batching remain owed. The preceding four-pass/three-work-fail Split
result is not rerun; coverage and requirement/gate completion are unchanged.

The following paragraphs retain the earlier preparation checkpoints and their
original three-pass/four-fail results.

The owner resumed the product goal after the build-tooling diversion. The
[fresh preparation checkpoint](qualification/sound-preparation-resume-2026-10-26.md)
retests the newer constant-transfer/family-interning candidate and adds a bounded
immutable per-timing asset inventory. Five sound-clock regressions, 514 Plan
tests, 190 Audio library and 201 existing stage tests pass, with strict
Core/Plan/Audio all-target lint. The final hash-pinned real-WAV matrix still has
**three passes and four preparation-work failures**. A current-source trace
attributes 5,980,514 units to constant transfer applications and 1,282,906 to
source-occurrence construction at the selected baseline's unchanged work limit.
The inventory removes repeated asset-set construction but does not resolve the
remaining failures. Original fixture, diagnostic setup and lint failures are
retained; tracing is byte-restored. No progress credit is added.

The [frozen research checkpoint](qualification/general-split-sound-research-2026-10-26.md)
retains the earlier candidate, 32 terminal runs and exact filter input. The
[delivery continuation](qualification/general-split-sound-delivery-2026-10-26.md)
records the subsequent uncommitted fix: a separate, bounded current-delivery
forest proof joins already prepared sound fragments without widening their
processing clocks, support, source admission or silence allowances. Full context
correspondence, exact adjacency, copy provenance and concrete Repeat paths are
checked; lineage alone supplies no authority.

The broad Core/Plan candidate passes **2,127 tests**, 98 nonempty suites, zero ignored.
The six processing-depth variants now pass exact real-WAV authored/faded PCM,
including root EQ and wet returns. The current seven-test real-WAV matrix has
**three passes and four failures**, all four at preparation work limits; it does
not establish all-owner or limiter preservation. Avoiding scope retrieval for
empty occurrence batches removes measured redundant work but does not resolve
those failures. Constant gains are now prepared once per route, with four focused
tests passing and dynamic automation retained; the matrix still has the same
four failures. Final Plan checks pass **511 tests** and strict Core/Plan/Audio
lint passes. Later traces place selected/nested failures before Split. The
candidate now reuses exact Original inputs within a shared processing request,
with source re-admission and eviction under the existing memory/entry ceilings.
Six focused tests and independent review pass; the selected-play full-WAV rerun
still exhausts work before Split. This optimization receives no completion
credit. Further measured preparation work remains. The per-read limit stays **16,777,216**;
clock and delivery proof compilation share the existing **100,000** work ceiling.

Native scoped Split stays blocked. Remaining preparation work, complete delivery
coverage, Store/history, native receipts/macros, emitted media and performance
remain owed. The production candidate remains uncommitted. This work block
awards no requirement, criterion or gate completion credit.

## Scoped target framing qualified, 2026-10-25

[Scoped saved-target ranges](qualification/scoped-target-sampling-2026-10-25.md)
complete the remaining DP-08 composition: explicit nested This-play Smash samples
root Edit In; Creep samples Out minus one frame, retaining the exact provider,
captured/inner geometry and a fixed center even for a full-owner Time range.
Camera proposal acceptance and named range recreation compose without recording
sampled targets. Scoped navigation now admits exclusive Out. Native forward and
backward endpoint selection, stale/refusal/Undo behavior, strict checks, independent
review, release visual/quiet replays and complete emitted-media/PCM/history proof
pass. Failed test/oracle attempts remain retained.

DP-08 is Complete under the primary-Mac verification scope. Its eight coarse
criteria already scored 1, so the fixed 162-criterion ledger stays **84.5238%,
rounded to 85%**. Strict completion increases to **10/24**; all seven gates stay
open and effects stay five of thirteen. General scoped Split, structural/audio
composition, full effects and performance remain under their existing partial
requirements, without changing criterion identities or reducing scope.

## Existing-path abrupt return qualified, 2026-10-25

[Chosen-boundary return](qualification/abrupt-return-2026-10-25.md) verifies
Split, Enter the right fragment and Camera Reset on the copied original owner
for retained Envelope and live animated Follow paths. Earlier pictures and
clocks remain exact; ancestor framing, complete canonical audio, marks,
Undo/Redo and portable history are preserved. The native replay uncovered a
real compact-viewer crash from long target labels; bounded ink-aware elision
and safe fractional positioning fix it. Final checks and release replays are
recorded in the qualification, including the original failures.

DP-08.2 changes from 0.5 to 1. The fixed ledger now computes 84.2634%,
still about 84%; nine requirements are Complete and all seven gates remain
open. Section 7.4 permits the remaining direct ranged-path refusal. Targeted
escalation transforms and broader qualification remain open.

## Existing Follow parameters qualified, 2026-10-25

[Follow retuning](qualification/follow-retune-2026-10-25.md) adds independent
arrival-scale and curve editing through Camera and semantic `:creep`, preserving
the exact target, fallback, clock and existing ramp window. Native field/IME
and pointer focus fixes keep edits temporary until Apply; compact controls fit
960×640. Final 60/120 Hz visual replays each pass 415 zoom checks plus the live
Kestrel audit. Two actual release exports verify 240 pictures, complete
384,384 stereo sample-frame PCM/limiter equality, zero-offset audio and exact
portable history. Original check, replay and driver failures remain explicit.
Independent reviews and the evidence audit cover the scoped completion.

DP-08.3 changes from 0.5 to 1. The unchanged 162-criterion ledger increases
from 83.7426% to 84.0030%, still about 84%. DP-08 remains Partial; strict
completion stays nine of twenty-four and every delivery gate stays open.

## AI Hold strict completion reconciled, 2026-10-25

[The DP-12 closure audit](qualification/ai-hold-reconciliation-2026-10-25.md)
confirms existing §12 behavior and retained real-model, native, packaging,
acceptance, history and offline-export evidence. DP-12 is Complete under §29.1.
Its eight criteria already scored 1, so acceptance coverage stays 83.7426%
(about 84%); strict completion becomes nine of twenty-four. No delivery gate
closes. Resident model/text caching and candidate/performance comparisons stay
open under DP-24 and Gates A/E/G. Human quality/listening and physical trials
stay on the owner list. Both latest rejected interior candidates remain failures.

## Private AI launch admission qualified, 2026-10-25

[Private AI launch trust](qualification/private-ai-launch-trust-2026-10-25.md)
binds the complete Python/imported-code/native/data tree to a compiled canonical
inventory before both Check and Inference. Local/ad hoc bundles share the same
inventory identity; disabled-AI builds and stale-host publication refusal are
exercised. The real negative suite passes 150 operations with 61 expected
refusals and no timeouts. Real Check passes in both hosts. The established
edge-extension fixture reaches Ready, is explicitly accepted, renders and
verifies with an empty HOME and IP denied, and Undo/full history checks pass.

The original broad 1,009-pass/one-failure run and test-driver/quality failures
remain explicit. Strict lint and 192 selected tests pass; the thirteen-test
serial warning follow-up passes. Two interior-Hold candidates remain rejected
by existing quality thresholds; they are not accepted or described as passes.

DP-22.3 advances from 0.5 to 1 and DP-22 returns to Complete. At that checkpoint coverage
was 83.7426%, rounded to about 84%, eight requirements complete and no closed
gate. The denominator, product scope and owner-only verification rules remain.

## Native helper admission qualified, 2026-10-25

[Native launch trust](qualification/native-launch-trust-2026-10-25.md) now binds
the three older workers and their six FFmpeg libraries to compiled image pins,
checks local/ad hoc signer identity and prevents damaged-package fallback.
The original 961-pass/one-failure check remains explicit; the 203-test lifecycle
and final 147-test continuations pass with strict lint. Local-certificate and
ad hoc bundle builds pass, as do actual packaged transcription/Vision work,
169 recorded operations with 34 expected refusals, and general relocation,
render/runtime and damaged-helper verification. Authored document bytes remain
unchanged.

At that checkpoint a further audit found no compiled inventory or app-seal
admission before private AI Python. DP-22.3 stayed 0.5 and DP-22 Partial, with
about 83% coverage and seven complete requirements. The subsequent private-AI
qualification above closes that same acceptance scope.

## Word timing proposals qualified, 2026-10-25

[Local English word refinement](qualification/word-refinement-2026-10-25.md)
now passes real native inference, guarded pure/host/store acceptance, CLI and
packaged acceptance/Undo, actual offline installation, final native review
(29 checks), real device audition through exact completion/restart (16 checks),
and re-signed substitution/marker/pin failures. The broad eight-package check
retains its three failures and bounded continuations explicitly; later language
admission and clean signing pass targeted checks. The stricter model quality
experiment remains failed and human phonetic judgment stays on the owner list.

DP-10.3 receives implementation/local-verification credit. DP-10 remains Partial
for its other criteria. At that checkpoint DP-22's private-runtime trust was
still open, with about 83% coverage and seven strictly complete requirements.
The later qualification above restores DP-22. Coverage does not estimate time.

## Original analysis windows, 2026-10-25

[Exact analysis-window preparation](qualification/analysis-windows-2026-10-25.md)
retains the full Original analysis origin and separate exact sample offset,
reuses the canonical global-phase resampler and adopts the verified source
snapshot. Invalid or excessive windows refuse before source access. A retained
session reuses its audio cache; initial cache opening still decodes the complete
stream and is not described as selected-range decoding. Debug snapshot hashing
now observes cancellation and the deadline between chunks.

Four actual CLI transcription tests pass over 44.1/48 kHz, zero/positive/negative
origins, fractional and terminal windows, preflight failures and retained-cache
reuse. The new adoption interruption regression, strict CLI/Media/Store
all-target checks and formatting pass. The original invalid-fixture and lint
failures remain explicit in the 182-file evidence collection. This is a tested
foundation. At that checkpoint DP-10.3 was unimplemented; the real worker,
model proposals and explicit native/headless acceptance are now qualified above. Separate model qualification records timing improvements and
failed ambiguity/precision experiments without calling them passed. Coverage
remains about 83%; no requirement or gate closes.

## Framing ranges inside Repeat plays, 2026-10-25

[Scoped framing ranges](qualification/scoped-framing-ranges-2026-10-25.md)
now retain an Edit Time range inside one explicit Play at each enclosing
Repeat. A bounded decoder-free picture-owner query proves exact structural
allocation and maps integer root boundaries into owner time through nested
Repeats and fixed Retimes. Source, Hold, Sequence, Repeat and Retime owners
support portable Keep/OriginalCenter ranges; fixed Follow keeps its live target
and distinct prefix/terminal scales. Named/dot replay resolves fresh ranges.
Unsupported paths, leaves and sampled-target replacements refuse atomically.
Slice placement now has matching fresh/delayed/pointer refusal in any active
Repeat/Retime inspector, including without a range.

Final Core/Plan passes 2,088 tests, strict checks and formatting pass, and final
release scoped replays pass 296 checks plus Kestrel at 60/120 Hz. Original-moment
regression passes nineteen plus Kestrel. Actual native-project media verifies
all 240 pictures, complete 384,384 stereo sample-frame PCM/limiter equality,
zero-offset encoded audio and exact portable history. Original integrated and
full-App failures remain recorded alongside the bounded corrected lanes;
neither is relabeled as a new all-green full run. The 607-file immutable
collection and separate publication proofs pass independent byte/hash/source,
release and media audit; original failed audits remain explicit. DP-08 remains Partial;
criterion DP-08.3 records this narrower completion while existing creep
parameter adjustment remains open. Assessed coverage stays about 83%.

## Camera pointer targets, 2026-10-25

[Pointer target authoring](qualification/pointer-targets-2026-10-25.md) adds
rectangle drawing, body movement, corner resizing and point placement/correction
to Camera's existing temporary target editor. Exact displayed Original geometry
governs input; keyboard/focus/raster changes revoke held capture. Release stays
unsaved and Enter keeps the existing revision-bound target save. The small-body
regression fails on the old hit rule and passes with the central-body fix.

The source-fourth full App lane passes 1,339 tests, with eleven configured skips
and two passing leak warnings that clear in a serial recheck. Final source-seventh
passes 61 focused tests, strict Clippy and formatting. Final release targets passes
72 checks plus Kestrel at 60 Hz visual and 120 Hz performance replay; Camera passes
seventeen plus Kestrel. Root inspects release captures. Independent durable audits
reconstruct both complete target projects and compare all 46 database/close-backup
tables and schemas. The corrected immutable collection retains seven source phases,
original failures, both projects and the first manifest's metadata correction.
Its independent byte audit verifies all 139 files, aliases and live guards.
No new export, physical display, tracking-quality or speech claim. DP-08 remains
Partial; coverage 89%, eight Complete/sixteen Partial, effects five of thirteen.
The exact physical pointer check is on the owner list below.

## Earlier ranged live Follow creep, 2026-10-25

[Ranged Follow creep](qualification/ranged-follow-creep-2026-10-25.md) now
retains the live target on ordinary Source, Hold, Sequence, Repeat and Retime
owners over contained Edit ranges. Prefix scales remain unchanged; explicit
`from=` begins at In and the end scale holds after Out. Exact clocks, separate
fallback scale, semantic replay/dot, Camera preservation and atomic history
remain shared with whole-owner framing. Repeat play ranges and path replacement
retain explicit refusals. Core 58 / SQLite 87 remain current.

All 1,584 Core tests and 488 Plan tests pass. The integrated lane retains its
original one invalid-fixture failure among 1,830 tests; the corrected Hold case
and both leak-warning cases pass a clean serial recheck. Final strict checks and
five focused App tests pass. Release zoom passes 296 checks plus Kestrel; the
final optimized-dev replay repeats those checks with inspector-row, unelided
paint assertions. Root inspects the release and final native captures.

A real release Source export passes 120 picture/independent-pose checks and
complete 192,192 stereo sample-frame PCM/limiter equality with the pre-creep baseline.
The emitted file has 228 observable/ten unobservable neighbor comparisons,
four signal/396 silent audio blocks and three zero-offset witnesses. Independent
audit verifies exact portable authored/history tables and executable identities.
The qualification retains the native selection assertion failure, dependency
path typo and other exact scope limits. The [immutable evidence inventory](../tools/media-qualification/evidence/2026-10-25-ranged-follow-creep/files.json)
retains 274 logical inputs in 141 files, with metadata-inclusive caps and a
passing independent [byte audit](../tools/media-qualification/evidence/2026-10-25-ranged-follow-creep-audit.json).
Implementation `3e069f99a48c9af99a2e2c3096ce705a3e251318` is pushed to main
and recorded as progress milestone150; DP-08 stays Partial,
coverage 89%, eight Complete/sixteen Partial,
effects five of thirteen. Remaining target controls and framing scopes stay open.

## Earlier Camera salient-region proposals, 2026-10-25

[Camera regions](qualification/camera-regions-2026-10-25.md) implements shared
Faces/SalientRegions/Both requests over one exact-picture preparation/decode
and one BGRA handler. Ephemeral protocol2 pins saliency revision2 within the
unchanged macOS15 minimum; saved targets/Core/SQLite remain unchanged. Region
boxes carry no invented confidence/rank. Camera accepts only explicit proposal
selection; region:N saves one target+framing atomically, face:N stays Faces-only.
Both completes fully or fails while saved/fixed targets remain usable.

Current fifty-nine-source release passes regions110/faces69 plus Kestrel,
inspected release proposal labels and separate quiet regions109. Complete
actual native-project media verifies120 picture fidelity/provider-metadata
checks (four observable/234 unobservable adjacent index comparisons) and full192192 stereo samples,
401 unshifted blocks, one unique-zero witness/seven honestly periodic segments
and all eight selected native/portable history tables. Stage performance limits
include intentional600ms guard delays; no overall or physical latency claim.

The original official full107 run remains exit1 with94 PASS/seven FAIL/six
fixture SKIP. Pinned continuations establish101 runnable latest passes across
the original/source58/source59 identities, not one all-green final invocation.
Harness repairs preserve production behavior and stronger actual capture,
cancellation, storage-fault, owner-reply and paint assertions. Earlier compile,
fixture, disk/setup, replay, periodic-driver and leak-warning evidence remains
explicit in the qualification; overlapping counts are not added together.

The normal --without-ai-runtime bundle build and copied scrubbed worker
relocation check pass, including signatures/doctor/helpers, six real section-
START proposal cases and unchanged SQLite/project/bundle/source/reference
bytes. Official full bundle-verify is skipped as incompatible with its
unconditional AI checks. This qualifies the narrow worker relocation scope
on this Mac, with no full-AI/clean-Mac or whole-product packaging claim.

Independent final raw audit passes10/10 groups with no findings within scope.
It verifies media's eight authored native/portable tables and package read-only
conservation of all46 SQL tables/fourteen persistent files as distinct claims.
Three immutable collections are published with independently audited inventories:
[Primary](../tools/media-qualification/evidence/2026-10-25-camera-regions/files.json),
[Regressions](../tools/media-qualification/evidence/2026-10-25-camera-regions-regressions/files.json)
and [Verification](../tools/media-qualification/evidence/2026-10-25-camera-regions-verification/files.json).
Their [post-publication audit](../tools/media-qualification/evidence/2026-10-25-camera-regions-audit.json)
passes with no findings:576 payloads/579 files/38,682,216 stored bytes, each
collection below256MiB including metadata, two explicit nonauthored sidecar
omissions. The qualification records exact hashes and retention limits.
Implementation commit `4ee7b86d517763eaa1e2680316cbec417ef5d2ed` is pushed
to main and recorded as progress milestone149.
Full raw replay reports remain pinned/indexed, not all copied into bounded
collections. Coverage stays89%, eight Complete/sixteen Partial, effects five
of thirteen. No requirement or gate closes; ranged following and the rest of
the normative scope remain open.

## Earlier scoped whole-owner following creep, 2026-10-24

[Scoped following creep](qualification/scoped-follow-creep-2026-10-24.md)
now admits whole Source, Hold, Sequence, Repeat and Retime owners through
existing valid explicit all-Play scopes, including nested Plays and supported
Retime traversal. Scale stays in selected owner output time; target lookup
uses exact Original provider time. Existing scoped authority, clone/isolation,
timing/audio, unrelated plays and gaps remain intact. Default, invalid/stale
scope, Visual ranges and implicit animated-path replacement still refuse.

Focused Core three and scoped semantic fifty-nine pass, with sixty-one unique
Core cases after overlap; CLI framing sixteen plus one resolver unit case
pass. Two Plan tests cover four point/region cases, 200 pictures and fifty-two
independent owner samples with complete mapped picture/audio preservation.
Three independent reviews have no findings. The first test-only Clippy 101
and two CLI proof-helper failures remain retained; corrected focused Plan,
App check, format and strict Clippy pass with all twenty-two listed sources
unchanged. The command writer's intentional 101 is separately retained.

Full Core/Plan passes 2,059 tests with zero failed/ignored and all twenty-two
source inputs unchanged. Full App passes 1,304 with eleven existing ignored;
ordinary command-reference validation and release pass on unchanged sources.
The same candidate passes scoped 155, Repeat 134, Retime 134, zoom 213,
Sequence 101 and Hold 67 native checks, each with Kestrel and unchanged guards.
Four actual images are reviewed; interactive controls fit, while the long
Camera explanation reaches its pane clip. Two declared physical/picker
exclusions remain. The first media driver stopped before any CLI command on
an overstrict exclusion assertion. A second stopped before rendering after
one read-only dump on an incorrect .58 fallback oracle; the retained entry
pose is (.5,.5). Both failures remain preserved without product changes or a
native rerun. Final actual media passes all nine commands, 240 pictures and
full 384384 stereo samples at zero offset/uncovered/gaps; portable document and
history digest match, with twenty-five revisions/fifteen history rows.
First/final twenty-two sources/five binaries are identical: one candidate/
scoped workflow. Independent raw auditing retains two assertion stops:
historical worker test-helper snapshots versus final scope, then a one-round
oracle instead of staged Q32 rounding. No product/evidence input changed or
product run repeated. The third passing audit is preserved before its final
integrity guard; fourth execution passes with all seven native/portable tables
exactly equal. The first package verifier stops on SQLite in-memory
deserialization of a checkpointed WAL-format database; its exact attempt is
preserved, along with the passing retry before final checkpoint binding.
Final third two-package verification passes with package/product bytes
unchanged. Primary/regression packages retain all six full native reports and
four reviewed images, with four explicit metadata sidecar omissions totaling
65,536 bytes and zero authored omissions. The separate verification supplement
retains final and earlier audit receipts. All three complete package readbacks
pass. Coverage remains 89%, eight Complete and sixteen Partial; Section8
effects coverage remains five of thirteen. No requirement
or gate closes. Ranged live following, non-face proposals and full normative
scope remain open.

## Earlier whole-Retime following creep, 2026-10-24

[Whole-Retime following creep](qualification/retime-follow-creep-2026-10-24.md)
adds the fifth ordinary whole-owner type. Scale follows the complete Retime
output clock while target lookup uses exact fractional Original coordinates,
separate from decoded picture ordinals. Child framing, pitch, audio and time
maps remain unchanged; occurrence and Visual-range authoring still refuse.

Focused Core two, CLI framing fifteen and Plan three pass. Ten Plan cases
check 78 owner and forty neighbor samples with independent time/scale/center
oracles. Current seventeen-path formatting, App check and strict Clippy pass;
three fresh independent reviews have no findings. The early Core global
snapshot's concurrent Plan-test delta is explicit; bounded Core inputs stayed
stable and subsequent CLI/Plan scope is complete and stable.

All 2,056 Core/Plan and 1,304 App tests and the release build pass with all
seventeen source inputs unchanged. Eleven existing App opt-in cases remain
ignored. The single candidate passes 129 native Retime, 101 Sequence, 129 Repeat
and 213 zoom checks with matching Kestrel audits; final images are reviewed.
The actual native project exports all 120 pictures/full 192,192-sample stereo
window at zero offset/gaps, with an exactly equal portable document. Complete
selected history tables match, including 21 revisions and thirteen history
rows. The final independent raw audit passes; its earlier audit-script schema
stop is preserved without a product rerun. The first independent immutable
package verification and complete Primary/supplement readback pass, with no
metadata or authored omissions.
Coverage remains 89%, eight Complete and sixteen Partial; no requirement or
gate closes. Scoped/ranged
live following-creep authoring, non-face proposals and full normative scope
remain open.

## Earlier whole-Repeat following creep and live clipping, 2026-10-24

[Whole-Repeat following creep](qualification/repeat-follow-creep-2026-10-24.md)
adds ordinary whole-owner authoring across all plays and intervening gaps,
retaining each provider's Original clock and separate inner escalation.
Retime, range and nested/selected-occurrence authoring still refuse.

An independent renderer comparison also exposed live Follow projection
ignoring intermediate crops without captured context. The shared helper now
honors every live clip irreversibly, using fallback while scale continues
when a target is removed. The old exterior-center behavior is corrected;
legacy no-aspect coordinate placement and wire/numeric formats remain.

Focused Core two, CLI framing fourteen, Plan three and renderer three pass;
the renderer includes 600 live and 960 captured geometry comparisons. The
old-helper failure and exact old sources remain retained. App and strict
checks, all 2,053 Core/Plan and 1,304 App tests, scoped formatting and the
release build pass. Eleven existing App opt-in cases remain ignored. Three
independent reviews have no findings. The single candidate passes 129 native
Repeat, 96 Sequence, 213 zoom and 67 Hold checks with Kestrel audits; final
images are reviewed. The repeated-scenario invocation only exercised Sequence;
zoom and Hold have separate passing runs. The native edit has eighty pictures.
The actual saved native edit exports all eighty pictures and the complete
128,128-sample stereo audio window at zero offset/gaps, with an exactly equal
portable document. A CLI-derived case verifies 28 clipped frames and complete
media/audio; its original wrapper's empty-WAL guard failure is preserved and
reconciled offline without another product run. Independent history/raw-result
checks pass. The immutable Primary has four explicit metadata sidecar omissions
and no authored omissions. Its first independent package verification and
complete Primary/supplement readback pass; eight retained raw EOF whitespace
warnings remain distinct from passing authored diff checks.

Coverage remains 89%, eight Complete and sixteen Partial. No requirement or
gate closes; Retime/scoped following, non-face proposals and the remaining
full normative scope stay open.

## Earlier whole-Sequence following creep, 2026-10-24

[Whole-Sequence following creep](qualification/sequence-follow-creep-2026-10-24.md)
adds Keep-center authoring on a selected ordinary group. Its complete output
clock drives scale while each child's actual Original clock drives the target
center across cuts. Existing child primitives and framing stay composable;
Visual, occurrence and Repeat/Retime-owner refusals remain.

Focused cases, thirteen CLI framing cases, all 2,049 Core/Plan and
1,304 App tests, strict checks, scoped formatting and the release build pass.
Eleven existing App opt-in cases remain ignored. Independent general, clock
and test reviews have no findings.
The single release candidate passes 107 native Sequence, 67 Hold and 213 zoom
checks with matching Kestrel audits; final images are reviewed. The actual
saved edit exports eighty pictures and the full 128,128-sample stereo window
at zero offset/gaps, with an exactly equal portable document. Independent
raw-result and complete immutable-history checks pass, including twenty-six
revisions and fifteen history rows. The first independent immutable package
verification and complete supplement readback pass.

Coverage remains 89%, eight Complete and sixteen Partial. No requirement or
gate closes.

## Earlier whole-Hold following creep, 2026-10-24

[Whole-Hold following creep](qualification/hold-follow-creep-2026-10-24.md)
adds ordinary Hold authoring and exact target projection through retained
captured views. A clipped target uses its fallback while scale continues.
Provider, captured geometry, audio recipe and duration remain unchanged.
Camera and the committed plan share projection; compact picker markers and
active-target labels address crowding found in the first native replay.

All 2,047 Core/Plan and 1,304 final App tests, the strengthened CLI Hold case,
the independent 960-case renderer geometry comparison, strict checks and
formatting pass. Final native Hold/faces/targets/zoom/points replays pass
67/69/41/213/34 checks, each with the live Kestrel audit. Final images are
reviewed. The actual saved edit exports all 132 pictures and the complete
211,411-sample audio window at zero offset, with an exactly equal portable
document. Independent raw-result and immutable-history checks pass, including
eighteen revisions and eleven history rows. The first independent immutable
package verification and complete supplement readback pass.

Coverage remains 89%, eight Complete and sixteen Partial. No requirement or
gate closes; broader scopes and general non-face proposals stay open.

## Earlier first-class point targets, 2026-10-24

[Point targets](qualification/point-targets-2026-10-24.md) now store only their
source center and retain exact interpolation, nested Follow, manual correction,
copy and history semantics. Camera switches rectangle / point while creating,
locks the saved kind during corrections and saves target and framing separately.
Tracking refuses points before work; area-based AI guidance retains their
identity with explicit unavailable coverage.

All 2,041 Core/Plan, 1,304 App, 533 CLI and 186 Models tests pass; existing opt-in
ignores remain explicit. The shared Rust/Python golden and 62 Python tests pass.
Final Metal replays pass 34 point, 41 rectangle-target, 66 face and 213 zoom
checks, each with the live Kestrel audit. The actual saved edit exports all 120
pictures and the complete 192,192-sample audio window at zero offset. Strict
checks and formatting pass. Image review fixed stale target help and feedback.

Coverage remains 89%, eight Complete and sixteen Partial. General non-face
region proposals, broader following-creep authoring and the remaining normative
scope stay open. No requirement or gate closes at this checkpoint.

## Earlier Camera detected-face picker, 2026-10-24

[Camera face proposals](qualification/camera-face-picker-2026-10-24.md) now
append to the numbered picker without changing existing digits. Keyboard pages
reach every entry. Explicit selection saves only the chosen target; Camera
continues its draft and Enter commits framing separately. Late results and
refused saves preserve the captured context and history semantics.

All 1,301 App unit tests pass, with eleven existing opt-in cases ignored.
Strict checks, formatting and the release build pass. Final Metal replays pass
66 face, 41 target and 213 zoom checks, each with the live Kestrel audit. Actual
image review fixed crowded badges and a stale detection notice. The final
saved edit exports 120 pictures and a complete 192,192-sample audio window at
zero offset; the verified portable copy retains identical authored history.
Positive detector observations are scripted. Raw failures, warnings and the
post-run diagnostic-wording correction remain explicit in the linked record.

Coverage remains 89%, eight Complete and sixteen Partial. Point targets,
general non-face proposals and broader following-creep authoring remain open;
no requirement or gate closes at this checkpoint.

## Target-following creep, 2026-10-24

[Following creep](qualification/follow-creep-2026-10-24.md) lets a whole ordinary
Source retain its live target while scale changes on the owner clock. Separate
live/fallback bases, Lost-region holding, exact retained clocks, Camera scaling
and explicit Static release survive the shared command, plan and history paths.
Recording and direct-edit dot retain semantic Keep intent. Broader authoring
scopes and remaining target controls stay open; DP-08 remains Partial.

The first complete Core/Plan lane passes 2,020 tests. Additional ingress and
structural witnesses, 1,294 App and 532 CLI unit tests, twelve CLI framing cases,
strict checks and formatting pass. Seventeen existing opt-in unit cases remain
ignored and are not verification evidence. The final release replay passes 213
workflow checks and the Kestrel audit, including a real Camera event-loss bug
found and fixed during qualification. Both fresh media runs verify all 120
pictures and 192,192 stereo samples at zero offset, with complete unchanged
canonical PCM, independent pose oracles and portable history. The linked record
retains failures, warnings, source/binary identities and proof limits.

Coverage remains 89%, eight Complete and sixteen Partial. No requirement,
performance target or delivery gate closes at this checkpoint.

## Compound wire-count reuse: production qualification, 2026-10-24

The [uninstrumented qualification](qualification/compound-wire-reuse-2026-10-24.md)
promotes exact local count reuse for one eligible InsertTime replay. Main's
2,053 Rust files and six inputs exactly match the tested snapshot. All 729
Core unit tests, 35 integration tests, 12 release Store tests, strict/format
checks and the real-media history/PCM case pass. Both actual exports verify
45 pictures and 72,072 samples at zero offset. Full initial/post counts,
every work charge, original full-serializer refusals and Store checks remain.

Five fresh alternating timing pairs pass 70 pause and 10 Kestrel checks.
All five improve, with paired median commit/composition reductions of
5.737334/5.692792 ms. No p95, 100 ms/50 ms target, memory claim, requirement
or gate closes. Coverage remains 89%, eight Complete and sixteen Partial.

## Compound wire-count reuse: private qualification, 2026-10-24

The [private trial](qualification/compound-wire-reuse-prototype-2026-10-24.md)
reuses exact counts for an untouched working clone and a header-only change
inside one eligible InsertTime replay. Actual initial/post counts, every
work charge and original full-serializer refusals remain. All 737 Core unit
tests, 35 integration tests, 12 release Store tests, strict/format checks and
the real-media history/PCM case pass; both exports verify 45 pictures and
72,072 samples at zero offset.

Eighteen native trials pass 126 pause and 18 Kestrel checks. Six matched pairs
have median commit/composition reductions of 6.030062/6.203416 ms, retaining
one slower pair. Replay-wire medians are 13.2755215/6.7645835 ms. Independent
raw-result review agrees. At that checkpoint Main runtime was unchanged;
the separate uninstrumented qualification above now promotes the change.
No p95, target or memory claim is made by the private batch.
Coverage remains 89%, eight Complete and sixteen Partial.

## Single-leaf net reuse: production qualification, 2026-10-24

The [uninstrumented qualification](qualification/single-leaf-net-reuse-2026-10-24.md)
promotes the narrow Core edit-retention change. Main's 2,053 Rust files match
the tested snapshot exactly. All 726 Core unit tests, 35 integration tests,
12 release Store tests, strict/format checks and the real-media history/PCM
case pass. Both actual exports verify 45 pictures and 72,072 samples at zero
offset. Store's actual result, encoded-size/readback and inverse checks remain.

Five fresh timing pairs pass all 70 pause and 10 Kestrel checks, with a median
paired commit reduction of 5.355917 ms and composition reduction of 5.468292 ms.
Six separate memory runs have median sampled physical-footprint maxima of
667.423/665.939 MiB and resident maxima of 653.922/672.938 MiB. These whole-app
samples establish neither a retained-edit cost nor a memory ceiling or saving.
The independently reread package has 341 payloads /12,369,663 bytes. No p95,
100 ms/50 ms target, requirement or gate closes. Coverage remains 89%, eight
Complete and sixteen Partial.

## Single-leaf net reuse: private qualification, 2026-10-24

The [private prototype](qualification/single-leaf-net-prototype-2026-10-24.md)
retains the successful leaf edit and changes only its outer revision headers
and description after final admission. Release Core avoids repeated aggregate
validation, diff and inverse work; Store still checks the actual result,
encoded edit, readback and inverse. All 734 Core unit tests, 35 integration
tests, 12 release Store tests, strict checks and the real-media history/PCM
case pass, including two verified 45-picture, 72,072-sample zero-offset exports.

Six matched timing pairs show a 6.047542 ms median commit reduction. Separate
whole-app memory samples show median observed maxima of 651.329 MiB for the
baseline and 668.954 MiB for the candidate; they do not isolate retained edit
memory or establish a ceiling. The 541-payload package and independent reread
are retained. This is private diagnostic evidence, separate from uninstrumented
qualification. No code is promoted by this record, no gate or target closes,
and coverage remains 89%, eight Complete and sixteen Partial.

## Insert-split proof prototype, 2026-10-23

The [private prototype](qualification/insert-split-proof-prototype-2026-10-23.md)
reuses a validated entry duration map only around the existing interior Split
bridge for a narrow InsertTime case. Six matched diagnostic pairs report a
4.502272 ms median input-to-commit difference and a 4.452875 ms median
offscreen-composition difference. All 35 phase counts and sums conserve; all
126 pause and 18 Kestrel checks pass. The private instrumented run is not
production qualification; the same narrow source change has separate
production evidence below. The 100 ms/50 ms targets remain open. Coverage remains
89%, eight Complete and sixteen Partial.

## Insert-split proof reuse: production qualification, 2026-10-23

The [production qualification](qualification/insert-split-proof-reuse-2026-10-23.md)
promotes one reviewed Core source change: for a narrow local/compact
InsertTime case, it borrows the already-validated entry duration map only
around the existing interior Split bridge. It avoids one redundant successful
validation of the captured working input; the capture, result, authority,
binding, budget and history checks remain. No API, cache or schema changed.
All 724 Core unit tests, 22 Core integration tests, 12 Store tests, formatting
and strict checks pass.

Five fresh uninstrumented control/candidate pairs pass 70 pause workflow and
10 Kestrel checks, with unchanged source and inputs, empty WALs, and 20 retained
layout warnings. Paired median differences are 2.380874 ms to commit and
2.504043 ms to offscreen composition; these five pairs establish no percentile
or target pass. The cached and uncached release-media/history test passes once
from the same fresh release archive after the extraction setup correction,
and verifies two independent 45-picture,
72,072-sample exports at zero offset. The 100 ms and 50 ms targets remain open;
no requirement or gate closes. Coverage remains 89%, eight Complete and sixteen
Partial.

## Detailed first-pause costs, 2026-10-23

The [private fine diagnostic](qualification/current-pause-detail-profile-2026-10-23.md)
refines the two dominant regions while reporting 26 exclusive rows for the
whole worker route. Eight tests of the
exact scalar helper, twelve release transaction tests, twenty parser tests,
release native integration, strict Core/Store/App all-target Clippy and
formatting pass. Nine fresh rotated trials pass 63 workflow/nine Kestrel checks
with eighteen layout warnings and unchanged authored inputs.

Median pure leaf application is 43.467459 ms, semantic planning residual
33.948583 ms and history codec/readback 22.490084 ms. The 4+4+3 named document-size
sites take a same-trial median 38.098416 ms, not every serialization. Capability
snapshot construction is only 0.004750 ms. All 26-phase sums/counts conserve;
the checkpoint row is exactly count zero/time zero. Worker median is
188.133125 ms. Control/off/on commit medians 194.051000 /194.373417 /
194.426958 ms overlap and establish no overhead bound.

The independently checked primary package has 195 payload files /4,749,341 bytes,
25 decoded gzip streams and nine safe authored-fixture archives. Both packager
and independent verifier pass on their first execution. Inspect the remaining
planning/application work before selecting a bounded optimization. Temporary
hooks remain outside main; no target, memory claim, requirement or gate closes.
Coverage stays 89%, eight Complete and sixteen Partial; the full goal is active.

## Semantic preflight leaf profile, 2026-10-23

The [private V3 profile](qualification/semantic-preflight-leaf-profile-2026-10-23.md)
adds nine exclusive child phases to the prior route trace. Three rotated
control/off/on triplets pass seven pause workflow checks and one Kestrel check
per run, with exact 35-row trace conservation and unchanged source/input guards.
Enabled medians are 92.230291 ms for the semantic region, 43.943916 ms for the
pure leaf region, and 32.736583 ms for legacy insertion including input-duration
calculation. The separately measured 1.471250 ms head-validation and 1.284166 ms
input-duration calls do not justify bypassing validation. The overlay is private;
no requirement or gate closes, and the 100 ms/50 ms targets remain open.
Coverage stays 89%, eight Complete and sixteen Partial.

## Coarse first-pause route costs, 2026-10-23

The [private current-route diagnostic](qualification/current-pause-route-profile-2026-10-23.md)
measures the qualified one-leaf implementation with thirteen disjoint worker
categories. Three rotated control/off/on triplets pass 63 workflow and nine
Kestrel checks, retaining eighteen layout warnings and unchanged authored
inputs. All eight new helper, twelve transaction and eighteen parser tests,
strict release Core/Store/App all-target Clippy and formatting pass.

Enabled worker-route median is 188.710583 ms. Semantic planning is
91.664125 ms and Store preparation 60.879542 ms; each trial's thirteen scalar
intervals conserves exactly. Control/off/on commit medians are
195.230667 /194.561208 /195.100625 ms, with overlapping observations and no
overhead bound. The independent reread verifies 184 payload files /4,143,448
bytes, 23 gzip streams and nine authored-fixture archives. Three corrected
independent-verifier assumptions are retained separately.

The temporary timers remain outside main. Refine those two dominant regions
before choosing another optimization. No percentile, memory, physical-display,
100 ms/50 ms target, requirement or gate closes; coverage stays 89%, eight
Complete and sixteen Partial. The full goal remains active.

## Validated leaf-result reuse, 2026-10-23

A private one-site [prototype](qualification/reused-leaf-result-prototype-2026-10-23.md)
reuses the validated document returned by the existing `apply_with_result`
instead of applying its forward patch again. All byte charges, staged updates,
identity/provider checks, Store admission and history checks remain.

Five quiet fresh-package pairs measure input-to-commit medians
270.121541 /266.234959 ms and offscreen composition 272.684834 /268.738125 ms.
The roughly 4 ms difference is diagnostic, not a formal percentile or target
PASS; this is a separate batch from the qualified shared-preparation result.
All ten replay/input/source/binary guards pass. Evidence verifies 118 files
/1,356,170 bytes.

The production change and pause/setter differential test are qualified: all
1,514 Core tests across 69 binaries pass with zero skipped, 3,505 broader tests
pass with 740 skipped, and 1,296 UI tests pass with eleven skipped. Strict
Core/UI Clippy and formatting pass. Debug/release verifies 15 pictures and
24,024 audio samples; release also verifies 19 pictures and 30,430 samples,
all at zero offset with canonical PCM/history controls. Six native scenarios
pass 1,340 workflow checks plus six Kestrel checks. Both nine-file portable
extractions validate and doctor without changing paths or bytes.

Five final quiet pairs measure control/candidate input-to-commit medians
272.579417 /265.164417 ms and offscreen composition 275.099042 /267.862584 ms.
The 7.236458 ms composition difference is about 2.63%, diagnostic only. The
100 ms fallback and 50 ms ordinary-edit targets remain open. These samples
are separate from the earlier prototype. See the
[final qualification](qualification/reused-leaf-result-2026-10-23.md).

The initial raw primary-mark lookup failed after a split; the fixture
now uses the revision-aware logical mark API while retaining exact frame 12,
all differential/replay, binding/mark, inverse and immutability assertions.
Production code stayed unchanged during that correction. Preserve the failed
run and its executable hash. See the
[Core checkpoint](qualification/reused-leaf-result-core-checkpoint-2026-10-23.md).
Both passing suite tests reported as leaky pass isolated reruns from the same
archived binaries without that warning. The first rerun wrapper failed before
any test on an incorrect guard filename; its corrected wrapper and all native
layout warnings remain in the evidence. No requirement or gate closes.

## Exact compact JSON size counting, 2026-10-23

Size-only Store traversals now use the identical compact serializer without
retaining complete output buffers. Captured checkpoint bytes, all work charges
and refusal order remain. Core 58 /SQLite 87 stay unchanged. All 1,517 Core
tests (zero skipped), 3,506 broader tests (740 skipped), 1,296 UI tests (eleven
skipped), strict Core/Store/UI checks and formatting pass. The UI suite's one
leak warning does not recur in an isolated same-archive rerun. Debug/release
media, six native workflows and both portable extractions pass.

Five final quiet pairs give commit medians 266.145459 /259.015416 ms and
offscreen composition 268.773292 /261.593583 ms. The latter difference is
7.179709 ms, about 2.67%, diagnostic only. The 100 ms/50 ms targets stay open.
The [final qualification](qualification/compact-json-size-2026-10-23.md) retains
467 files /15,051,302 bytes, including three emitted movies and complete
decoder/control evidence, all independently verified. The earlier
[private prototype](qualification/compact-json-size-prototype-2026-10-23.md)
and [Core checkpoint](qualification/compact-json-size-core-checkpoint-2026-10-23.md)
remain separate historical records. No requirement or gate closes.

## Complete frozen-layout reuse, 2026-10-23

Complete immutable frozen timing tables now reuse their admitted storage after
the full alias closure and canonical wire guard; provisional, partial and
reference behavior remain. All 1,522 Core tests (zero skipped), 3,506 broader
tests (740 skipped), 1,296 UI tests (eleven skipped), strict Core/UI checks and
formatting pass, without leak annotations. Debug/release media and canonical
history controls, six native workflows and both portable copies pass.

Five final quiet pairs give commit medians 257.618792 /248.292125 ms and
offscreen composition 260.353458 /250.976250 ms, a 9.377208 ms /3.60%
diagnostic difference. The 455-file /14,990,467-byte inventory verifies and
retains all three emitted movies and decoder/control reports. See the
[production qualification](qualification/full-timing-closure-reuse-2026-10-23.md).
No formal percentile, memory target, requirement or gate closes.

## Semantic refresh duration reuse, 2026-10-23

Selection refresh now shares the same immutable staged document's proved
durations while retaining bounds/index query order and reference/derived
fallbacks. Four new differential tests pass in the full 1,526-test Core run
across 70 binaries, with zero skipped. Broader 3,506 tests (740 skipped), UI
1,296 tests (eleven skipped), strict Core/UI checks and formatting pass without
leak annotations. Debug/release media and canonical history controls, six
native workflows and both portable copies pass.

Five final quiet pairs give commit medians 246.381375 /238.914458 ms and
offscreen composition 248.946250 /241.268958 ms, a 7.677292 ms /3.08%
diagnostic difference. The 470-file /15,087,083-byte inventory verifies,
including all three emitted movies and decoder/control reports. See the
[production qualification](qualification/semantic-refresh-duration-reuse-2026-10-23.md).
The private prototype below remains a separate batch. No percentile, memory
target, requirement or gate closes; the 100 ms/50 ms targets remain open.

## Remaining preparation cost, 2026-10-23

The [current leaf timing and owned-memory diagnostic](qualification/current-leaf-timing-memory-profile-2026-10-23.md)
passes all 18 fresh native trials and 54 commands, with 126 workflow and
18 Kestrel checks. Three enabled timing traces measure semantic and replay
leaf medians 43.780792 /43.559500 ms inside a 233.354042 ms service route.
The 146.001626 ms residual is computed per trial, not from subtracted medians.
A separate memory lane retains 16,303 owned-app samples; observed footprint
maxima are about 659–700 MiB with gaps up to 7.818667 ms. Sampled whole-app
lower bounds cannot establish exact/per-leaf peaks, retention or a ceiling.
The 376-file /4,211,716-byte inventory and two-file toolchain supplement verify;
all failure history and 36 layout warnings remain. No production change,
calibrated overhead, percentile or target PASS follows.

The private [one-leaf replay reuse prototype](qualification/one-leaf-replay-reuse-prototype-2026-10-23.md)
passes 39 focused checks without warnings and independent review. All twenty
fresh trials pass, with 140 pause and twenty Kestrel checks; all forty layout
warnings remain. Five timing pairs give control/prototype composition medians
242.163542 /197.496833 ms, a 44.666709 ms /18.44% diagnostic difference.
Separate memory trials retain 18,296 whole-app samples, with gaps up to
12.329791 ms; these are sampled lower bounds, not per-leaf peaks or savings.
The 394-file /5,809,852-byte inventory verifies. Exact immutable value/mode
binding preserves independent Core/Store authority; this remains a separate batch.

[Production one-leaf replay reuse](qualification/one-leaf-replay-reuse-2026-10-23.md)
is qualified at 5fdae748. All 1,543 Core, 3,513 broader and 1,296 UI tests pass
(0/741/11 skipped), with strict checks, native workflows and portable copies.
Completed test maps remain truthful across one reviewed test-only PCM reader
amendment; strict and affected media controls rerun afterward. Both cached and
uncached real-media commits preserve full PCM/history and verify 45 pictures /
72,072 samples each at zero offset. All five movies and 995 evidence files /
23,149,781 bytes verify independently. Five final pairs give offscreen medians
243.137083 /197.389583 ms, a 45.747500 ms /18.82% diagnostic difference.
Five actual failures, warnings and clean isolated case checks remain retained.
No percentile, peak/retention memory, latency target, requirement or gate closes.

A private [semantic refresh duration reuse prototype](qualification/semantic-refresh-duration-reuse-prototype-2026-10-23.md)
reuses one complete map for the same immutable staged refresh, preserving
query/error order, assignments, both anchor indexes and all authority. Four
new integration tests plus three semantic-trim and three command-work tests
pass. Five quiet pairs give commit medians 245.353334 /240.185084 ms and
offscreen composition 248.012042 /242.880417 ms, a 5.131625 ms /2.07%
diagnostic difference. The initial private-constructor compile failure and
equivalent test-helper correction are retained. The 161-file /2,865,045-byte
inventory verifies. Production qualification is complete above; no target closes.

A private [complete frozen-layout reuse prototype](qualification/full-timing-closure-reuse-prototype-2026-10-23.md)
recognizes a full alias closure after all existing alias checks, retains the
canonical wire guard, excludes provisional layouts and reuses its admitted
immutable table. Five new regressions plus eleven scoped-capture, three
command-work and 104 InsertTime tests pass. Five quiet native pairs pass every
guard and measure commit medians 257.887084 /244.544917 ms and offscreen
composition 260.609875 /247.164042 ms, a 13.445833 ms /5.16% diagnostic
difference. The 138-file evidence inventory verifies. Those prototype results
remain separate from the completed production qualification above; no latency
target closes.

The subsequent [Core detail profile](qualification/core-planning-detail-profile-2026-10-23.md)
passes nine rotated control/off/on trials with all pause/shortcut and identity
guards. Its three enabled traces validate 24 phase counts and nine containment
equations. Across two leaf applications, median reduction totals 64.732208 ms
and postprocessing 17.760625 ms. Semantic execution has a 30.641416 ms residual
outside the leaf call and after-leaf size/charge. These contain multiple
operations and overhead; the next study must identify a concrete repeated
operation before changing it. The 152-file /3,434,107-byte evidence inventory
verifies. No production change, target PASS or requirement closure follows.

The later [Store detail profile](qualification/store-preparation-detail-profile-2026-10-23.md)
passes nine rotated control/off/on runs with seven pause checks and one
Kestrel check each. All three enabled traces pass exact 33-phase counts and
ten containment equations. Median semantic planning is 103.020292 ms; Store
preparation is 110.396833 ms, including 76.287833 ms Core replay (5.053292 ms
host visitor) and 19.017083 ms stored-edit historical decode/equality.
Replay minus visitor is a 71.226624 ms residual, not an individual Core phase.
The next investigation must attribute Core planning/replay internally before
choosing another algorithmic change. Control/off/on overhead remains
uncalibrated. This private profile changes no production source or target
status. Evidence verifies 141 files /2,997,706 bytes.

The isolated [prepared-Compound profile](qualification/prepared-compound-profile-2026-10-23.md)
passes nine fresh-process runs and exact source/binary/input guards. In three
profile-on runs, median semantic planning is 108.336666 ms and Store command
preparation 117.677375 ms, versus 7.352000 ms writing and 6.503167 ms durable
commit. Refresh is 4.204375 ms. Totals are nested; children are disjoint. Queue
wait and calibrated timer overhead remain unmeasured. The empty-bank callback
cost is not a general bound for copied-content programs.

This is a diagnostic after the qualified improvement below, with no further
production change or target PASS. It directs the next investigation toward
planning, validation and staging, while preserving work bounds and durability.
Evidence verifies 129 files /1,718,991 bytes; no requirement or gate closes.

## Shared transactional runtime preparation, 2026-10-23

Native semantic edits now prepare their Compound once inside the existing
Immediate transaction. A fallible host callback prepares the runtime bank
before any write; that private bank is returned only after durable commit.
Head, bank, allocation, capture and consumed-analysis authority remain checked,
and old commit APIs use the same path. No schema change is involved.

All five focused Store tests, 3,505 broader selected tests (740 skipped), and
1,296 UI-feature tests (11 skipped) pass. Changed-package and explicit UI
Clippy plus formatting pass. Debug/release verify 15 pictures /24,024 samples
and release verifies 19 pictures /30,430 samples at zero offset, with canonical
PCM/history controls. Six release scenarios pass 1,340 workflow checks plus
Kestrel per run. Both nine-file portable extractions validate/doctor unchanged.

Five quiet fresh-package pairs improve input-to-commit median
390.680250 → 273.119542 ms and input-to-offscreen Background composition
393.261916 → 275.697709 ms. These are short diagnostics, not formal percentiles
or a 100 ms fallback/50 ms ordinary-edit target PASS. Source decoding, physical
display and PCM performance are outside this fixture. The failed old Split
expectation also fails on the qualified baseline; the corrected unsupported
insertion replay passes 383 checks. Preserve that failure, the initial zero-test
filter, debug performance-mode refusal and layout warnings. See the
[qualification](qualification/compound-runtime-preparation-2026-10-23.md).
Coverage stays 89%, 8/24 sections complete; no requirement or gate closes.

## Native first-pause service profile, 2026-10-23

Three rotated root-control/instrumentation-off/on triplets all pass seven
replay checks and source/binary/input guards. Root-control median is 385.551042
ms input to commit and 388.289833 ms through offscreen composition. Instrumented
service phase medians locate 108.376500 ms in semantic planning, 119.775334 ms
in preview/runtime-bank preparation and 130.474125 ms in Compound commit;
refresh is 4.128667 ms. These are coarse diagnostic phases with inclusive
totals, uncalibrated overhead and no queue-wait or percentile claim. See the
[profile](qualification/ui-first-pause-profile-2026-10-23.md).

A transaction-owned runtime-preparation callback avoids replaying the Compound
for a separate preview. The uncommitted candidate passes five focused Store
tests, all 3,505 broader selected tests (740 skipped) and all 1,296 UI tests
(11 skipped). Its bad native capture-provenance case rejects before saving
the bank and then retries successfully. Two reviews report no defects.
The initial native `--lib` filter selected zero tests, which is retained as a
qualification mistake. At that checkpoint strict, native/media, release and
performance checks were still owed; the completed qualification is above. See
the [checkpoint](qualification/compound-runtime-preparation-checkpoint-2026-10-23.md).

## Large-project first-pause measurement, 2026-10-23

The verified `large-first-pause` replay measures one production `hold 12f`
insertion at frame 17,933 in a fresh 10,000-Hold package. Its final release
sample takes 393.681375 ms to commit and 396.208834 ms through new-revision,
same-ticket offscreen Background composition. This is one diagnostic sample,
not a percentile or target PASS. No native-window, scanout, source-decode or
PCM claim follows. The 100 ms fallback and 50 ms ordinary-edit targets remain
open. Direct store-and-refresh measurements have different boundaries.

All 1,295 UI-feature tests pass (11 skipped), strict UI Clippy and formatting
pass, and seven replay checks pass. The original authored inputs remain
unchanged; the final full-file guard accounts only for SQLite shared-memory
and empty-WAL coordination files. Retain the initial wrapper's failed full-path
assertion and the two-frame layout retry warning. See
[qualification and evidence](qualification/large-first-pause-harness-2026-10-23.md).
Coverage remains 89%, 8/24 sections complete; no requirement or gate closes.

## Compact bindings and full-capture fallback, 2026-10-23

Implemented and qualified: Core 58 / SQLite 87 omits six exact binding defaults,
retains bounded raw preflight and nondefault semantics, and skips a scoped
projection that cannot be smaller. Old unused packages are refused without
mutation. Independent frozen-context/sound-clock formats remain unchanged.

All 1,513 Core, 3,499 broader selected and 1,292 UI-feature tests pass, plus
strict changed Core/Store, explicit UI Clippy and formatting. Debug/release
verify 15 pictures / 24,024 samples and release verifies 19 pictures / 30,430
samples, all at zero offset with complete canonical/history controls. Native
passes 197 checks plus Kestrel. Both nine-file extracted packages validate and
doctor unchanged, as do all five edited benchmark packages under full replay.

Five quiet current-format pairs measure first store-and-refresh median
114.587 → 93.633 ms and second 32.896 → 28.906 ms. Picture display and formal
percentile qualification remain unmeasured. Native descriptive commit p95 is
90.561250 ms over nine samples, so the full 50 ms ordinary-edit target stays
open. Retain every failed build/fixture run, one passing output-handle warning
with clean isolated/full reruns, and two native layout warnings. Final evidence
verifies 683 files / 24,889,683 bytes. See
[qualification and evidence](qualification/compact-audio-bindings-2026-10-23.md).
Coverage remains 89%, 8/24 sections complete; no requirement or gate closes.

## Binding and capture diagnostics, 2026-10-23

Four isolated five-pair batches measure the remaining first-pause overhead.
The combined prototype improves first store-and-refresh median from 111.576 to
91.629 ms; the standalone binding reader is still incomplete in that prototype.
A round-trip test exposed its missing-path preflight. The final implementation
above adds bounded optional paths and historical fixture preservation, and
separately completes Core 58 / SQLite 87 qualification. No visible-Hold, p95 or gate PASS is claimed. See the
[diagnostic report and 329-file evidence inventory](qualification/first-pause-binding-prototypes-2026-10-23.md).

## Compact frozen timing, 2026-10-23

Implemented and qualified: Core 57 / SQLite 86 omits only all-Automatic frozen
edge objects, retaining exact policies and the complete timing lattice.
Standalone context codecs 1–12 preserve their historical bytes. Old unused
packages are refused without mutation.

All 1,503 Core, 3,499 broader selected and 1,292 UI tests pass, followed by six
strengthened codec cases, strict Core/Store and UI Clippy, and formatting.
Debug/release verify 15 pictures / 24,024 samples; release also verifies
19 pictures / 30,430 samples, all at zero offset with full PCM/history controls.
Native replay passes 197 checks plus Kestrel, and both nine-file portable
packages validate and doctor unchanged. Retain two layout findings and all
fixture/setup/driver failures. The earlier lint-scope attribution is corrected.

Five quiet current-format pairs measure first-pause median 130.636 ms before
and 114.691 ms after, with second medians 33.206 / 33.265 ms. This improves the
observed first median by 15.945 ms but remains above 100 ms without a picture;
no p95 or visible-Hold target is established. Native commit p95 remains
85.747709 ms over nine samples, so the full ordinary-edit 50 ms target stays
open. DP-24 remains partial and no gate closes. See
[qualification and evidence](qualification/compact-frozen-edges-2026-10-23.md)
and the earlier [diagnostic profile](qualification/first-pause-profile-2026-10-23.md).

## Neutral retained-scope Ungroup, 2026-10-23

Implemented and qualified: remove a neutral unary introduced group without undoing
later labels, descendant gain or sound parameters. Every affected historical
reference must still treat the group as introduced and unpaired. Scope, timing
tables, recipes and exact aliases remain intact; genuine later captures stay
protected. Core 56 and SQLite 85 are unchanged.

All five focused PCM cases and both actual-media Store cases pass, with frozen
source/input and executed-binary guards. Complete 24,024-sample authored,
limited and limiter-gain comparisons remain exact, including cold/partitioned
reads, later fractional-rate displacement and return. Store verifies forced
rollback, media admission, durable history, registers and portable copies.
Independent reviews, strict ordinary/UI checks and formatting pass. All 1,497
Core tests across 67 binaries pass with zero skipped after correcting three
invalid test assumptions without changing production admission. All 3,499 broader
selected tests pass with 740 ignored or unselected. One passing output-handle
warning does not reproduce in an isolated same-archive run; both are retained.
The UI-feature archive passes 1,292 tests with eleven skipped. The debug
semantic CLI fixture preserves complete PCM/history and verifies all fifteen
pictures and 24,024 audio samples at zero offset. Release passes the same
Ungroup case and the previous nineteen-picture / 30,430-sample grouping,
Split, Move and copy regression at zero offset. Native replay passes 197
workflow checks plus Kestrel, including later gain, Undo/Redo and reopening.
Both nine-file extracted release packages pass validation and doctor unchanged.
Two layout warning findings remain. Input-to-state p99 is 1.578334 ms over
330 samples; commit p95 is 90.700292 ms over nine, so the complete 50 ms target
is not established. Retained
baseline failures, controls and qualification status are in the
[qualification record](qualification/neutral-ungroup-2026-10-23.md). No requirement or
gate closes.

## Aligned sound scopes, 2026-10-23

Implemented, qualified and pushed in `c72abe12`. Scope remains bounded to an
aligned complete Source/Hold group under ordinary Sequence ancestry.

Fresh current-source verification passes all 1,489 Core tests across 67 binaries
with zero skipped, and all 3,492 broader selected tests across 109 binaries
(739 ignored or unselected). The fresh Core library SHA-256 is
`1c69c724a70b193d5e311d4a9089b280c5f6f355a19774c5f1befffd48392bf9`.
Strict ordinary/UI checks and formatting pass. After the broad run, one private
UI replay assertion was corrected to distinguish old pre-Split owner aliases
from each live fragment in a later Move capture. That file is excluded by
`ui-harness` in the ordinary broad build; all other 2,038 Rust hashes and six
inputs remain unchanged. A fresh strict run passes after the correction.
The full UI-feature archive passes 1,292 tests, with 11 skipped. Its frozen
source/input and executed-binary hashes remain unchanged. The debug real-media
preflight passes complete PCM/limiter equality through neutral grouping and
Split, moved/copied cold reads, durable Undo/Redo and portable reopening. It
verifies all 19 emitted pictures and audio samples `[0,30430)` at zero offset,
with no coverage or low-SNR flags. The native replay failed because its new
helper expected committed presentation while the app displayed the exact
proposed picture. The harness now checks the proposal identity and dismisses an
open proposal during failure cleanup; independent source review agrees. The
corrected release replay passes 130 workflow checks plus Kestrel. It retains
two layout-retry warnings; input-to-state p99 is 1.607 ms over 233 samples,
while commit p95 is 88.299 ms over seven samples, so the full 50 ms target is
not established. Fresh strict checks and formatting pass after the correction;
the renewed UI archive passes 1,292 tests with 11 skipped. The release
aligned-scope media case also passes all 19 pictures and
30,430 samples at zero offset, with complete staged PCM/history controls; the
121-picture / 193,794-sample endpoint regression also passes at zero offset.
Both safely extracted nine-file release packages pass full validation and
doctor without changing any package path or byte. Exact sources, executables,
earlier failures and complete numerical evidence are retained in the
[qualification record](qualification/aligned-sound-scope-2026-10-23.md).
No requirement or gate closes.

The `integrated-eleventh` archive passes twelve focused audio checks. A new
valid-control counterfactual first measured 7,581 changed output channel samples
after a label-only edit and 7,570 after a fragment trim. Removing live labels
and treatment values from the certified permission identity fixes both, with
exact complete PCM and raw gain-input equality. Recipe replacement still
retires authority. Audio SHA-256:
`37673f39244c8b762efd57c00ae541cc984386d0df3307fb2945a27e4038bccc`.
The subsequent Store archive passes three introduced-group/Split-forest cases
through Source and RoomTone Hold history, reopening and portable copies, with
binary SHA-256 `ff227d7cf191f2b34983cc693048725c9a6e08a4469648b5ab41848478a206cc`.
Later strict lint cleanup changes the Core argument interface and test helpers;
renewed current-source checks remain owed. Native and CLI fixtures have been
reviewed and integrated but not executed. No coverage or gate changes.

The earlier `integrated-eighth` archive passes all eleven focused audio checks.
The late-birth oracle now independently applies the retained half-open output
mask after EQ: raw label 33633 remains exhausted, and all 33,634 output samples
match. The unmasked FIR-tail control remains nonzero at that label. Audio
executable SHA-256 is
`dd2d85183b974c2f01fef6d934fb3444105e668d1d10519c2ebaf056a6cccf31`.
The separate `integrated-eighth-core` archive passes all 1,489 Core tests across
67 binaries, with none skipped. Its library executable SHA-256 is
`33f2f49d4f2b01313798732af67376de6bc6d7089ba098ccc3a2ea6ffd26aa2e`.
Both runs preserve the frozen identities of 2,037 Rust files and six inputs.
Label/trim continuity within a certified Split family is under further audit;
its two new fixtures currently fail setup and establish no result. Durable
Store, broader/strict, native and export qualification remain. This is still
an uncommitted core 56/store 85 candidate, with no coverage or gate change.

The earlier context-preserving candidate passes all 679 integrated Core library
tests; the new Split certificate and lifecycle checks still need execution.
The corrected partial-copy fixture requires entering its group
before slicing. Independent source reviews report no concrete findings. The
first processing-birth fix preserves the terminal raw sample and suppression
state, and all 30,430 stereo output samples match the explicit EQ oracle. Eleven
private route tests now pass, including Split-piece phase continuity and
retention of pre-birth losses.

The earlier `integrated-fifth` audio run passes six of nine checks, including
the ordered-journal authority positive and ten rejection cases. A standalone
[neutral Split witness](qualification/neutral-split-research-2026-10-23.md)
confirms a real failure with all controls valid: the
complete group input and 2,048-sample filter halo remain identical, but 7,581
output channel samples change, with maximum absolute difference
0.009660936455475166. The later-move witness cannot establish a separate defect
while that neutral-Split control fails. The late-birth fixture still fails its
setup assertion and establishes no audio result. The archived executable is
`9627184bb29f11f8bb49ee89445b33bfabc3d0744da9d62b87b6920c7f9e29c2`;
all 2,031 Rust source identities and six build inputs match their frozen hashes
before and after the run. Split context retention and final qualification remain
in progress; candidate core 56/store 85 are not committed or release-qualified.

The subsequent `integrated-seventh` run passes ten of eleven audio checks.
Neutral Split preserves all 24,024 stereo output samples, including EQ followed
by saturation, and Split followed by a move preserves all 25,626 samples and
the retained terminal input. Public permission-family authority and locality
checks pass. The late-birth fixture now establishes its controls: complete raw
input matches and lost label 33633 remains silent. Its EQ oracle expects
nonzero output only at that exhausted label, producing two channel differences;
the output-mask semantics of that oracle are under review. No repair success
is inferred from weakening the comparison. Executed audio SHA-256:
`51ee1ee328bea6db725f697dfa9f03e080a4fcaf6283253a07fa9a6f01cdca38`.
The separate eleven-check Plan archive passes with executable SHA-256
`3fe101c190d76183f3539ce5e03f5f89fdde635a23ed8126e2cf605c1e74c0d2`.
Both runs retain unchanged hashes for 2,037 Rust files and six inputs. New
Core lifecycle tests, broader/strict checks and vertical qualification remain.
No coverage percentage, requirement completion or gate changes.

[Research qualification](qualification/sound-scope-research-2026-10-23.md)
refutes manual journal-scope narrowing. A two-placement test loses retained
terminal sample 22422 from an existing ancestor EQ's input and changes 1,899
canonical PCM channel samples. The earlier 40 single-placement comparisons,
40 audible-effect controls and four JSON-admitted exterior crops pass. Complete
failure evidence is retained; production grouping guards remain unchanged.
The context-preserving candidate is not yet qualified as an edit. No requirement
or gate is newly complete; committed versions remain SQLite 84/core 55.

## Attached-sound endpoint grouping, 2026-10-22

[Qualification](qualification/sound-group-endpoints-2026-10-22.md) adds ordinary
`GroupSelection` endpoint Splits through the existing checked sound transport.
One complete pre-edit audio layout supplies both Original and independent sound
clocks. Aligned groups keep their existing tables and recipes byte-for-byte.
The group parent must remain outside every retained sound processing scope;
root-owned BeatSound transport and processing-interior ownership changes still
refuse. Existing partial copy, deletion, movement, pause and Repeat support is
unchanged.

All 14 focused Core/PCM/Store checks and the 97-check native debug replay pass.
The real-media preflight compares every one of 193,794 canonical samples and
limiter-gain bits through grouping, Undo/Redo and portable reopen, then verifies
all 121 encoded pictures and the complete audio window at zero measured offset.
All 4,904 broader and 1,292 UI-feature tests pass, as do strict ordinary/UI
checks and formatting. Release replays pass 97 visual and 82 performance checks
plus Kestrel in each mode; quiet input-to-state p99 is 1.420ms over 161 samples.
Final release export repeats complete canonical bit equality and verifies all
121 pictures and 193,794 samples at zero offset, minimum block SNR25.39dB.
The extracted nine-file owner project validates and passes doctor without
changing paths or bytes. Retained evidence records failed fixtures, the
test-only lint correction, exact source/binary identities, four inspected
captures, sparse commit latency and unobservable picture-neighbor comparisons.
No requirement or gate is newly complete. SQLite 84/core 55 stay unchanged.

## Corrected transcript-search selectors, 2026-10-22

[Qualification](qualification/transcript-search-selectors-2026-10-22.md) adds
`:select search="hello world"` in ordinary Your edit with Linked role. The
bounded raw query survives recording, named calls and fresh dot resolution.
Canonical provider-clock coverage proves complete forward Source phrases,
including continuous split seams and positive speed changes, before exact
inverse boundaries become a normalized interval set. Holds, gaps, reverse
segments, incomplete phrases and out-of-scope enclosures cannot contribute.
Copy captures one entry snapshot; Cut deletes its separate intervals atomically.

One qualified proposal/correction snapshot supplies both word navigation and
search within an invocation. Consuming publication checks its exact authority
in the same SQLite transaction, including register-only copies. Native entry,
completion and installed geometry retain the displayed snapshot and its absence.
Later analysis clears stale search highlights while historical Undo/Redo remains
usable. Pause/shot-only operations remain independent of word authority.

The focused candidate passes 124 checks, and the final broader run passes all
4,872 selected cases. Its passing output-handle warning also passes an isolated
run of the same retained executable with both pipes reaching EOF. All 1,292
UI-feature cases pass; 11 existing ignored model/playback/stress cases are
explicitly outside this checkpoint. Final strict checks and formatting pass.
Both release native modes pass 103 workflow checks plus Kestrel, with three
inspected compact captures and quiet input-to-state p99 1.580ms over 104 samples.
Release export verifies every one of 120 pictures and all 192,192 samples at
zero offset. The extracted nine-file owner package validates and passes doctor
without changing its paths or bytes. Failed candidates, layout warnings,
source/binary identities and canonical-audio limits are retained.
No requirement or gate is newly complete; remaining temporal and sound families,
mastering, renderer and complete workflows remain open. SQLite 84/core 55 stay
unchanged.

## Discontiguous interval unions, 2026-10-22

[Qualification](qualification/discontiguous-range-selectors-2026-10-22.md) adds
`:select ranges=[{"start":2,"end":6},{"start":10,"end":14}]` in ordinary
Your edit with Linked role. Signed pairs resolve from one entry cursor, validate
before allocation, and normalize into up to 64 separate positive half-open
intervals. Finished geometry has no enclosing hull. Recording retains relative
intent; named calls resolve afresh, and Visual cut-dot requires a new set.

One immutable entry capture supports atomic descending Cut, durable registers,
Undo/Redo and seam paste, including disjoint windows sharing one Source and one
logical mark with several destination bindings. Strict admission rejects mixed
component kinds and forged shared context. Scalar consumers, unsupported
interiors, looping, AI generation, saved framing and replacement refuse set
authority, including Place slice with an ordinary copied source. The read-only
inspector lists every interval and their summed duration.

All 106 focused checks, 4,596 broader cases and 1,281 complete UI-feature cases
pass, as do strict checks and formatting. Both release native modes pass 85
workflow checks plus Kestrel, with the compact three-range capture inspected.
Quiet input-to-state p99 is 2.265ms over 79 samples. The release export verifies
all 30 pictures and 48,048 samples at zero offset; the extracted nine-file owner
project validates and passes doctor. Failed candidates, exact source/executable
identities, layout warnings and canonical-audio limits are retained.

No requirement or gate is newly complete. Corrected transcript-search-result
selectors, remaining temporal and sound families, mastering, renderer and
complete workflows remain open. SQLite 84/core 55 are unchanged.

## Discontiguous whole-node unions, 2026-10-22

[Qualification](qualification/discontiguous-node-selectors-2026-10-22.md) adds
`:select nodes=["id-a","id-b"]` in ordinary Your edit with Linked role.
One bounded structural traversal orders up to 64 distinct whole members,
including empty siblings and different immediate parents. Copy captures one
entry-document union; Cut deletes in reverse order in one Compound. Seam paste
omits unselected gaps and allocates shared logical marks once. Immutable
capture admission, durable registers, Undo/Redo and reopen preserve the union.
Duplicate, ancestor-overlapping and unsupported temporal targets refuse.

The inspector is read-only and lists actual member identities and ranges.
Selection loops, saved framing, AI generation and unsupported consumers refuse
set authority, including stale single-Hold AI targets. Global AI cancellation
and independent sound audition remain available. SQLite 84 adds the closed
register kind under the existing unused-development-format authorization.

All 63 focused checks and 4,570 broader cases pass. Later Clippy cleanups have
separate frozen evidence: four affected Store and 46 native cases pass again,
with all 1,273 selected final UI-feature cases and strict checks. Both release
native modes pass 143 checks plus Kestrel. The compact main-viewer/inspector
capture was inspected, with its pending beat thumbnail explicit; quiet state
p99 is 1.977ms over 147 samples. The release export
verifies all 32 pictures and 51,251 samples at zero offset. The extracted
nine-file owner package validates and passes doctor. Failed setups/builds,
layout warnings, exact source/binary identities and canonical-audio limits are
retained separately; no independent PCM or full-mastering claim is made.

No requirement or gate is newly complete. Arbitrary positive half-open range
sets, transcript-search-result freshness, remaining temporal/sound families,
mastering, renderer and complete workflows remain open.

## Whole authored-node selectors, 2026-10-22

[Qualification](qualification/explicit-node-selectors-2026-10-22.md) adds exact
NodeId selection beneath ordinary Sequences in the captured Edit scope.
Selection, recording, named calls and dot retain identity independently of
cursor, selected beat or time geometry. Whole-child capture and mutation use
the actual immediate parent; empty siblings remain distinct. Native p/P
supports structural replacement, while Place slice and role-only consumers
explicitly refuse unsupported authority. Repeat/Retime interiors remain closed.

All 52 focused checks, 4,549 broader cases and 1,270 UI cases pass. Import
timeouts from the original Cargo deps location are retained and traced to
Foundation bundle discovery; verified Nextest archive runs preserve every case,
test/helper hash, timeout and concurrency. Strict checks pass. Each release
native mode passes 84 checks plus Kestrel; inspected captures retain target
identity and empty-node state. Quiet state p99 is 1.992ms across 79 samples.
The release export verifies 37 pictures and 59,259 samples at zero offset; the
extracted nine-file owner fixture validates and passes doctor. Layout warnings,
test setup failures and canonical-audio limitations remain explicit.

No requirement or gate is newly complete. Whole-node unions are qualified
above; arbitrary range sets, transcript-search-result selectors and the
remaining temporal/sound families remain open.

## Named-mark semantic range selectors, 2026-10-22

[Qualification](qualification/named-mark-selectors-2026-10-22.md) adds
`:select marks start=<exactMarkId> end=<exactMarkId>` in ordinary Your edit.
Recorded selection, copy/cut, Group, Duplicate, Repeat and dot retain mark
addresses and resolve each staged document. Missing, ambiguous, wrong-clock,
reversed, collapsed and out-of-scope endpoints refuse without a fallback.
Existing temporal and one-child role constraints remain intact. Selecting alone
changes neither authored history nor registers; consuming runs keep atomic
capture, bank and Undo behavior.

Eight named-mark core cases pass. The broad run passes 4,531 of 4,532 cases;
its existing import timeout and passing output-handle marker both pass exact
isolated runs of the retained binaries. After internal carrier boxing, all
1,220 normal app and 1,266 UI-feature checks pass without either anomaly.
Three final test-only allocation cleanups pass their affected cases in both
builds. Strict checks and formatting pass.

Both release native modes pass 95 checks plus Kestrel, with an inspected compact
capture. Quiet state p99 is 2.174ms over 94 samples. The real-media release
workflow verifies all 33 pictures and 52,853 presented samples in one complete
zero-offset audio window without failures or flags. The extracted nine-file
owner package validates and passes doctor. Sources, binaries, failed candidates,
layout warnings, reports, MP4 and owner archive are retained.

DP-06 and DP-21 remain partial; no requirement or gate is newly complete.
Explicit node, search-result and discontiguous selectors, remaining temporal
families, sound owners, mastering, renderer work and complete workflows remain.

## Selected-play Hold lengths through one fixed Retime, 2026-10-22

[Qualification](qualification/selected-fixed-retime-hold-duration-2026-10-22.md)
extends selected body batches through one fixed, unclocked Edit Retime below
all enclosing Repeats. Either axis may supply the selected Plays; every other
level retains literal Default/Play authority. Every member, including no-ops,
is checked before allocation, and staged proofs preserve fixed extent, audio
phase and cropped primary continuation. Inner scalar scope choices preserve
fresh outer selections; replacing the selected axis and rejecting structurally
incompatible counterparts remain atomic.

The new accepted-media regression also fixes Undo ownership: intents born on
removed clones close as Deleted instead of acquiring the shared Hold identity.
Writer admission and chronological replay use the same complete-address rule.
Original authority, historical request addresses and fresh Redo authorizations
are preserved without weakening uniqueness or changing the schema.

All 3,528 broader checks pass. After one test-only Copy cleanup, all 211
affected normal-app and 213 UI-feature checks pass again; production Rust is
identical to the passing broad candidate. Strict checks and formatting pass.
Both release native modes pass 219 workflow checks plus Kestrel; four final
compact captures were inspected. Quiet state p99 is 1.665ms over 652 samples.
Sixteen release exports verify 1,312 pictures and 2,101,304 presented samples
across 40 complete zero-offset windows without failures or flags. Five owner
archives validate after safe extraction. The affine local-block minimum remains
12.008dB, above the unchanged 1.5dB limit; listening and full mastering stay open.

Evidence retains the valid old-code control, all failed candidates and their
corrections, exact sources/binaries, output-handle markers and isolated passing
runs, layout warnings, MP4s and owner packages. DP-06 and DP-07 remain partial;
no requirement or gate is newly complete. Named-mark selectors, remaining
temporal families, sound owners, mastering, renderer work and complete workflows
remain open.

## Mixed All plays/This play Hold length through one fixed Retime, 2026-10-22

[Qualification](qualification/mixed-fixed-retime-hold-duration-2026-10-22.md)
extends literal mixed body choices through one fixed, unclocked Edit Retime
below every enclosing Repeat. Exact temporal isolation preserves Default
families, stable Plays, fixed output, cropped sampled continuation and physical
audio phase. Owned setters allocate no new identities. Existing commands,
contexts, schema and ordinary mixed owned-gap/dormant behavior remain intact.

All 48 focused and 3,515 broader tests pass, including 206 affected normal app
checks; all 208 UI-feature checks pass. Strict checks and formatting pass.
Both release native runs pass 115 workflow checks plus Kestrel. Two final
compact captures were inspected; quiet state p99 is 1.622ms over 332 samples.
Eight release exports verify all 656 pictures and 1,050,652 presented samples
across 20 complete zero-offset windows without failures or flags. The affine
Default/Play growth case retains its measured 12.008dB local-block SNR minimum,
above the unchanged verifier limit; acoustic and full mastering claims stay open.
Evidence retains the valid counterfactual witness, exact source/binary identities,
independent PCM, accepted-media history, compile fixes, isolated passing checks
for two captured-output markers, layout warnings and three validated owner packages.
DP-06 and DP-07 remain partial; no requirement or gate is newly complete.
Selected fixed-Retime batches, wider selectors, processing clocks, Partition
and sound lifecycles remain open.

## Nested This play Hold length through one fixed Retime, 2026-10-22

[Qualification](qualification/nested-fixed-retime-hold-duration-2026-10-22.md)
extends explicit body choices through bounded Repeat ancestry, with one fixed,
unclocked Edit Retime below the innermost Repeat. Actual outside-in proofs retain
mapped Holds, stable plays, fixed extent, cropped visibility and physical audio
phase. Partly owned paths copy only remaining shared branches. Saved instructions
and context schemas are unchanged. Mixed/selected choices and other temporal
placements retain their separate guards.

All 46 focused and 3,503 broader tests pass, including all 205 affected normal
app checks; all 207 UI-feature checks pass. Strict checks and formatting pass.
Both release native replays pass 59 workflow checks plus Kestrel; two compact
captures were inspected. Quiet state p99 is 1.645ms over 172 samples. Four
release exports verify all 328 pictures and 525,326 presented audio samples
across ten complete zero-offset windows, without failures or flags. Retained
evidence includes the valid old-code witness, two/three/five-level and sparse
ownership cases, independent PCM, admitted-provider history, failed test setups,
exact sources/binaries, output markers and layout warnings.
DP-06 and DP-07 remain partial; no requirement or gate is newly complete.
Mixed/selected fixed-Retime lengths, wider selectors, processing clocks,
Partition and sound lifecycles remain open.

## This play Hold length through one fixed Retime, 2026-10-22

[Qualification](qualification/this-play-fixed-retime-hold-duration-2026-10-22.md)
adds duration-only authority for one explicit Repeat body containing one fixed,
unclocked Edit Retime. Actual temporal isolation maps the selected Hold and its
presentation; the ordinary reducer preserves the fixed window and audio phase.
Already-owned branches reuse identities. Cropped starts continue at the first
sampled visible picture without inventing an intrinsic boundary. Hidden,
unsampled, mixed/selected, Partition, derived-clock and unsupported sound
paths retain their guards. The inspector explanation follows the active scope.

All 39 focused and 3,496 broader tests pass. All 204 affected normal app tests
and 206 UI-feature tests pass, with strict lint and formatting. Both release native replays
pass 56 workflow checks plus Kestrel; two compact captures were inspected.
Quiet state p99 is 1.828ms over 151 samples. Four release exports verify every
one of 160 pictures and 256,256 presented samples at zero offset. Retained
evidence includes exact sources/binaries, actual MP4s, independent PCM,
admitted-provider history, failed fixtures, source-freeze and script repairs,
layout warnings and isolated passing checks for captured-output markers.
DP-06 and DP-07 remain partial; no requirement or gate is newly complete.
Mixed and selected fixed-Retime durations, wider selectors,
processing clocks, Partition and sound lifecycles remain open.

## All plays Hold length through fixed Retimes, 2026-10-22

[Qualification](qualification/shared-fixed-retime-hold-duration-2026-10-22.md)
extends shared duration authority through fixed unclocked Edit Retimes below a
direct Repeat inspector root. One exact visibility projection composes crops,
scales and stable sparse Repeat offsets, then selects picture centers. Fixed
windows and independent owned plays remain unchanged. Hidden and unsampled
live Holds refuse, and cropped visibility never proves dormancy. No saved
instruction, wire format, schema or raw reducer changes.

All 23 focused, 3,480 broader and 202 final UI-feature tests pass, with strict
lint and formatting. Retained captured-output markers pass individually from
their exact binaries; causes remain unknown. Both final release native replays
pass 60 workflow checks plus Kestrel, including recording, saved Run, dot,
history, delayed receipts and reopen. Two 960×640 captures were inspected.
Quiet state p99 is 1.724ms across 134 samples. Four release exports verify all
160 pictures and 256,256 presented samples at zero offset. The qualification
retains failed fixtures/replays, exact clock-ID comparisons, warnings and
source/binary identities. DP-06 and DP-07 remain partial; no requirement or gate
is newly complete. Mixed and selected fixed-Retime durations,
processing clocks, Partition, wider selectors and sound lifecycles remain open.

## Owned-gap Hold duration semantics, 2026-10-22

[Owned-gap qualification](qualification/owned-gap-hold-duration-semantics-2026-10-22.md)
extends the existing mixed and selected duration authorities to explicit owned
gaps, including final dormant definitions. Complete Default/Play ancestry,
current stable iteration position, actual isolation and exact staged identity
pools preserve every counterpart. A dormant primary retains the ordinary cursor
while live counterparts may ripple; no trailing gap or invented presentation is
created. Accepted-provider preparations and partial 44.1 kHz audio retain their
own clocks through one history entry, Undo, Redo and reopen.

The valid old-code entry witness fails. All 18 focused and 3,465 broader tests
pass; final affected normal/UI runs pass 199/201 tests. Strict checks, formatting
and the generated command reference pass. Both native release replays pass
27 owned-gap checks plus Kestrel; two compact captures were inspected. Quiet
input-to-state p99 is 1.733ms over 45 samples. Six release exports verify all
411 pictures and 658,257 presented audio samples at measured offset zero.
Retained failures include incorrect native fixture navigation, an out-of-range
fixture cursor, accepted-request controls and fractional split/window oracles;
the final PCM oracle checks complete pre-split Source lattices without skipping
extra samples. A single all-Play dormant gap, implicit default-gap node targets,
Repeat/Retime combinations, attached sounds and wider selectors remain open.
DP-06, DP-07 and DP-24 stay partial; no requirement or gate is newly complete.

## Selected-play Hold duration semantics, 2026-10-21

[Selected duration qualification](qualification/selected-hold-duration-semantics-2026-10-21.md)
adds one atomic duration edit for 2..=1024 captured Hold bodies across one
selected Repeat axis, retaining later Default/Play choices. Actual staged
isolation maps every counterpart and the final primary cursor; no-op members
allocate nothing, and opposite changes can leave total duration unchanged.
Native controls, recording, calls and direct-edit dot use invocation-relative
length intent. Browsing revokes mapped selection, requiring explicit reselection.
The valid old-code entry witness fails; all 15 focused, 3,446 broader,
196 affected app and 198 UI-feature checks pass, with strict lint and the
generated command reference. Both final release replays pass 1,318 scoped
checks plus Kestrel; three compact captures were inspected. Quiet input-to-state
p99 is 1.418ms. Four release exports verify all 240 pictures and 384,384 audio
samples at measured offset zero. Retained compiler, fixture, capture and replay
corrections include the intentional mapped-selection revocation. At that
checkpoint, mixed/selected owned gaps, Repeat/Retime combinations and wider
selectors remained open; owned-gap duration is qualified above.
DP-06, DP-07 and DP-24 stay partial; no requirement or gate is newly complete.

## Mixed Repeat Hold duration semantics, 2026-10-21

[Mixed duration qualification](qualification/mixed-hold-duration-semantics-2026-10-21.md)
adds native Hold lengths with explicit All plays and This play choices in the
same body path. Actual temporal isolation maps the authoring target and its
stable presentation; Default choices retain their shared meaning. Dormant
definitions still isolate when needed without moving either clock or picture.
Generation requests, accepted coverage and fallback preparations follow the
same proof through durable history. Recording, calls and direct-edit dot retain
only invocation-relative length intent. Two valid native old-code witnesses
fail; all 15 focused, 3,431 broader, 759 final affected and 197 UI-feature tests
pass, with strict checks and the generated command reference. Concurrent-run
captured-output warnings and isolated passing follow-ups remain recorded.
Both final release replays pass 1,222 scoped checks plus Kestrel; six compact
captures were inspected. Quiet input-to-state p99 is 1.854ms. Release exports
verify all 240 pictures and all 384,384 presented audio samples at measured
offset zero. At that checkpoint, several-play durations, mixed gap timing
and Repeat/Retime temporal combinations remained open. DP-06, DP-07 and DP-24
stay partial; no requirement or gate is newly complete.

## All plays Hold duration semantics, 2026-10-21

[Shared duration qualification](qualification/shared-hold-duration-semantics-2026-10-21.md)
adds native duration controls, recording, named calls and direct-edit dot for a
Hold with All plays explicit at every Repeat level. Independent owned plays
retain their recipes and physical clocks. A live shared default continues at
the new start of its same stable representative; a positively proven dormant
default changes its recipe without inventing a play or moving the cursor.
Two valid old-code native witnesses fail; all 14 focused, 712 broader and 193
UI-feature tests pass, with the generated command reference and strict lint.
The retained first-run core fixture identity collision is corrected without
production changes. Both final native replays pass 1,069 scoped checks plus
Kestrel; four compact captures were inspected. Quiet input-to-state p99 is
1.619ms. Release exports verify all 120 pictures and all 192,192 presented
audio samples at measured offset zero. Replay capture, history-oracle,
instruction-hint and trace-capacity corrections retain their failed evidence.
At that checkpoint, mixed choices, several-play timing and Retime ancestry on
Repeat duration paths remained open. DP-06, DP-07 and DP-24 stay partial; no
requirement or gate is newly complete.

## Native Hold lengths inside fixed Retime, 2026-10-21

[Native duration qualification](qualification/native-fixed-retime-hold-2026-10-21.md)
exposes ordinary Hold resizing inside fixed unclocked Edit Retime windows through
Enter, Change duration, command entry, recording, named calls and dot. Exact
half-open crop visibility determines the first sampled continuation frame.
Independent output sounds, fixed stage windows and the Original cursor retain
their clocks. Repeat ancestry, derived pause clocks, Visual ranges and hidden
or unsampled Holds retain explicit refusals. Rendering reads cached inspector
geometry; command capture and commit perform full validation. Two valid native
old-code witnesses fail before implementation; all 15 focused, 698 broader and 190
UI-feature checks pass, as do the command reference and strict lint. DP-06,
DP-07 and DP-24 remain partial; no requirement or gate is newly complete.
Both final release replays pass 958 scoped checks plus Kestrel. The inspected
960×640 captures retain visible controls and footer; quiet input-to-state p99
is 1.454ms and maximum input-to-picture completion is 94.903ms. A follow-up
144-test app selection and strict lint pass after separating framing authority
from duration continuation. The release nonzero master/export continuity check
verifies all eleven pictures and 17,618 audio samples at zero measured offset.
Two mistaken dot-replay assumptions and the corrected ordinary scope description
remain recorded with their source and binary evidence.

## Fractional retained audio-owner support, 2026-10-21

[Owner-support qualification](qualification/fractional-owner-support-2026-10-21.md)
repairs the nonzero nonlinear master refusal at sample8236. Retained intervals
may cross more than one reference point before their owner's first allocation;
the exact signed clock and rate remain unchanged. Four planner and eight PCM
checks pass, including negative Hold gain/envelope clocks and genuine exhausted
support. All 1,234 broader plan/audio/playback tests pass with zero skips.
The complete real nonzero-Original master matches its independent equation
within 3.01e-8; release export verifies all eleven pictures and all17,618 samples
in two contiguous windows at offset zero, minimum block SNR31.05dB.
Source admission remains strict; retained fixture/compiler/lint corrections do
not change production media or DSP behavior. Native Retime Hold editing was still
gated at that checkpoint; the native duration qualification above exposes its
ordinary fixed-window path. DP-07 and DP-09 stay partial; no requirement or gate
is newly complete.

## Fixed Retime output sounds through Hold edits, 2026-10-21

[Output-sound qualification](qualification/fixed-retime-output-sound-2026-10-21.md)
permits ordinary interior Hold resizing while an independent sound retains its
fixed Retime output clock, exact phase, sample offset and durable journal.
Child-owned changed-input dependencies still refuse atomically. Six valid
positive old-code witnesses fail before implementation. All thirteen focused
core, six PCM and two real-package store tests pass; all 3,130 broader
core/plan/audio/store tests and final strict lint pass. The release export
verifies all eleven pictures and 17,618 presented samples in two contiguous
windows at offset zero; its independent event oracle has zero numerical error.
Native Hold editing under Retime was still gated at that checkpoint. It retains a
separate pre-existing nonlinear owner-support refusal at sample8236; the
[subsequent owner-support qualification](qualification/fractional-owner-support-2026-10-21.md)
repairs it and verifies the complete nonzero master. DP-07 and DP-09 stay
partial; no requirement or gate is newly complete.

## One-play literal audio setters, 2026-10-21

[Setter qualification](qualification/scoped-audio-setters-2026-10-21.md) adds
absolute trim, saturation, balance and typed EQ to recording, calls and dot
with explicit This play at every Repeat and no Visual range. Literal values
and stage removals replace only their named treatment. Exact isolation,
automation, nonempty sends, timing and durable history remain intact.
Four valid old-code witnesses fail before implementation. All 23 focused core,
514 normal and 207 UI-feature tests pass, with strict lint and release builds.
The new release export verifies all 72 pictures and all 115,315 presented audio
samples in six contiguous windows at measured offset zero; a separate numerical
PCM oracle checks all six occurrences. Both final native replays pass 846
scoped checks plus Kestrel. Three compact captures were inspected; input-to-state
p99 is 1.388 ms and maximum picture completion is 93.165 ms in the quiet run.
Two existing replay setup assumptions were repaired without changing production
behavior; their failures and independent follow-up reviews are retained.
DP-06, DP-09 and DP-24 stay partial. Visual ranges, interactive drafts, sends
and mixed/multiple-play macro authority remain outside this scoped path.

## One-play mute macros, 2026-10-21

[Mute qualification](qualification/scoped-mute-macros-2026-10-21.md) adds
explicit whole-owner mute state to recording, calls and dot with This play at
every Repeat level and no Visual range. Saved actions apply their chosen on/off
state without toggling, preserving other treatments, picture timing and cursors.
Six new core tests and two real-package native tests pass. The normal selection
passes 490 cases plus the refreshed command-reference check; all 202 UI-feature
tests pass. Two output-handle diagnostics have clean serial reruns, with their
cause unproven. Final strict lint and release builds pass. Both release replays
pass 735 scoped checks plus Kestrel. Compact captures exposed and then verified
the footer-hint fix. Input-to-state p99 is 1.379 ms; maximum picture completion
is 92.858 ms. Initial failures and exact evidence are retained. DP-06, DP-09 and
DP-24 remain partial; wider scoped audio, Visual and mixed/multiple-play macro
authority remain open.

## One-play gain macros, 2026-10-20

[Gain macro qualification](qualification/scoped-gain-macros-2026-10-20.md)
adds relative whole-owner gain steps to recording, named calls and dot with
explicit This play at every Repeat level. Each instruction reads its staged
target's current gain, preserves other treatments and timing, and carries actual
isolation into subsequent instructions. Thirty focused core tests pass. The
normal selection passes 466 cases plus a corrected no-edit test's targeted
rerun; all 186 UI-feature tests pass. Strict normal/UI lint passes. One
output-handle diagnostic has a clean isolated rerun; its cause remains unproven.
Final release visual/performance replays each pass 671 scoped checks plus
Kestrel, and both 960×640 gain captures were inspected. The guard-bypass review
finding, zero-step fixture failures and stale-footer correction remain retained.
Input-to-state p99 is 1.319 ms; the 93.465 ms picture-completion maximum remains
recorded. DP-06, DP-09 and DP-24 stay partial; broader occurrence audio changes,
Visual and mixed/multiple-play macro authority remain open.

## Exact speed updates around an inner Repeat, 2026-10-20

[Inner-Repeat qualification](qualification/inner-repeat-speed-2026-10-20.md)
extends exact-pause tape-speed updates to one inner Repeat level, including
owned plays and independent RoomTone gaps. Every physical carrier keeps its
historical clock and filtering stage; bounded admission checks all old and
born-play rate classes. Missing carriers capture their pre-edit entries once.
Core, PCM, inverse and serialization checks pass. The broad run records 2,726
passes and two unrelated failures; both pass serially after a disk-full fixture
repair, with strict lint passing again. The release export passes all 22
pictures and the complete 35,235-sample audio window at measured offset zero,
without verification flags. The original failures and exact results remain
retained. DP-07 stays partial: nested Repeat and processing families, fractional
own extents, non-Sequence ancestors and broader Section 8 work remain open.

## Selected audio crops after exact speed changes, 2026-10-20

[Crop-carrier qualification](qualification/source-crop-carriers-2026-10-20.md)
admits selected Source audio whose meaningful historical sample domain is
empty, including fresh captures and either offset sign. A retained silent
Hold now returns exact suppressed zeros without an unused transfer, fixing
the failure exposed by an enclosing PCM read. Nonempty Source and RoomTone
controls retain the strict rate refusal. Eighteen focused core/semantic and
nine PCM checks pass, including intrinsic support, disjoint crops, partition
invariance, provider isolation, public update and inverse. Formatting, strict
Clippy and all 2,375 core/plan/audio tests pass, with none skipped. The existing
shorter/longer release exports pass all 53 pictures and complete zero-offset
audio windows. The extreme crop case uses bounded PCM reads, not a full export.
DP-07 remains partial. One inner Repeat level is now covered above; nested
Repeat and processing families and the broader Section 8 work remain open.

## Speed changes after exact pauses, 2026-10-20

[Speed-update qualification](qualification/clocked-speed-update-2026-10-20.md)
adds in-place tape-speed changes for an eligible ordinary Retime after exact
pause insertion. Native preview, inspector and semantic macros resolve the
exact selected input, preserve descendant pause clocks and retain historical
audio carriers. Transfer admission uses the actual old lattice, including
successive updates and empty-carrier controls. Thirteen core/semantic, four PCM,
two store and ten native/inspector checks pass, with strict Clippy and 2,807
selected regressions. Both release exports verify all 53 pictures and complete
audio at offset zero. Final visual/performance replays each pass 52 checks plus
Kestrel; the compact capture was inspected. Input-to-state p99 is 1.330 ms;
the 87.886 ms picture-completion maximum remains recorded. The replay-focus
failure and dependency-update reruns remain retained. DP-07 stays partial: fractional
own extents, nested Repeat or processing families, non-Sequence ancestors and
changed-processing-subtree sound transport remain outside this increment.

## Hold clocks inside processing inputs, 2026-10-20

[Processing-input qualification](qualification/processing-hold-clocks-2026-10-20.md)
retains moved Source, Repeat-play and RoomTone-gap clocks when an ordinary Hold
resize changes the operand of a fixed Preserve/Shift stage. Existing output
bindings remain unchanged while canonical DSP rebuilds the current input.
Seven core tests, 18 exact PCM tests, six affine controls and a durable-history
matrix pass. Formatting, strict Clippy and all 2,661 selected regressions
pass. Four release exports verify all 100 pictures and four complete audio
windows at measured offset zero, without verification flags. The wrong journal
fixture and its atomic-refusal boundary remain recorded. DP-07 stays partial;
changed-processing-subtree sound transport, general speed-stage updates and
broader Section 8 work remain open.

## Hold clocks inside fixed Retime windows, 2026-10-20

[Fixed-window qualification](qualification/fixed-retime-hold-clocks-2026-10-20.md)
retains moved interior Source and Repeat clocks when an ordinary Hold resize
leaves the root duration unchanged. Affine windows, outside placements and
unused default bindings retain their authored behavior. Six core, four exact
PCM and one durable-store regressions pass. Formatting, strict Clippy and
2,635 selected tests pass, followed by a fixture-only lint check and two real
release exports: all 54 pictures and both complete zero-offset audio windows
pass without verification flags. Cropped-filter, exhausted-allocation and
incomplete-fixture failures remain retained with their corrections. DP-07 stays
partial. Ordinary Preserve/Shift input clocks are now covered by the
[processing-input qualification](qualification/processing-hold-clocks-2026-10-20.md);
general speed-stage updates and broader Section 8 work remain open.

## One-play Hold sound, 2026-10-20

[Hold sound qualification](qualification/scoped-hold-audio-2026-10-20.md)
adds Silence and exact prepared RoomTone to recording, named calls and dot
with explicit This play at every Repeat level. Staged edits follow the actual
isolated Hold and preserve the absolute cursor, picture and duration.
Five focused core tests, exact fractional-rate PCM and real qualified store
admission pass. Formatting and strict normal/UI lint pass, alongside 458 normal
and 220 UI-feature assertions. One output-handle diagnostic has a clean isolated
rerun; its cause remains unproven. Final release visual/performance replays each
pass 608 scoped checks plus Kestrel, and both compact captures were inspected.
Two replay scroll/cleanup failures and their corrections remain retained.
Input-to-state p99 is 1.420 ms; the 94.388 ms picture-completion maximum remains
recorded. DP-06, DP-07 and DP-24 stay partial; broader occurrence edits, mixed
macro authority and performance qualification remain open.

## Ordinary Hold clocks, 2026-10-20

[Hold clock qualification](qualification/hold-duration-clocks-2026-10-20.md)
retains downstream sample clocks when an ordinary Hold duration changes,
including initially unbound Sources and shared or owned Repeat plays.
A failing 30000/1001, 44.1 kHz witness exposed a 147/400-sample phase reset;
the corrected path keeps the old source entry and exact retained PCM.
Silent owners preserve existing bindings without accumulating unused captures,
so a 257-Hold atomic resize stays within the existing work budget.
Formatting, strict Clippy and all 2,617 selected core/audio/store and CLI tests
pass cleanly. The reproduced failures and focused reruns remain in the record.
Two real release exports verify all 120 pictures and four complete zero-offset
audio windows, with no level, silence or SNR failures. A fixture-only amplitude
assumption was corrected without changing PCM comparisons or verifier gates.
DP-07 remains partial. Hold resizes inside fixed affine Retime windows are
covered by the separate [fixed-window qualification](qualification/fixed-retime-hold-clocks-2026-10-20.md).

## One-play Hold duration, 2026-10-20

[Hold duration qualification](qualification/scoped-hold-duration-2026-10-20.md)
adds exact existing-Hold lengths with explicit This play at every Repeat level.
Enter and Change duration capture the same target; recording, dot and named
calls continue through its actual isolation mapping at the resized Hold's
start. A failing fractional-rate PCM witness exposed a downstream phase reset;
old-clock capture now preserves later Sources, plays, gaps and unbound suffixes.
Strict normal/UI checks pass 234 selected tests; separate UI and existing-path
selections pass 103 and 57. Two receipt output-handle diagnostics have clean
individual reruns. Both real release exports verify all 120 pictures
and four full-coverage audio windows at zero measured offset. Final visual and
performance replays each pass 444 scoped checks plus Kestrel, and all three
compact captures were inspected. The 93.848 ms picture-completion tail
remains recorded. DP-06 and DP-07 stay partial: broader occurrence edits,
mixed/multiple-play macro authority, scoped AI and Retime timing remain open.

## Single-play pause macros, 2026-10-20

[Pause macro qualification](qualification/scoped-pause-macros-2026-10-20.md)
adds freeze/black recording and dot with explicit This play authority at every
Repeat level. Saved intent retains no concrete addresses or sampled picture;
each invocation resolves its current exact boundary and retains the actual new
Hold for subsequent pause, Zoom or named-call instructions. Native receipts
preserve the ordinary context, and navigation revokes delayed continuation.
Thirteen focused core and ten native/CLI cases pass, as do strict normal/UI
checks with 211/101 selected tests. Two output-handle diagnostics have clean
isolated reruns. Final release visual/performance replays each pass 312 scoped
checks plus Kestrel; all three new compact captures were inspected. The
109.054 ms picture-completion tail remains recorded. DP-06 and DP-07 remain
partial: broader occurrence edits, mixed/multiple-play macro authority, scoped
AI and Retime timing remain open.

## Mixed and selected-play pauses, 2026-10-20

[Mixed pause qualification](qualification/mixed-repeat-pauses-2026-10-20.md)
adds independent Default/Play choices at each Repeat level and atomic pause
insertion across selected plays. Each play retains its own sampled picture,
framing and exact suffix clocks. Native receipts preserve the actual new Holds
across unequal child slots and independently inferred inner plays, including
subsequent pause and gain edits. Navigation generations revoke delayed
continuations through keyboard, pointer, pane, view and analyzed-word/search
round trips. Initial qualification passes all 1,874 core/store and 1,220 app UI
tests; a store output-handle diagnostic has a clean isolated rerun. Strict normal/UI lint and 136 selected
tests pass. Two real release exports verify all 135 pictures and four zero-offset
audio windows. Visual/performance replays pass 226/226 scoped checks plus Kestrel;
all five new compact captures were inspected. DP-07 remains partial; broader
occurrence edits, AI batches, Retime scopes and payloads remain open.

After integration with concurrent main `8a6aeb43`, strict normal/UI checks and
161 selected normal tests pass, as do 98 selected UI tests. One UI output-handle
diagnostic has a clean isolated rerun. Both real exports again verify all 135
pictures and four zero-offset audio windows. Final release visual/performance
replays each pass 226 scoped checks plus Kestrel, with all five compact captures
inspected. Separate integration evidence preserves the source and binary hashes
and the recorded 99.188 ms picture-completion tail.

## One-play framing macros, 2026-10-18

[One-play framing qualification](qualification/one-play-framing-macros-2026-10-18.md)
adds whole-owner Zoom/Creep recording and dot with explicit current Play
authority at every Repeat level. Saved intent retains no concrete owner or play
IDs. Staged isolation maps both authoring and presentation addresses, preserves
intrinsic owner and Follow clocks, and publishes durable native scoped receipts.
Scope navigation permanently revokes delayed continuation ownership, including
same-batch away/back navigation. Normal and UI selections cover 448 and 474
tests; generated-reference and output-handle diagnostics have clean targeted
reruns. Final strict checks pass. Release visual/performance replays each pass
150 checks plus the Kestrel audit, and all eight final 960×640 captures were
inspected. DP-06 and DP-21 remain partial: broader placement, selectors, parser
acceptance and general occurrence edits remain open. Sampled-target recording
and unsupported scoped instructions refuse without writes.

## Ranged framing macros, 2026-10-18

[Ranged framing qualification](qualification/ranged-framing-macros-2026-10-18.md)
adds ordinary invocation-relative Zoom/Creep intent and dot-repeat. Each staged
instruction resolves the current owner and exact range, with retained-center
or projected Original-center intent and explicit sampled-target/path refusals.
Native and semantic commands share the pure builder; qualified metadata and
the render plan provide geometry without decoding or proxies. Pending framing
cannot survive Escape or save an incomplete recording. Scoped normal and UI
configurations each pass 111 tests, including actual accepted generated media,
dry-run parity, late atomic refusals and durable Undo/Redo. Final
release visual/performance replays each pass 136 checks plus the Kestrel audit;
all seven retained 960×640 captures were inspected. DP-06 and DP-21
remain partial; broader placement, selectors and parser acceptance remain open.
Subsequent one-play authority is recorded above. Sampled-target recording refuses as specification §7.4
requires.

## Place slice macros, 2026-10-18

[Place slice macro qualification](qualification/place-slice-macros-2026-10-18.md)
adds recorded Apply for exact register paste/replacement proposals, with
literal sibling slots, captured range/Object identity and one semantic
transaction. Unsupported destinations, Move and refined endpoints refuse
without writes or closing the draft. Replay resolves the new invocation target.
Real-media tests compare decoded pictures, complete framing clocks and exact
canonical PCM at both joins; one Undo restores the authored baseline.
Scoped normal/UI configurations each pass 89 tests and strict lint. Final
release visual/performance replays each pass 904 checks plus the Kestrel audit;
all five retained minimum-size captures were inspected. DP-06 and DP-21 remain
partial, including broader placement intent, ranged/one-play framing, remaining
selectors and parser acceptance.

## Original mark macros, 2026-10-18

[Original mark qualification](qualification/original-mark-macros-2026-10-18.md)
adds active-clock frame/start/end motion, letter marks, jumps and named calls on
the qualified measured Original. Programs retain intent; replay binds the new
invocation's exact PTS and preserves the independent Edit context. Native jump
history, coalesced headless mark saves, nested inspection and Escape cancellation
retain their ownership. The scoped 181-, 67- and final 27-test selections pass;
the final UI-harness selection also passes 27 tests. Release visual/performance
marks replays each pass 203 checks, and the broader macro replay passes 401,
with the Kestrel audit. Full-size captures and the evidence inventory were
reviewed. These overlapping selections do not constitute a full workspace gate.
DP-06 and DP-21 remain partial; broader Place slice placement, ranged/one-play framing,
broader selectors and the other tracked requirements remain open.

## Standalone pause detection sleep interruption, 2026-10-17

[Detection qualification](qualification/activity-sleep-2026-10-17.md) extends
sleep interruption to standalone speech activity through its own lifecycle
capture. Queued/running detection stops, stale output and saves are rejected,
and retry requires wake and worker drain. Saved words and corrections remain
available. All 56 scoped tests and both strict Clippy configurations pass.
A real Silero run cancelled after its model-loaded notification drains in
514.775 ms, then completes a fresh retry. Final visual/performance transcript
replays each pass 60 checks; full warning/retry paint was inspected and native
Metal/window startup and shutdown pass. Physical sleep delivery, other
analysis workers and the remaining DP-18 work stay open.

## Transcription sleep interruption, 2026-10-17

[Transcription sleep qualification](qualification/transcription-sleep-2026-10-17.md)
adds lifecycle capture to initial recognition and Recognize again. A scoped
watcher cancels native work without UI callbacks; wake does not resume it.
Delayed results and queued transcript/sibling speech-activity saves retain
the old epoch and are refused, with transactional rollback before publication.
Existing annotations, corrections and edit history remain intact. The scoped
gate passes 55 tests and both strict Clippy modes. An actual Whisper Metal run
interrupted at 25% progress drains cancellation in 356.724 ms, then completes
a fresh retry. Final release visual/performance transcript replays each pass
58 checks; inspected keyboard retry is fully visible, and native Metal/window
startup and shutdown pass. Physical sleep delivery, standalone analysis workers and the
rest of DP-18 remain open.

## Real inference/playback interference, 2026-10-17

[Interference qualification](qualification/inference-interference-2026-10-17.md)
records actual app-owned Extension inference alongside device audio and full
offscreen Metal presentation. Paired baseline/inference runs each cover six
five-second loops at 1080p60 and 4K30, with every picture present and zero
underruns, faults, silent padding or proxy pictures. Production scheduling
acknowledges the pause after about 15.5 seconds while playback continues,
then resumes inference. Both attempts cancel durably without accepting media.
Each release run passes 22 checks plus the shortcut audit; nine focused tests
and both strict Clippy modes pass. Request-time ticket/lap attribution prevents
stale pictures from inflating coverage. Seek/split/undo smoke samples are not
full latency percentile qualification. DP-18 and DP-24 remain partial; broader
effects, render interference, other models and stress workloads remain open.

## Model memory admission, 2026-10-17

[Memory qualification](qualification/memory-admission-2026-10-17.md) records
app-owned generation and transcription admission from the selected pack's
estimate, observed macOS headroom and an OS/editor reserve. Missing or
insufficient capacity cannot be overridden; held jobs remain cancellable and
ordinary edits still commit. Model release invalidates old and in-flight
observations. The tightened policy passes 60 scoped tests and both strict
Clippy modes, following the initial 198-test integration selection.
A complete maximum-shape Extension measured a 17.089 GiB sampled worker
physical-footprint peak across 1,221 samples without sampling failures;
independent media qualification passed for 81 native and 72 output frames.
Final release visual/performance Jobs replays each pass 41 checks; the complete
held reasons, Cancel actions and lack of an override were visually inspected.
Native Metal/window startup and shutdown pass.
This is one measured input, not a universal memory bound. Broader model
envelopes, runtime allocation limits, interference measurements and the rest
of DP-18 remain open.

## Cooperative AI inference scheduling, 2026-10-17

[Scheduling qualification](qualification/inference-scheduling-2026-10-17.md)
records app-owned generation yielding to playback and rendering at supported
MLX boundaries, acknowledged pause state, bounded controls, retained model
ownership and cancellation while paused. Paused computation time is accounted
separately from cancellation deadlines. The 259-test scoped gate, strict Clippy
and 134 Python tests pass. A real Extension run paused, resumed denoising and
cancelled from a second pause in 157 ms without publishing a candidate or
changing the edit. A separate complete generation reached verified Ready with
the exact 12-frame output and unchanged authored document. Final release
visual/performance Jobs replays each pass 38 checks, and native Metal/window
startup and shutdown pass. Playback interference, broader memory-envelope qualification, other model workers
and the rest of DP-18 remain open; the subsequent memory admission work is recorded above.

## AI generation sleep interruption, 2026-10-17

[Sleep qualification](qualification/generation-sleep-2026-10-17.md) records
typed native sleep/wake observation, durable interruption of running and queued
AI pauses, retained retry intent and guarded candidate publication. Wake does
not restart affected jobs; Retry creates a new attempt. Earlier user cancellation
and concrete worker/storage failures keep their own causes. Physical sleep and
real inference teardown remain owner checks; other job kinds and the remaining
DP-18 requirements stay open.
The scoped regression selections, both strict Clippy configurations, 37-check
release visual/performance Jobs replays and native Metal startup/shutdown pass.
Visual review also fixed keyboard scrolling to reveal complete Jobs rows and
their action buttons.

## Jobs and whole-Original macros, 2026-10-16

[Jobs qualification](qualification/jobs-2026-10-16.md) records power/pressure
admission, independent queued AI pauses, explicit Run anyway, stage estimates
and the quit/wait workflow. [Whole-Original macros](qualification/whole-original-macros-2026-10-16.md)
add semantic reuse with full measured A/V timing, fresh identities, unchanged
registers and one compound Undo, including insertion from the Original pane.
Release visual/performance replays pass 29 Jobs and 401 macro checks; native
Metal startup/shutdown passes. The final targeted gate passes all 330 tests
and both strict Clippy configurations. Expanded media checks now run in
exhaustive groups below their unchanged per-test time limit.
DP-06 and DP-18 remain partial; the linked evidence retains remaining scope,
initial failures and verification limits.

## Resumed implementation, 2026-10-06

A plain `Split` of a beat that owns attached sounds no longer refuses: it
uses the beat-sound clock transport already used by interior pauses, keeps the
sound on both fragments and leaves the mixed PCM bit-identical, with and without
a shared equalizer. `SetHoldDuration` uses the same transport when the project
has no root sounds: a longer pause before an attached sound owner moves the
sound, which resumes on its own boundary. Since 2026-10-15 Trim, Roll, the
combined native Trim and speed changes elsewhere in the project use the same
transport (core journal tests and decoded PCM moved by the exact boundary
shift). Edits inside an owner's processing scope, Repeat and Retime wraps that
include the owner, and override edits around sound owners still refuse.

[Tracking evidence](qualification/tracking-evidence-2026-10-15.md): rotated,
anamorphic and rotated-anamorphic synthetic Originals tracked by the real
`deadpan-track` worker stay within 1.3 upright pixels of FFmpeg's presentation,
and the saved target's `display_aspect`, Camera's Fit and `follow_pose` land on
the true subject. A real defect was fixed: Vision received squeezed anamorphic
pictures and repeated stale boxes (up to 24 px error); tracking and face
detection now analyse square-pixel pictures. `track --save` through the open
app's live endpoint is exercised with the real worker (commit, refresh-failure
receipt, stale refusal, replace, missing endpoint, undo).

[Letterbox-corrected follows](qualification/letterbox-follow-2026-10-15.md):
targets record their exact display aspect (Camera, app and CLI tracking, face
proposals; corrections keep it) and `follow_pose` maps the center through the
presentation canvas's Fit layout, identically for preview, Camera and export.
A 16:9 Original on a 1080x1920 canvas now follows the right point; the
`letterbox-follow` recipe passes release `verify-export` (30/30 pictures, luma
PSNR at least 44.35 dB, zero audio offset). Targets saved earlier keep the old
behavior until re-saved.

[Pauses inside pitch-preserving speed changes](qualification/preserve-pauses-2026-10-15.md)
split the stage at the pause's output frame into two full processing contexts,
with the Hold between them and the stage's framing, gain and edges lifted onto
an enclosing group. Results stay integral, so marks, sounds and ordinary
commands keep working; decoded processed PCM resumes bit-identically after the
pause, with and without lifted gain. An animated group inside the stage, a
treated stage under a window, Repeat ancestors and nested speed changes refuse.

Headless `command --dry-run` now reports spec §6.4 warnings (removed or
unresolved marks, removed sounds, new exact speed-change clocks, AI requests a
change may make stale) and the jobs a commit would require (AI generation),
alongside the existing changes, affected IDs and duration delta
(`dry_runs_report_warnings_and_required_jobs`). Semantic selector resolution
remains DP-06 work.

[Recognizer failure detection](qualification/recognizer-failures-2026-10-15.md)
(`deadpan-recognition-1`) flags repetition loops (at least 4 copies and 24
words) and at least 5 s of detected speech without words, derived on read and
never editing words; `transcribe`/`transcript` and the transcript rail report
it, and `transcribe --again` or the rail's "Recognize again" re-recognizes a
flagged passage explicitly, once. On LibriSpeech test-clean through the CLI it
flags all 3 known failures and none of 84 good chapters; WER after
re-recognition fell from 100.3%, 90.6% and 2.1% (mistimed) to 2.8%, 3.1% and
2.4%. Conversational, noisy and music material remain unmeasured.

[Pauses inside FollowSpeed speed changes](qualification/retimed-pauses-2026-10-15.md)
now commit through `InsertTime`: the speed change stays the live owner, its
slope is unchanged and fractional child cuts and pause spans become exact
derived clocks, while integral results keep ordinary recipes. Decoded PCM
resumes the old sample stream bit-identically after the pause, pictures keep
their exact source times, and SQLite history replays, undoes and redoes the
edit. Retained bindings and lineage are now admitted alongside affine derived
clocks. [Ordinary editing around those clocks](qualification/clocked-editing-2026-10-15.md)
now works: commands run in a sealed scope where integral recipes are proven
exact outside clocked speed changes, and each leaf refuses changes inside them.
Root pauses, Split (which copies clocks), grouping, deletion and Undo work; the
native workspace opens these projects and native black and freeze pauses land
inside a slowed beat at fractional child frames. Editing inside a clocked speed
change, Preserve/Shift routes, marks and sounds remain open. No requirement or
gate is newly complete.

2026-10-15: [Section 8 export evidence](qualification/preview-export-2026-10-15.md)
adds twenty headless recipe fixtures (False start, Interrupted answer,
Callback/registers, Reaction ping-pong, Abrupt return, three gags, group and
ungroup, dot-repeat, `,i`, `,s`, Trim/Slip/Roll, one-play overrides, macros,
Camera on a Hold, local recipes, detach) exported through public Render and
passed by `verify-export`, with framing negatives for the Camera rows. All 66
recipe fixtures pass in one optimized run. Native key paths remain separate
evidence; no requirement or gate is newly complete.

[Historical exact audio geometry](qualification/frozen-audio-clocks-2026-10-12.md)
now captures effective Retime windows, fractional Sequence offsets and compact
Repeat placements independently of intrinsic recipe counts. Bounded layout JSON
round-trips those facts; FollowSpeed historical queries match live root sample
allocation. Preserve preparation, captured raw audio and retained binding
consumers still refuse exact layouts until owned output scheduling is integrated.
Physical output history, metadata/lifecycle and atomic Retime insertion remain
open. The primary implemented and self-reviewed alone; no requirement or gate
is newly complete. See the [contract](FROZEN_AUDIO_CLOCKS.md).

[Owned Preserve inputs](qualification/owned-preserve-inputs-2026-10-12.md) now
persist complete current-tree operands in core 55/store 83 and compile them into
intrinsic preparation projections. Nested stages consume inner intrinsic output
before splice allocation; Source recipes remain live. Decoded PCM verifies
canonical nested history, fractional input phase, Repeat scope, source changes
and output silence with no input-grid point. Core patches and SQLite named-take
restore/undo/redo preserve the payload. Ordinary and flattened audio paths refuse
unplaced output, including effective unity mappings. Metadata/lifecycle,
retained output clocks, atomic Retime insertion and native editing remain open.
All 2,941 core/plan/audio/playback/store tests pass, none failed or ignored.
Strict all-target Clippy across those crates, CLI and app, plus formatting, pass.
The primary implemented and self-reviewed alone. No requirement or gate is newly
complete; source/binary identities and logs are retained with the qualification.

[Persisted effective clocks](qualification/derived-clocks-2026-10-12.md) add a
separate exact Retime extent/window table in core 54 and SQLite schema 82.
Document validation, reversible patches, snapshot restore, saved history and
production picture/audio plans retain those clocks without changing intrinsic
recipe counts. Exact Sequence/Repeat allocation and actual root duration are
compiled from persisted documents. Decoded raw PCM retains exact fractional
seam phase across bounded reads. Legacy edits and integral duration maps refuse
derived-clock documents; at that increment marks, sounds, retained bindings/lineage
and Preserve/Shift were outside derived-clock admission. Owned routes,
atomic Retime insertion and temporal/native lifecycle integration remain open.
The native workspace refuses these documents before exposing integral controls.
All 2,927 core/plan/audio/playback/store tests pass, none failed or ignored.
The separate headless native admission test, strict all-target Clippy for the
seven affected crates and workspace formatting also pass. Work was implemented
and self-reviewed by the primary alone.
No requirement or gate is newly complete.

[Effective plan extents](qualification/plan-extents-2026-10-12.md) separate exact
output allocation from integral recipe counts in picture/audio walkers,
definition grids, owner windows and independent voice admission. Committed
framing and Camera preview use the same exact owner extent. That increment's
compiler-boundary fixtures exercise fractional allocations; the later persisted
clocks above replace those injected allocations. Retime insertion remains guarded
until owned routes and lifecycle transforms are implemented. All 1,153
plan/audio/playback tests, six focused plan tests and 32 native Camera/presentation
tests pass, none failed or ignored. Strict all-target Clippy for plan, audio,
playback, CLI and app, plus formatting, pass. No requirement or gate is newly complete.

[Production plan prefixes](qualification/plan-prefixes-2026-10-12.md) connect
the exact Repeat index and exact Sequence offsets to picture/audio walkers,
owner queries, Hold context and retained processing plans. Existing integer
Repeat storage embeds without reconstruction, and the integer occurrence-range
API refuses fractional endpoints instead of rounding. All 2,910 core, plan,
audio, playback and store tests pass, none failed or ignored. Both new plan
tests pass again after adding direct PointCeil coverage below Preserve;
production Rust is unchanged since the broad run.
Final strict all-target Clippy for those crates and formatting pass.
Node extents, persisted Retime clocks, marks and the owned Preserve splice route
remain open. No requirement or gate is newly complete.

[Exact Repeat prefixes](qualification/exact-repeat-2026-10-12.md) supply compact
fractional play/gap clocks through the existing Repeat indexing algorithm.
Ordinary documents retain their integer API and arithmetic. Independent expanded
oracles cover fractional seams, stable IDs, bounded lookup and arithmetic
exhaustion; all 2,908 core/plan/audio/playback/store tests pass, none failed or
ignored. Strict all-target Clippy for those crates and formatting pass.
This advances the Retime insertion prerequisite. Persisted effective
clocks, mark transforms and owned Preserve routes remain required before
insertion can be enabled. No requirement or gate is newly complete.

[Pauses in This play](qualification/scoped-pauses-2026-10-12.md) now carry an
explicit occurrence address from the scoped inspector through insertion and
the saved selection receipt. Both edges belong to the selected play; stale
commands and broader play selections cannot retarget it. All 1,725 core/audio
tests and 13 final scope/service tests pass. Final offscreen replays pass 78
scoped-edit and 56 ordinary AI-insertion checks, including the corrected compact
controls and delayed-history guard. Strict workspace/all-target Clippy and
formatting pass. The qualification retains the broad app
run, corrected reference/scope assertions and bookmark timeout diagnosis with
the same executable passing in an isolated directory. All/multiple-play pauses,
Retime, cross-parent freezes and other temporal operations remain open. No
requirement or gate is newly complete.

[Pauses inside Repeat](qualification/occurrence-pauses-2026-10-12.md) now
isolate one concrete play or gap before inserting time. The enclosing Repeat
and its live framing remain intact; later allocations retain their exact sample
entries. Native and macro hosts share identity preflight and full-instance
freeze capture. Focused regressions cover nested boundaries, marks, permissions,
attached/root sound processing, maximum Repeat counts and durable AI intent.
The final core/plan/audio/playback/store run passes 2,899 tests; all eight native
pause tests and strict Clippy checks pass. Debug and release exports verify all
15 pictures and 31 audio blocks with zero measured offset, and six authored-PCM
windows match bit-for-bit across builds. The qualification record retains failed
runs, source/binary identities and actual media. Authored Retime/fractional
insertion and general scoped-inspector temporal editing remain required. No requirement
or gate is newly complete.

[Selected-play preflight](qualification/scoped-preflight-2026-10-12.md) now
avoids collisions between private temporary names and legal node, mark or
retained audio identities. Missing targets also remain invalid when they name
private temporary clones. All 1,073 core tests passed; after the two review
corrections, 80 focused core/native tests pass, including selective nested gain,
one Undo, Redo and SQLite reopen. Both sets of failing regressions are retained.
Strict workspace Clippy and focused final core Clippy pass.
No requirement or gate is newly complete.

[Existing Repeat Split clocks](qualification/repeat-split-clocks-2026-10-12.md)
now preserve attached sound recipes and concrete play histories when repeating
a partial range inside an existing Repeat. Exact reducer aliases retain ordered
Repeat ancestry and original play cohorts; later births and shrink/regrow stay
independent. Dry/treated Source and nested Preserve, copy/recopy, original-Repeat
deletion, gaps and atomic durable history are covered. All 2,880 core/plan/audio/
playback/store tests pass with none failed or ignored, along with strict workspace
Clippy and focused release checks. Debug and final release exports verify all
17 pictures and 44 audio blocks with zero offset. Five authored PCM windows match
bit for bit across debug/release; app/CLI release builds and formatting pass.
Evidence and self-review are retained. Root-owned and changed processing-scope
transforms remain guarded; no requirement or gate is newly complete.

[AAC noise substitution](qualification/aac-noise-substitution-2026-10-12.md)
resolves the short partial-Repeat export's fidelity failure at the unchanged
384 kbit/s target and timing/content thresholds. Encoder ABI 4 disables PNS;
both live and stored observation validators retain their version boundaries.
All 20,821 canonical input pairs are unchanged. The final release movie passes
13 pictures and 33 blocks with zero offset and 18.3043 dB minimum SNR; the
120-picture nested-EQ export also passes. All 31 release export tests pass,
including 42 constructed recipes; four optional synthetic-generation fixtures
were not built. Native debug/release checks, protocol and job checks, strict
workspace Clippy, app/CLI release builds and formatting pass. No gate is complete.

[Independent source projection reuse](qualification/source-projection-reuse-2026-10-12.md)
resolves nested Preserve plus event/owner EQ exhausting the 64-stage preparation
budget. A checked recipe/instance/context/stage key, paired with immutable plan
identity, reuses complete input while preserving source, depth and resource checks.
The previous release fails full Render on the real-media fixture; the new debug
and release renders verify all 120 pictures and four audio windows with zero signal offset.
Cold PCM and old/new inspection windows match exactly. The separate short
partial-Repeat AAC fidelity failure remains open. All 1,139 plan/audio/playback
tests pass with none failed or ignored; strict workspace Clippy and focused
release checks pass.

[Partial Repeat sounds](qualification/partial-repeat-sounds-2026-10-12.md) now
retain split-owner recipes and initial-play clocks, with independent later births,
gap edits, copy/recopy and durable Undo/Redo. Repeat also reserves its pre-edit
timing identity when every source recipe was already bound. All 1,481 core/plan,
48 attached-audio and six store clock tests pass. The real-media export has all
13 pictures and zero audio offset correct, but one of 33 AAC blocks fails the
existing fidelity threshold; the diagnostic failure is retained. Nested Preserve
with EQ also exposes a pre-edit preparation-budget failure, retained for follow-up.
Strict workspace Clippy, three release PCM regressions, debug/release diagnostic
checks, app/CLI release builds and formatting pass. Neither failure is qualified
away, and no requirement or gate is newly complete.

[Split sound owners](qualification/split-sound-owners-2026-10-12.md) now retain
attached recipes and chronological clocks on eligible surviving pause, deletion
and move fragments. Exact Split provenance prevents suffix loss; unchanged full
processing owners supply history behind delivery crops. PCM and durable-store
regressions pass, and debug/release real-media exports verify nine pictures,
21 audio blocks and zero offset. All 2,867 core/plan/audio/playback/store tests
pass with none ignored. Strict workspace Clippy, release PCM/store regressions,
app/CLI release builds and formatting pass; evidence and self-review are retained.
External Repeat/root-owner and enclosing-subtree transforms remain guarded.

[Copy mastering context](qualification/copy-mastering-context-2026-10-12.md)
resolves the pending mixed-bus comparison: the copied shared-EQ authored PCM
matches exactly, while a preceding Original transient changes master gain at
the new placement. The regression verifies every limited sample against the
independently read authored bus and reported gain. No production change or
newly complete requirement is claimed. Debug/release, strict CLI Clippy and
formatting pass; the existing seven-picture/19-block release export still
verifies zero offset.

[Partial attached-sound copies](qualification/partial-sound-copies-2026-10-12.md)
now preserve complete recipes, chronological delivery clips and treatment input
through eligible capture/paste/recopy and original-owner deletion. Shared EQ
retains a terminal input sample outside the newly rounded nominal allocation;
half-sample ties retain their original ownership after odd moves. All 2,859
core/plan/audio/playback/store tests pass with none ignored. The real CLI fixture
passes independent PCM checks and a seven-picture/19-audio-block export with
zero sample offset in debug and release. Seven focused release regressions,
store persistence, strict workspace Clippy, formatting and the app/CLI release
build pass. Source/Hold and supported-fragment range rules still apply;
partial composite Preserve ranges, surviving-owner splits and wider attachment
transforms remain open. No requirement or gate is newly complete.

[Sound occurrence windows](qualification/sound-occurrence-windows-2026-10-12.md)
retain each crop and rounding anchor while preserving complete processing input.
All 1,121 plan/audio/playback tests and strict workspace Clippy pass; the new
fractional-route and decoded nested-Preserve regressions also pass in release.
The partial-copy integration is recorded above.

[Shared event treatment prefixes](qualification/event-prefix-cache-2026-10-12.md)
fix a real canonical-playback work-budget failure with EQ and both event returns.
Per-request tiles reuse the input before EQ and each return without raising
limits. All 706 audio/playback tests and strict workspace checks pass; both
regressions also pass in release. A real export verifies 30 pictures, four audio
blocks and zero sample offset. Persistent caching and full qualification remain open.

[Retained viewer pixel verification](qualification/retained-viewer-pixels-2026-10-12.md)
corrects the previous visual assessment of a black placement-replay viewer.
Both saved images contain the picture. The replay now checks composed fixture
pixels and caption visibility after inspector scrolling at both window sizes.
Debug and release each pass 273 scenario checks plus the shortcut audit; the
release build, strict workspace Clippy and formatting pass. No requirement or
gate is newly complete.

[Independent sound treatments](qualification/event-treatments-2026-10-12.md)
now give root and attached sounds gain envelopes/mutes, EQ, saturation and
balance before their sends and group processing. Native captured commands
author placed sounds and preserve complete recipes through Undo/reopen.
The event-local clock follows retained input through moves and delivery cuts.
All 3,969 tests, strict workspace checks, 663 final offscreen scenario checks
and a real 30-picture/four-audio-block export pass; its audio window verifies
zero offset. Test-fixture and replay-helper failures remain in evidence. The
initial black-viewer visual assessment is corrected above. Partial attached-sound scope routes, wider
incoming-tail/loop qualification, persistent filtered caching, mastering and
performance keep DP-09 and DP-16 partial. No requirement or gate is newly complete.

[Independent sound returns](qualification/event-returns-2026-10-12.md) now
process root and attached sound recipes before saved delivery routes and group
treatments. Native `:sound-wet-send` authors captured placed-sound levels.
Selection/cut fades, current silence permissions and allocated-Tail tap ownership
remain explicit; copies through nested Preserve retain their effect history.
All 3,962 tests, strict workspace checks, 603 final offscreen scenario checks
and a real 30-picture/four-audio-block export pass; its signal window verifies
zero offset. Replay setup failures and their fixes remain in the evidence.
That increment left creative event treatments to the implementation above;
partial scope edits, wider incoming-tail/loop qualification and
cache/mastering/performance work keep DP-09 and DP-16 partial.
No requirement or gate is newly complete.

[Retained sound input](qualification/retained-sound-input-2026-10-12.md) adds a
checked old-clock read shared by routed sound preparation. It preserves trimmed
history, exact source phase and nested Preserve context independently of current
delivery gates. Event sends now consume it as recorded above; DP-09 and DP-16
remain partial.

[Continuous owner returns](qualification/wet-owners-2026-10-12.md) now use stored
beat/group levels, captured `:wet-send` authoring and the canonical shared bus.
Returns enter before parent treatments, keep per-contribution clocks and obey
silent-Hold and explicit Tail permissions. All 3,950 tests, final service checks,
350 offscreen scenario checks and a real 30-picture H.264/AAC export pass; the
emitted audio window verifies zero offset. Independent event sends, broader
retained/Preserve coverage and remaining scope/cache/mastering work keep DP-09
and DP-16 partial. No requirement or gate is newly complete.

[Finite reverb/delay processors](qualification/finite-wet-2026-10-12.md) now
declare complete history and prepare fixed tiles that match cold and linear
reads. All 622 audio/DSP tests, final focused tests, strict workspace checks,
fatal ASan/UBSan and allocation-failure checks pass. Three release processes
measure and reproduce both kernels. That increment qualified numerical
preparation; owner integration is recorded above. Existing selected Tail sends
retain their behavior; DP-09/DP-16 stay
partial and no feature or gate is newly complete.

[Selected tail sends](qualification/tail-sends-2026-10-12.md) now route explicit
beat, group and sound contributions into allocated reverb/delay pauses without
changing dry audio or picture time. Native commands preserve captured targets,
Undo, dot/macros and single-play scope. All 3,927 tests, strict workspace checks,
335 final visual scenario checks and a real 60-picture H.264/AAC export pass;
both emitted audio windows verify zero offset. Continuous wet returns, complete
incoming-tail policy, partial sound-owner edits and remaining cache/mastering
work keep DP-09 and DP-16 partial.

[Automation through shared stages](qualification/shared-automation-2026-10-12.md)
now retains each contribution's gain/mute clock before and after saturation/EQ,
including nested parent treatments. Saturation uses one combined drive; linear
EQ retains each contribution through its complete shared prefix. All six stage
orders, exact cancellation, silent-Hold permissions, cold partitions and bounded
admission pass. All 1,069 audio/plan/playback tests and strict workspace checks
pass; archived regressions and a new real release export establish the change.
Partial attached-sound scope edits, continuous wet returns, incoming-tail policy,
broader bindings and persistent filtered caching remain open. DP-09/DP-16 stay partial.

[Native EQ](qualification/native-eq-2026-10-12.md) now authors the shared recipe
through captured fields and typed commands, preserving both clocks, other
treatments and one-step Undo. Buffered invalid values cannot commit; dot, macros,
stale captures and one-play isolation pass native replay. The full app suite
passes 1120 tests (nine existing opt-in ignores), final focused checks and strict
workspace Clippy pass, and final EQ/Gain/audio replays pass 500 visual checks.
Separate performance evidence and inspected default/minimum captures are
retained. Remaining automation/send/tail/cache work keeps DP-09/DP-16 partial.

[Shared group EQ](qualification/shared-eq-2026-10-12.md) now mixes Original,
root sounds and beat attachments through ordered recursive EQ and saturation.
Per-contribution permissions keep allowed sounds from reviving forbidden audio
under silent Holds. Hidden and saved/copy history, neutral crop edges, 970
plan/audio tests, strict workspace Clippy and a real 54-picture H.264/AAC export
pass. Native authoring and independent automation are covered above; arbitrary
sends/tail policy, broader binding coverage and persistent filtered caching remain open.
DP-09/DP-16 stay partial.

[Saved sound input](qualification/beat-sound-owner-input-2026-10-11.md) now reads
full attachment history through move/copy clocks, current Hold gates and event
edges/gain. A separate rounded delivery anchor fixes a one-sample shift exposed
when copying cropped NTSC input. This input path is now consumed by the shared graph above;
remaining send/tail policy remains unfinished; native EQ is covered above.
DP-09/DP-16 stay partial.

[Independent sound history](qualification/sound-owner-context-2026-10-11.md)
now retains full current owner input behind crops while preserving nested
Preserve, inner selections, stable Repeat scope and current Hold policy.
Cold decoded PCM and fractional-phase checks pass, with all 958 plan/audio
tests and strict workspace Clippy. The shared mixer now uses this
history path, as qualified above. Remaining processing and send/tail policy
remain agent work; DP-09/DP-16 stay partial.

[Stored and recursive EQ](qualification/authored-eq-2026-10-11.md) now uses typed
semantic edits, preserves order through other treatments, and evaluates nested
Original owners through the shared authored bus. Crops, resumed Preserve,
Repeat reuse, silence and nonzero tail input have regressions. A release CLI
fixture commits EQ, checks independent tone amplitudes and verifies its emitted
H.264/AAC file. The subsequent shared-EQ qualification above adds independent
sound/group processing, and native EQ authoring is qualified above. Remaining
owner/policy/cache qualification is still owed; DP-09/DP-16 remain partial.

[Ordered owner EQ](qualification/owner-eq-2026-10-11.md) now evaluates a checked
treatment prefix, actual EQ and treatment suffix over full Original history.
Direct convolution, cold crops, Preserve, retained RootRoundEven/PointCeil
clocks and silent Holds have decoded-media checks. This earlier preparation API
is now a foundation for the stored recursive path above. Mixed sounds,
incoming-tail policy and filtered caching remain agent work. DP-09 and DP-16
remain partial.

[Retained fades and treatments](qualification/retained-treatments-2026-10-11.md)
now evaluate creative edges and current gain/mute, saturation and balance on
retained consumer clocks, including history before delivery. Exterior owners
remain separate, and the internal reader carries source dependencies and its
parent's work/deadline/depth limits. That increment's full plan/audio suites
passed 935 tests. The complete mixed-voice graph and native integration remain
owed beyond the Original-only path above. DP-09 and DP-16 remain partial.

[Owner input contexts](qualification/owner-input-2026-10-11.md) now retain
hidden Sequence/Repeat input and preserve enclosing time/pitch processors.
The shared PCM reader evaluates their output edges; separate checked reference
maps expose resumed history before current delivery. Cold decoded PCM and a
cropped-group EQ comparison verify this prerequisite. The stored path above
extends it; mixed-voice silence/tail policy, native controls and full
preview/export integration remain agent work. DP-09 and DP-16 remain partial.

[Equalizer parameters and processing](qualification/eq-recipe-2026-10-11.md)
add exact bounded three-band gains and low/high cuts through one compensated
finite kernel. All 1,051 core and 523 audio tests pass, including response,
phase, cold reads and exact neutral bypass. Release response evidence uses the
actual coefficients. That numerical checkpoint preceded the stored integration
above. Full group/tail policy, native controls and preview/export integration
remain owed. DP-09 and DP-16 remain partial.

[The finite stereo filter processor](qualification/finite-filter-2026-10-11.md)
adds a bounded 4,097-tap f64 FIR adapter with exact context extents, signed
channel output, atomic failure handling and allocation-free native reads.
Direct-convolution, cold-seek and sanitizer checks pass. Three release runs
measure 52.0–52.2 µs p95 per 1,024 stereo frames on this Mac. This is numerical
infrastructure only; the later stored-EQ qualification above covers Original
integration. Full voice/policy and native qualification remain owed.
DP-09/DP-16 stay partial.

[Saved stretched audio](qualification/persistent-audio-2026-10-11.md) adds
cross-restart reuse with fresh input/source admission, BLAKE3 keys and complete
payload verification. Atomic publication, a shared 4 GiB budget, reader-safe
grace-period cleanup and native status keep it derived and inspectable. Cache
lookup has a separate bounded query allowance after a long-fixture regression
exposed interference with canonical preparation. Full audio/playback tests and
Storage/Diagnostics replays pass. The remaining voice/group/mastering pipeline
keeps DP-09 and DP-16 partial. Six fresh-process device checks verify reuse,
including a 240.24-second stage; they record the cold cost and have no audio
faults or missing pictures in their measured intervals.

[Audio preparation during playback](qualification/audio-ahead-2026-10-11.md)
uses a bounded FIFO on the existing worker: up to 29.867 seconds of canonical
PCM beyond the native queue, with unchanged first-batch activation and delivery
clocks. Full-queue Stop, supersession, edited-proposal revocation and exact
future-stage PCM have regressions; the controlled one-slot variant fails the
future-stage test. Two real-device runs have no underruns or faults, including
a 35-second loop that reaches the FIFO bound; all 102 UI replay checks pass.
Persistent reuse is recorded above; the remaining audio pipeline keeps DP-09/DP-16 partial.

[Idle audio preparation](qualification/audio-warming-2026-10-11.md) examines up
to 30 seconds ahead of a settled Original/Edit/Sound cursor on the existing PCM
worker. Matching Play reuses its work, while navigation, waveform requests and
Stop cancel obsolete preparation. A future Preserve seek matches canonical PCM;
native status leaves both clocks, history and output untouched. Active preparation
and saved-cache lifecycle are recorded above; the remaining audio pipeline
keep DP-09/DP-16 partial.

[Gain scrolling verification](qualification/gain-scroll-2026-10-11.md)
resolves the waveform replay failure as incorrect input sequencing: corrective
wheel events arrived before native smoothing finished. Complete trackpad
gestures and explicit settled mouse-wheel checks now pass all 296 Gain checks
in visual replay and release runs at both 60 and 120 Hz. Full graph/axis and
keyboard-focus assertions remain, and inspected compact/default captures fit.
The correction is confined to the replay; DP-20 remains partial.

[Final-picture handoff](qualification/terminal-picture-2026-10-11.md)
preserves the final useful decode through GPU submission after the exact audio
endpoint. Twelve deterministic Original/Edit cases and seven real-device runs
pass, presenting all 8,206 pictures without audio faults. One final picture
completes after the admitted End update and is explicitly marked in the report;
this fixes cancellation without claiming physical timeliness. Core checks and
eight interaction replays pass. A Gain waveform visibility replay failed on
both binaries; the subsequent scrolling qualification above resolves it as a
replay input-sequencing defect.

[Generated slice decoding](qualification/generated-slices-2026-10-11.md)
uses four FFV1 slice threads for preview while preserving single-threaded
index/export readers and the allocation bound. All 5,220 independent pixel
comparisons, 183 native tests (also under ASan/UBSan), seven real-media app tests
and strict checks pass. Five ordinary 1080p60 runs, one 4K30 run and both
30-lap loops present all 3,752 pictures. The longer fixture presents every
Generated picture but misses its final Original during the End-to-stopped
handoff; that failure is retained, with its cancellation addressed by the
subsequent terminal-picture qualification above. DP-16 remains partial.

[Generated selection loops](qualification/generated-loops-2026-10-11.md)
now prepare the missing Original decoder when playback starts on Generated.
Thirty laps present all 1,830/930 pictures at 1080p60/4K30, plus the next lap's
first picture, without audio faults. The first 4K Original read falls from
64 ms to 8.7 ms and its following skipped picture is recovered. Six real-media
tests, the new 4K regression and strict checks pass. The ordinary nonloop
control still exposes a separate first-Hold timing gap; DP-16 stays partial.

[Closely spaced Generated providers](qualification/generated-transitions-2026-10-11.md)
now prepare the next accepted Hold while a different one plays. Two real LTX
Holds separated by one Original picture present all 180 frames at 1080p60 in
both final runs, with no audio faults. Second entry falls from 190 ms to
0.15–0.16 ms on the worker. Playback retains at most four decoders, including
Original look-ahead and one upcoming Generated admission. Five real-media
regressions, 45 worker tests and strict Clippy pass. DP-16 remains partial.

[Real Generated export](qualification/real-generated-export-2026-10-11.md)
passes all 225 pictures and six audio windows of accepted LTX Holds at 1080p60
and 4K30, with zero measured audio offset. Hand-derived frame/sample probes
verify the inserted pause; all 15 Generated frames reject the pre-acceptance
Freeze reference. Periodic-audio and missing-color-tag failures remain recorded.
Broader interactions, formats and effects keep DP-16/DP-17 partial.

[Real Generated playback in the app](qualification/generated-app-playback-2026-10-11.md)
now qualifies the actual preview worker with real accepted LTX pictures,
full-resolution Originals and device audio. All 1,830 frames at 1080p60 and
615 frames at 4K30 reach offscreen Metal, with no skips, proxies, underruns or
faults. A replay telemetry fix retains completion timing when the next decode
is requested in the same UI pass. Full format/effects and broader transition
stress remain open; DP-16 stays partial.

[Generated entry preparation](qualification/generated-prefetch-2026-10-11.md)
now verifies and decodes the upcoming accepted Hold on a bounded background
job. Four real-media tests pass, including prepared-first-picture proof,
cancelled/stale rejection and all 90 reference pictures; 44 worker and 60 media
unit tests pass. Full-resolution app/device measurements are recorded above.

[Generated playback decoder reuse](qualification/generated-playback-cache-2026-10-11.md)
keeps the Original and its prepared return across Generated Holds and reuses
the accepted decoder on repeated entry. Two real-media tests verify three
selection-loop laps, both prepared returns and all 90 picture buffers against
the single-slot reference. The subsequent cold-entry preparation is recorded
above; the subsequent app/device qualification is also recorded above. DP-16 stays partial.

[Optional VideoToolbox decoding](qualification/hardware-decode-2026-10-11.md)
implements persistent H.264/HEVC/VP9 surfaces, lazy exact CPU transfer and
software fallback with independent receipt measurement. Real decoder tests
match 2,208 pictures at both output depths and 864 reverse seeks. Longer-clip
measurements find substantially slower hardware seeks, so normal preview and
playback retain threaded software. Faster hardware integration and the other
DP-16/DP-24 obligations remain open; no requirement is closed by this work.

[Exact timing acceptance](qualification/exact-timing-acceptance-2026-10-11.md)
closes DP-02 under §29.1. The new combined-edit matrix passes 40 real exports
across 20 source fixtures, with 1,040 independently expected picture identities,
36 zero-offset signal windows and exact terminal sample counts. The audit maps
every §4 timing obligation to the current implementation and accumulated
evidence. Renderer, remaining operations, output and performance work stay open.

[Local Original and sound acceptance](qualification/local-import-acceptance-2026-10-11.md)
closes DP-15 under §29.1. Missing and corrupt placed sounds refuse final export
without changing the edit; identical restoration and portable copies export
successfully. Explicit container audio selection and catalog deduplication pass,
along with 360 picture comparisons, six zero-offset signal windows and 36
focused store tests. External provider, physical-input and owner-media checks
remain on the owner list; broader media-engine coverage stays open.

[Catalog sound recovery](qualification/sound-recovery-2026-10-11.md) adds
explicit repair of a selected sound whose retained file is present but damaged.
It captures identity and location before command entry, preserves history and
placements, and keeps missing-sound reporting separate from Original pictures.
The acceptance audit above closes its remaining DP-15 verification.

[YouTube acceptance](qualification/youtube-acceptance-2026-10-11.md) closes
DP-14 under §29.1 after auditing every §15.2–15.4 requirement. The final
relocated bundle imports, reopens and validates a full-resolution 4K Original
through the confined helper in 38.023 s. Physical input, assistive speech,
authenticated content and clean/second-Mac checks remain explicit owner work.
The audit reconciles optional video resumption, hosted updates and ports with
the normative scope; other requirements and release gates remain open.

[Reordered-video proof with current AAC](qualification/mux-preroll-2026-10-11.md)
corrects a stale packet budget exposed by the full gate. The private duration
proof now admits the encoder's two required priming packets. A deterministic
fixture fails against the old bound and confirms that finalization changes
only the video duration header; authored audio, packet clocks and emitted-file
verification remain unchanged. No additional requirement is closed.

[YouTube network confinement](qualification/youtube-network-2026-10-11.md)
restricts metadata and stream helpers to an owned authenticated CONNECT
gateway. Each tunnel requires a supported YouTube host, HTTPS and an admitted
public address; macOS blocks direct connections and proxy bypasses. Offline
helper probes, bounded relay/shutdown, shared DNS admission and adversarial
parser coverage accompany the change. The qualification records verification
outcomes, including the completed acceptance audit above.

[YouTube confirmation thumbnails](qualification/youtube-thumbnail-2026-10-11.md)
are now fetched and decoded on the import worker before media transfer. The
native confirmation preserves the image's aspect and accessible name, keeps
the decision visible at compact sizes, and reports an optional-image failure
without refusing a valid import. Strict URL/address admission and bounded
network/image work are included in the privacy audit. Real CDN and app-job
checks and offscreen workflow replays pass; regression and release results,
including the retained intermittent proxy failure, are recorded in the linked
qualification. DP-14 remains partial for its other requirements.

[YouTube import size bounds](qualification/youtube-limits-2026-10-11.md)
replace unchecked metadata addition and recheck complete downloads before
assembly. Overflow, a known over-limit stream beside an unknown one, and fast
oversized transfers now fail with `YouTubeTooLarge` without publishing a
project. Both initial regression tests fail against the old implementation.
The fresh bundle passes relocation/tamper checks and imports a real
3840×1608 AV1/Opus Original in 37.439 s, then reopens and validates it.
DP-14 remains partial.

[Proxy encoder investigation](qualification/proxy-order-2026-10-11.md)
reproduces the intermittent packet-order violation and retains three timed-out
diagnostic runs. The retry lifecycle test now uses recorded, independently
verified encoder output after its deliberate stall; all seventeen real-media
proxy tests and twenty lifecycle repetitions pass. Production still rejects
invalid packets and retries once after teardown. This is not an encoder-service
fix and closes no additional requirement.

[Correctable source interpretation](qualification/source-interpretation-2026-10-11.md)
adds the Original pane's **Interpret picture…** control and
`:source-interpret`. Range and chroma corrections are undoable, bound to the
authored revision, and shared by preview, playback, export, AI conditioning,
proxies and picture analysis. Source bytes, qualification receipts, timing and
audio stay unchanged. Core schema 49 and store schema 77 refuse obsolete
development packages. Independent pixel references, history tests, native
offscreen replay and real export comparisons pass; final milestone checks are
recorded in the linked qualification. Broader format/operation coverage and
the remaining URL workflow keep DP-02, DP-14 and DP-16 partial.

[SDR encoder quality](qualification/encoder-quality-2026-10-10.md) now uses
supported frame-QP and bounded bitrate controls. Two concurrent release and
packaged exports pass all 7,012 decoded-picture comparisons and 286 zero-offset
signal windows. The previously failing scene cut improves from 30.54 to
40.63 dB without lowering the 32 dB gate. The regression gate completes in
parts, all 1,099 UI tests and 63 native encoder sanitizer tests pass, and bundle
relocation/tamper checks pass. The OS software encoder lacks the required QP
property and is refused explicitly; it is not a qualified quality fallback.
Expanded content/raster/runtime qualification and the complete export workflow
remain open under DP-17.

[Higher-quality YouTube acquisition](qualification/youtube-quality-2026-10-09.md)
now selects qualified AV1/VP9 as well as H.264 picture and Opus as well as AAC
sound. Descriptor-only assembly preserves coded content, exact picture/sample
clocks, positive offsets, gain, trim and color metadata. Missing AV1/VP9 chroma
siting uses a recorded left interpretation with a picture-bound notice.
The workspace gate completes in parts, 1,099 UI tests/replays and all 242
native sanitizer tests pass, and the real app-job AV1/Opus import completes
in 13.8 seconds. Both release exports pass 1,170 sampled picture comparisons
and 286 zero-offset audio windows. Packaged import and relocation pass. The
initial concurrent packaged export failed the luma gate at scene-cut frame
3,126 while an isolated export passed; both original outcomes remain retained.
The encoder quality follow-up above reproduces and fixes that failure.
Broader profiles, high-resolution performance and remaining URL workflow work
keep DP-02, DP-14 and
DP-16 partial; complete export-quality qualification remains open under DP-17.

[MP4 Opus audio](qualification/opus-mp4-2026-10-09.md) implements mono/stereo
sources with exact pre-skip, terminal trim, header gain and sample-clock offsets.
Eleven retained fixtures preserve independent reference PCM and copied H.264,
AV1 and HDR VP9 pictures; malformed inputs and altered receipts fail explicitly.
The gate completes in parts with one recorded VideoToolbox proxy follow-up;
all 170 native sanitizer tests pass. Eleven release imports/exports verify
1,104 pictures and 27 zero-offset audio windows, including sound placement
and Undo/Redo with unchanged picture time. The fresh-home bundle imports all
eleven and verifies 504 pictures/twelve audio windows; relocation and tamper
checks pass.
The later acquisition milestone above extends the former H.264/AAC-only
downloader and assembly. DP-02, DP-14 and DP-16 stay partial.

[VP9 HDR import](qualification/vp9-hdr-sources-2026-10-09.md) adds ten-bit
limited-range BT.2020 NCL PQ/HLG in MP4/WebM/Matroska, including exact or explicitly
ignored static metadata and mono/stereo Opus combinations. Real rendering also
exposed and fixed the small HEVC coded-raster budget while preserving exact
visible dimensions. The gate completes in parts with recorded proxy/stdio
follow-ups; 167 native sanitizer tests and the affected export tests pass.
Fifteen release imports verify 180 output pictures/nine zero-offset signal
windows. The fresh-home bundle imports all fifteen and verifies 84 pictures/four
signal windows; relocation and tamper checks pass. Broader profiles, performance
and the full operation matrix remain open; DP-02 and DP-16 stay partial.

[AV1 Main import](qualification/av1-sources-2026-10-09.md) now retains
eight/ten-bit SDR, ten-bit PQ/HLG, VFR, anamorphic pixels, grain and super-resolution
through pinned dav1d in MP4/WebM/Matroska. Bounded sequence/packet admission and
static HDR metadata checks precede acceptance; quantized VFR keeps its measured
clock. The gate completes in parts after three test repairs and one unchanged
VideoToolbox follow-up; 163 native sanitizer tests pass. Release checks import
29 inputs and verify 444 pictures/sixteen zero-offset signal windows. The
fresh-home bundle imports all 29 and verifies 120 pictures/five zero-offset
windows; relocation and tamper checks pass. Broader AV1 profiles, high-resolution
measurements and the full source/operation matrix remain open. DP-02 and DP-16
stay partial.

[ProRes 422 import](qualification/prores-sources-2026-10-09.md) now implements
Proxy/LT/Standard/HQ, ten-bit SDR pictures, VFR, anamorphic pixels and both
field orders through the shared picture and audio paths. Bounded QuickTime
metadata and every packet are checked, including real Apple-encoded headers.
The full gate, 157 native sanitizer tests and all ten release imports/exports
pass: 144 pictures and ten zero-offset audio windows. The fresh-home bundle
imports all ten and verifies 48 pictures/three zero-offset windows; relocation,
runtime and tamper checks pass. Broader ProRes profiles/containers, AV1 and the
operation matrix remain open; DP-02 and DP-16 stay partial.

[Wider PCM/WAVE sound import](qualification/pcm-sources-2026-10-09.md) adds
unsigned8, signed24/32 and float32 alongside signed16. Exact format/alignment,
speaker declarations and optional fact counts are checked before decoding;
finite float levels remain intact and non-finite samples fail without a receipt.
Fourteen scalar-reference fixtures, all workspace/UI cases after two test
corrections, strict lint and 151 native sanitizer tests pass. Separate
nonperiodic signals verify 720 release pictures/24 audio windows and 360
packaged pictures/twelve windows at zero offset. Both builds register twenty
files. The qualification retains the original periodic/unobservable timing
result and stdio diagnostics. Broader WAV metadata/containers, AV1, ProRes and
the full operation matrix remain open; DP-02 and DP-16 stay partial.

[MP3 sound import](qualification/mp3-sources-2026-10-09.md) adds raw mono/stereo
MPEG-1/2/2.5 at all nine rates, with exact declared trim across frames, bounded
metadata and CRC checks. Untagged samples remain intact. Six release exports
verify 720 pictures and 24 zero-offset audio windows; the isolated-home packaged
app registers all 16 fixtures and verifies 360 pictures and 12 windows.
The full gate and native sanitizers pass; the qualification retains the one
nextest stdio warning and its focused follow-up. VBRI, broader metadata/container
grammars, other required codecs and the operation matrix remain open. DP-02
and DP-16 stay partial.

[Opus source audio](qualification/opus-sources-2026-10-09.md) adds mono/stereo
WebM/Matroska sounds and audio-bearing Originals, with CELT, SILK and hybrid
decoding through pinned libopus 1.6.1. Exact pre-skip and terminal trim define
available samples; every raw timestamp is checked against the origin-based
sample clock. Audio-first and multiple-audio tracks preserve stream identity.
Release verification covers 48 pictures and four zero-offset audio windows;
the packaged app covers 60 pictures, five zero-offset windows and eight catalog
sounds. The qualification records final regression and packaging evidence. Ogg,
multichannel mappings, broader container grammar and the remaining format and
operation matrix stay open; DP-02 and DP-16 remain partial.

[Video-only VP9 WebM/Matroska](qualification/webm-vp9-sources-2026-10-09.md)
now preserves explicit eight/ten-bit SDR color, anamorphic display, measured
millisecond timestamps and VFR intervals through native decoding and public
Render. Checked nominal cadence retains fractional project rates. Ten inputs
verify 166 exported pictures, including thirty project frames from twelve VFR
pictures. Opus/audio-bearing WebM is covered by the newer milestone above;
quantized-CFR proxies, missing-color
interpretations and the remaining format matrix stay open; DP-02 and DP-16
remain partial. The qualification records final checks and known limitations.

[SDR VP9 in MP4](qualification/vp9-sources-2026-10-09.md) now retains
eight/ten-bit full/limited-range pictures, explicit chroma siting and exact
clocks. Hidden altref pictures and repeated existing frames preserve the
authored timeline. Native pixel/seek tests, source receipts, real proxies and
debug/release public Render pass, including 112 VP9 pictures and four zero-offset
signal windows. Final gate results are recorded in the qualification. DP-02
and DP-16 remain partial for VP9 HDR, AV1, other required codecs
and the full source/operation matrix.

[SDR HEVC and H.264 High10](qualification/sdr-source-codecs-2026-10-09.md)
retain full/limited range, ten-bit picture precision and exact B-frame clocks.
Six public Render cases verify 72 pictures and six audio windows at zero offset.
Bounded draining before HEVC seeks also fixes stale output in existing HDR
decoders. Further VP9 formats, AV1, ProRes, deep-SDR AI conditioning and the broader
source/operation matrix remain open; final gate evidence is in the qualification.

[H.264 telecine](qualification/telecine-sources-2026-10-09.md) now uses explicit
picture-timing declarations through decoder reordering, exact two/three-field
intervals and byte-identical repeated fields. Source receipts preserve the
59.94 fps basis and Original audio endpoints; debug and release public Render verify
155 pictures and eight audio windows with zero offset. The remaining codec,
profile and creative-operation matrix is still open; final gate evidence and
unsupported standalone field-pairing cases are recorded in the qualification.

[Interlaced Originals](qualification/interlaced-sources-2026-10-09.md) now use
deterministic progressive fields with exact source timing, measured final
durations and temporal seek context. Real TFF/BFF, B-frame and variable-timing
fixtures pass; the public Render matrix compares all 62 pictures and four audio
windows at zero measured offset. Other source formats
and the broader source/operation matrix remain open. Final gate evidence is
recorded in the qualification.

[Fractional MP4 clean apertures](qualification/fractional-aperture-2026-10-09.md)
now retain exact source geometry through receipts, project basis, canonical
rendering, thumbnails, analysis, tracking and proxy presentation. Fractional
HDR samples stay untouched until the shared color/filtering stage. Independent
pixel/geometry tests, real Vision and UHD proxy fixtures, 5,546 workspace tests
and release Render's 120 pictures/four audio windows pass. The qualification
records final follow-up checks and exact binary evidence. DP-02 remains
partial for the broader source-policy and source/operation matrix.

The [eleven-case VFR recipe qualification](qualification/vfr-recipes-2026-10-09.md)
fixes reverse and bleep picture phase and ping-pong endpoints by retaining exact
Hold source-clock origins. All 867 exported pictures, 28 audio windows and 17
zero-offset signal windows pass. Core 48 / SQLite 76 refuse obsolete unused
development formats through 75 without writes. DP-02 remains partial pending
the complete source/operation matrix.

[AAC opening fidelity](qualification/aac-preroll-2026-10-08.md) now preserves
quiet onsets with one explicit silent encoder preroll block and exact emitted
timing under ABI 2. Authored PCM, content thresholds and sample origin are
unchanged. The real 10,000-command fractional-rate qualification passes every
canonical sample, all 10,100 exported pictures and all 34 audio windows with
zero measured offset. DP-02 and DP-17 retain their broader source/output work.

The [2026-10-07 scoped AI qualification](qualification/bridge-scopes-2026-10-07.md)
adds exact Repeat Default/Play and Retime generation, isolated acceptance,
durable address mapping and stale-preview rejection. The repository gate
completed after fixing two test expectations; 1,039 UI tests, 84 Python tests
and 77 replay checks passed. A packaged real-model run passed 60 checks,
including 30 intrinsic generated pictures sampled into 15 root frames,
Play 2-only acceptance, Undo/Redo and verified export.

[Automatic AI replacements](qualification/ai-replacements-2026-10-07.md)
now save the longer pause and fallback immediately, preserve the previous
controls and scope, and prepare a fresh request off the writer. Missing inputs
remain visible in Jobs with Retry/Discard; Ready still requires explicit
acceptance. Schema 70 retains claims and immutable history proofs. Verification
includes 73 real-model assertions, packaged accepted-media export, the repository
gate completed in parts, all 68 runnable replay scenarios (four harness failures
corrected), and separate release responsiveness measurements. Two fixture-only
replays retain their explicit skips. The overall estimate remains about 86%.

[Counted AI insertion](qualification/ai-insertion-2026-10-07.md) now covers
`,a`, counts, `:ai-hold` and `:hold … video=ai audio=silence`. One transaction
saves the silent pause and durable generation intent. Existing-Hold provider
commands, explicit fallback reversion after later edits and native exact timing
are implemented. Schema 71 distinguishes inserted intent from accepted-pause
replacement. The gate, 79 real-model assertions, a model-free packaged export
and focused native checks pass; the final broader qualification is recorded
in that milestone's evidence.

[Source-boundary replacement](qualification/ai-boundaries-2026-10-07.md) now
commits changed source inputs, the required fallbacks and durable intent together
(schema 72). Preview, commit and full history replay independently derive the
final provider set, including dependent and cyclic Holds. The repository gate
completed in parts, with all four initial failures corrected and verified;
1,069 UI tests, 178 final replay checks, 119 real-model assertions and 30
model-free packaged checks pass. Four generated-picture exports verify 181
pictures and five audio windows. Across 40 prepared Slip edits, Apply to the
completed GPU picture was 22.094 ms at p95. Exact measurement limits, retained
failures and executable hashes are in the qualification record.

DP-12 remains Partial. Verified normal Extension integration and production
qualification are recorded below, alongside the open warm-runtime
and model-comparison obligations. These are separate from owner-only quality
checks.

The [extension foundations](qualification/extension-foundations-2026-10-07.md)
now implement exact generated-only sampling, native conversion and bounded
temporal context queries. All 1,717 affected-crate tests and strict workspace/UI
lint passed at that checkpoint; no new extension capability was advertised.

The [supervised extension worker](qualification/extension-worker-2026-10-07.md)
now retains bounded temporal inputs and runs both directions through protocol 3.
Two real model runs, independent native conversion and 51.6 million exact channel
comparisons pass, as does real cancellation without escalation. Verification
also includes 843 affected Rust tests, 95 Python tests, strict workspace/UI lint
and 265,506 mutation cases. The subsequent
[project-context capture](qualification/extension-context-2026-10-07.md)
now uses real qualified project footage, exact structural span coverage and a
bounded transition guard. Real-video tests cover hidden cuts, fractional clocks
and fades fragmented into one-frame beats.

[Durable extension inputs](qualification/extension-inputs-2026-10-07.md) now
share full temporal identity through preparation, stored requests, relevance,
automatic replacement and history (schema 73). Missing context retains the
chosen operation, restored context creates fresh work, and resource exhaustion
cannot become saved input authority. Quality admission, accepted extension
media, native controls and longer-duration qualification remain open. The
repository gate completed in parts: all 5,251 workspace tests are covered,
1,071 UI tests and both doctests pass, and formatting and strict lint pass.
The qualification retains the six corrected fixture failures and two isolated
pipe-closure diagnostics. Approved provider capabilities and the 88% estimate
are unchanged.

The [retained extension evidence](qualification/extension-evidence-2026-10-08.md)
now binds the saved descriptor, physical coverage and bounded picture signatures
to context schema 2. Host retention repeats the continuity rule from private
snapshots; the Python reader checks the same identity and byte contract.
Output admission and accepted extension media remain open.

The [extension pixel checks](qualification/extension-pixels-2026-10-08.md) add
generated-only motion/lighting coverage and actual sampled-join rejection,
including one-frame output and present unconditioned seams. The
[directional geometry checks](qualification/extension-geometry-2026-10-08.md)
add real single-anchor face/region inspection and generated-only chronological
mouth checks, with explicit unavailable coverage and preserved prior rejection.
The [complete host qualifier](qualification/extension-bundles-2026-10-08.md)
now binds all output checks, retained inputs, independent capability, exact clocks
and worker provenance in a revalidated immutable envelope. Store completion/Ready
and accepted extension media remained open at that checkpoint; qualification
alone changes no Hold.

[Saved extension acceptance](qualification/extension-saved-2026-10-08.md) now
retains the complete bundle through Ready, explicit acceptance, undo/redo,
portable copies and model-free reopening. The
[one-second measurement](qualification/extension-one-second-2026-10-08.md) then
runs both real model directions from actual project capture through complete
host qualification and an independent full-pixel oracle. Both saved fallbacks
remain unchanged. That milestone established separate Extension pack constraints
for temporal conditioning and exact duration. Its estimate remained 88% before
the normal integration below.

The [normal Extension workflow](qualification/extension-native-2026-10-08.md)
now connects the shared CLI/native preparation and attempt lifecycle to an
independent pack, both directions, explicit mode controls and automatic
replacement. The gate completed in parts; 1,080 UI tests, 123 Python tests and
128 targeted replay checks pass. All eight real 0.5/1/2/3-second cases pass both
directions, exact pixel sampling, explicit acceptance, Undo/Redo, cold reopening
and verified portable export with source-package, model/runtime and network
access denied. Native accessibility checks confirm saved controls, command
prefill and cancellation without an edit. The first relocated bundle refused
model activation because the strict smoke-report reader omitted an emitted
field; the corrected reader passes 18 focused tests and both model packs now
pass installation in the relocated bundle. All positive and tampering checks
pass. Two-second generation took 501.7–507.1 seconds; three seconds took
685.3–704.0 seconds. These are individual fresh-worker measurements, not a warm
latency distribution. DP-12 retains the unimplemented resident model/text cache
and required MPS/precision/performance comparisons. The overall coverage
estimate is about 89% (±5 points); four strict requirements remain Complete.

The same milestone exposed and corrected a [reordered video media-duration
defect](qualification/mux-duration-2026-10-08.md). A retained real HDR file
declared 47 ticks for 46 exact pictures. The encoder now proves the packet
timing before correcting only that header; independent export verification
remains strict. This does not close DP-17's broader output qualification.

The [release audit](RELEASE_AUDIT.md) records the continuation after
`ae79d614`, including retained failures and executable/source hashes.
Keyboard `ib` and `ab` now resolve beat objects through operators, Visual
selection and semantic programs. First-play attachment handling preserves
captions, cutaways and logical mark identities atomically. Descendant-owned
sounds now retain Repeat clock mappings through wrapping, count/gap changes,
copying and first-play isolation. Changed Repeat-owned and ancestor-spanning
sound processing still refuses pending owner-local routing. The follow-up
has passed 54 focused core, semantic, cache, PCM and persistence tests and
independent review, followed by the passing full gate below.
Corrections, URL entry and Marks use the shared layout-aware key routing.

Local diagnostic export is implemented in the native panel and CLI, with
bounded allowlisted JSON and no authored content or credentials. Worker
admission, cancellation and privacy tests, 27 instrumented fuzz targets,
complete Deno/V8 notice regeneration and a relocated signed bundle have
passed their focused checks. A follow-up run passed 68 tests, including the
corrected grammar expectations and both AI network-restricted launch paths.
Real packaged model checks, generation, explicit acceptance and verified
Render passed. Complete application replacement and rollback preserved the
project exactly. Independent review found no remaining DP-23 gap. The final
gate passed 4,700 workspace tests, 1,023 UI harness tests, strict lint and doc
tests; DP-23 is complete within §29.1's verification scope. Later paragraphs
retain historical checkpoints; the table
and this current record govern their status.

[Native online and archive workflows](qualification/native-install-2026-10-06.md)
now cover both import paths, native model download/install, explicit AI
acceptance, restart and verified export. The Documents export found an
asynchronous macOS document-tracking flag change; native recovery now confirms
that exact movie, and a fresh Documents export also publishes. Per-user backup cadence and retention
settings have passed six persistence tests, the focused native service tests
and 31 backup UI replay checks, plus native Save and restart. Model updates now revalidate installed retries,
retain their version lock through selection, and compare each safetensors
header with the qualified tensor schema. Thirty-five model-pack tests and
66 Python worker tests pass. Real packaged update, installed retry, generation,
explicit acceptance, rollback and verified Render also pass, with the selected
version retained in host and worker provenance. A real generation exposed and
fixed a missing worker asset-receipt field; the failure remains in the evidence.
The [follow-up gate](qualification/followup-2026-10-07.md#full-gate)
passed 4,734 workspace tests, 1,027 UI-harness tests, strict lint and doc tests.

The [adversarial suite](ADVERSARIAL.md) adds Gate G crash, chaos and
malicious-input testing on stable Rust: 33 seeded mutation targets cover source
container admission and codec records, isolated FFmpeg decoding and the real
conversion helper, every core JSON boundary with exact command inversion,
tampered packages, all framed worker protocols, the live-project socket,
yt-dlp metadata, model-pack archives and stored analysis evidence. They run
deterministically in the normal tests, and `cargo xtask chaos` runs
time-bounded and ASan/UBSan campaigns. The
[2026-10-05 record](qualification/adversarial-2026-10-05.md) ran 52.7 M cases
in 10 minutes and 38.0 M sanitized cases with no panic, abort, sanitizer report
or bound violation. Its one finding, register slot-row loss, rename or a
changed bank version validating undetected, is fixed by a register bank digest
(database schema 65) and pinned as a regression. A
10,000-beat, two-hour synthetic project passed its long-project stress budgets
in release. That checkpoint advanced DP-23 and Gate G; neither was complete then.

[Explode, duplicate and multi-play occurrence edits](EXPLODE_DUPLICATE.md)
convert a Repeat into an ordinary Sequence of independent plays, copy a beat,
span or range in place, and apply value edits to chosen plays or a range
inside one play, each as one reversible transaction. Explode keeps overrides,
gaps, escalation, captions, marks and Hold permissions, and closes retained
audio clocks for their concrete plays: decoded 30000/1001 PCM is bit-identical
through the raw, edge-faded and limited stages, and exploded fixtures hash
exactly like their committed [golden renders](GOLDEN_RENDERS.md) (Metal I420
pictures and limited-bus PCM on Apple Silicon with the pinned FFmpeg). A
[property suite](STRUCTURAL_PROPERTIES.md) drives random sequences of 27
structural command families, checking exact patches, duration accounting,
mark loss policies and retained clocks. This advances DP-03 and DP-04; neither
is complete.

[The bundled AI runtime and model pack manager](qualification/ai-runtime-2026-10-05.md)
put the private AI runtime inside `Deadpan.app` and install the 36.2 GB LTX
bridge pack through a verified, license-accepting manager with offline folder
and archive import. A scrubbed copy of the packaged app imported the pack
offline in 13.3 s and generated, accepted and rendered an AI pause, with
pixels identical to the development environment. The macOS 15 MLX build was
2.2 times slower, so AI pauses require macOS 26. This advances DP-12, DP-13,
DP-22 and DP-23; clean-machine tests remain open (Developer ID signing and
notarization are out of scope by owner decision, 2026-10-05).

[Copied sound clocks](OWNED_SOUND_VOICES.md#copying-retained-sound-clocks)
retain each attached sound's complete processing definition and chronological
placements across whole-beat/group copies, repeated pastes and recopies. Fresh
node and Repeat identities remain paired with their historical clocks. Capture
retains the source's final placement so paste cannot revive previously clipped
samples. Supported ordinary Sequence moves preserve those histories; current
Hold gates, edges and gain still apply. Register admission recaptures the named
immutable revision, while preview and commit recheck qualified source evidence.
This advances DP-04 and DP-09. Edits inside a surviving processing branch,
partial copies, allowances and native beat-sound placement remain open.
See the [qualification record](qualification/sound-slice-clocks-2026-10-03.md)
for review, retained evidence and limits. The full workspace passes 3,600 tests
with none failed or ignored. All 40 focused regressions, formatting and strict
workspace lint pass on the same unchanged source inventory.
At that checkpoint no product requirement or release gate was complete.

[Occurrence sound preparation](OWNED_SOUND_VOICES.md) projects one independent
catalog recipe from a checked current beat occurrence through its enclosing
time maps. Each Preserve stage retains that voice's complete processing history;
current silent Holds gate the final root samples. Stable Repeat paths select
actual plays and overrides without expanding unrelated plays. A bounded window
query now finds all relevant current occurrences, including processed output
retained by an outer crop, and the PCM adapter sums their independent histories
under shared limits. Saved owner/local-ID recipes now use typed Set/Delete,
source admission, reversible history and whole-owner copies. The authored bus
applies per-occurrence processing, edges and gain before one common limiter.
This advances DP-04 and DP-09 groundwork. Complete temporal edit/partial-copy
transforms, allowances and native placement remain
required before `ib`/`ab` can include attached sounds. The
[single-occurrence record](qualification/owned-sound-voices-2026-10-03.md) and
[batch record](qualification/occurrence-batches-2026-10-03.md) retain review,
source inventories, checks and limits.
The [saved-sound record](qualification/saved-beat-sounds-2026-10-03.md)
tracks the persistence and authored-bus increment. The full workspace run
recorded 3,546 passes and four outdated format assertions. All four affected
targets pass after test-only corrections; final formatting and strict lint pass.
No requirement or gate is complete.

The borrowed [occurrence route](OWNED_SOUND_VOICES.md#retained-occurrence-sample-routing)
now transports a voice's old integral PCM labels through chronological edits,
retaining its complete nested Preserve history and intermediate clipping.
Current consuming gates, edges and gain remain outside this raw provider.
This provided preparation groundwork for the saved independent clocks above. The
[qualification record](qualification/routed-occurrence-pcm-2026-10-03.md)
records 756 passing planner/audio tests, formatting and strict workspace lint,
with fixture corrections and source inventories retained.

[Group objects](EDITED_SLICES.md#group-objects) add native `ig`/`ag` to
copy/cut/Repeat operators and typed Visual selection in ordinary Sequence
scopes. Registers, semantic recording and supported dot retain object intent;
Group and Original/Edited paste or Place replacement preserve exact structural
ownership. Scope receipts distinguish an inner edit from its surrounding
navigation. Empty contents stay explicit and can receive a paste. This advances
DP-05, DP-06 and DP-21. The
[qualification record](qualification/group-objects-2026-10-03.md) carries the
actual checks and limits. Beat objects `ib`/`ab` still need their owned temporal
attachment lifecycle; analysis/role/occurrence selectors and every full-product
requirement and release gate remain open or partial.

[Exact sibling selections](STRUCTURAL_SELECTIONS.md) add identity-based
capture, Group, Repeat and atomic deletion that retain empty children at both
endpoints. This provides the structural foundation for the group objects above
in DP-05, DP-06 and DP-21.
[Qualification](qualification/sibling-selections-2026-10-03.md) records
305 core, 90 store and 73 real-audio tests, 748 default and 784 optional app
tests, 846 rendered placement checks and strict lint at that checkpoint. The
later group-object implementation adds its keyboard, Visual and scope behavior;
beat-owned temporal attachments remain required.

[Named groups](GROUP_EDITING.md) adds exact beat/range grouping, neutral
Ungroup, captured names, recording and dot in ordinary Sequence scopes. Headless
Macro results report complete final mark changes. This advances DP-05, DP-06
and DP-21; framed/treated Ungroup, temporal occurrence editing, saved gag recipes
and full slice placement acceptance remain open.
[Qualification](qualification/group-editing-2026-10-03.md) records 747 default
and 783 optional app tests, 1,416 rendered checks, strict lint, exact PCM/GPU
comparisons and native register/macro/Undo verification. The full workspace run
had five new assertion failures; all three affected targets pass in full after
the documented test corrections. The native QA app exited cleanly.

Current [development formats](DEVELOPMENT_FORMATS.md) are database 58/core 46,
refusing earlier development packages before writes. Current recovery and
embedded audio-context codecs remain. Historical
evidence below describes its recorded revision, not current format admission.

[Scoped editing](SCOPED_EDITING.md) adds native navigation inside Repeat/Retime
contents and explicit All plays/This play gain, Camera and Hold audio editing.
Browsing is read-only; changed values isolate only selected shared ancestors
and retain mapped receipts. Cached projection preserves exact clips and sample
centers through Retime. This advances DP-05, DP-06, DP-08 and DP-21.
[Qualification](qualification/scoped-plays-2026-10-03.md) records the 3,780-test
workspace gate, final app checks in both configurations, 1,083 rendered workflow
checks, strict lint and native verification. Mark-only saves preserve the current
play and any navigation made while the save is pending.
Temporal occurrence editing, implicit gap recipes, complete
definition previews and occurrence macros remain open, along with every gate.

[Repeat selections](REPEAT_SELECTION.md) adds `r` with a motion, Visual `r`,
whole-beat `rr`, captured command targets, semantic recording and dot for Repeat
wraps. Range endpoints retain their composite contexts and one Undo restores the
whole edit. Safe count changes preserve surviving plays, gaps and suffix clocks;
independent root sounds move once. This advances DP-05, DP-06 and DP-21. Total-play
setters now share semantic recording and dot, retaining the requested count while
resolving a new explicit selected Repeat. Temporal occurrence edits, text/role selectors, the remaining
editing surface and all product gates remain open.
The [qualification](qualification/repeat-operator-2026-10-03.md) records 3,292
passing tests, 1,094 rendered workflow checks and native keyboard verification.
The later [count-setter qualification](qualification/repeat-count-2026-10-03.md)
records the 3,799-test workspace gate, 764 optional app tests, 1,043 rendered
checks and native record/run/Undo verification. These overlapping configurations
are reported separately. The QA app exited cleanly.

[Cut selector repeat](SEMANTIC_REPEAT.md) extends `.` to typed motion cuts,
whole beats and Visual cuts. Current Visual ranges override retained selectors;
empty ranges refuse, and saved Visual cuts require a new selection. The requested
count/direction and destination register survive retargeting. Versioned Apply
admission, exact retries and saved-cut proofs preserve intent through refresh
failures without reusing old coordinates. Recording dot retains its effective
instruction. This advances DP-06; remaining editing operations, semantic
text/role/occurrence selectors and all product gates remain open.
See [qualification and retained failures](qualification/selector-repeat-2026-10-03.md).

[Operator motions](SEMANTIC_MACROS.md) add Normal Edit `y`/`d` plus frame,
beat or group-boundary motions, with explicit `yy`/`dd` whole-beat selection.
The native router, CLI and macros share typed selectors and exact historical
Child/Range captures. Copy-only operations preserve history; counted macros
commit once. Prefixes retain their captured context, including absent targets,
and stale completion cannot reclaim cursor ownership. This advances DP-05,
DP-06 and DP-21. Text objects, analysis-dependent motions, broader
dot-repeat and all product gates remain open.
See [qualification and retained failures](qualification/operator-motions-2026-10-02.md).

[Visual macros](SEMANTIC_MACROS.md) extend recording and replay with oriented
Visual selections, frame/beat/group-boundary motion, range copy/cut and atomic
Original or Edited replacement. Programs can start with an existing selection
or construct a relative range. Empty selections refuse without beat fallback;
copy-only execution retains timeline history, and counted authored runs share
one Undo. Final selection follows the exact owned native receipt. This advances
DP-05, DP-06 and DP-21; full operator grammar, semantic text/role/occurrence
selectors, remaining edits and all product gates remain open.
See [qualification and retained failures](qualification/visual-macros-2026-10-02.md).

[Macro reuse](SEMANTIC_MACROS.md) adds selected-beat yank and before/after
register paste to recording and execution. Original and Edited copies use the
same staged planner in native and headless requests. Selection is independent
of the cursor, including empty siblings; copy-only runs save the bank without
changing timeline history. Counted authored runs retain one Undo. Original
mappings come from measured qualifications and are independently checked at
store admission. This extends DP-06 and DP-21; broader selectors, remaining
editing instructions and the complete product acceptance remain open.
See [qualification and retained failures](qualification/macro-reuse-2026-10-02.md).

[Headless Macro commands](SEMANTIC_MACROS.md#headless-inspection-save-and-run)
inspect consistent revision/bank snapshots, save named programs and run the
supported vocabulary from explicit Sequence coordinates. Both
closed and open-native projects share full dry-run admission and atomic commit.
Runtime copies prepare before save; exact bank-only receipts survive refresh
and reply-detail failures. Remote motion returns its position without moving
the GUI cursor. See [qualification and limits](qualification/headless-macros-2026-10-02.md).
This advances DP-06 and DP-21; neither requirement nor any gate is complete.

[Semantic macros](SEMANTIC_MACROS.md) add `q` recording and counted `@` calls
for relative frame motions, frame cuts and named programs. Schema 55 persists
typed Macro registers without changing the default copy or timeline history
when saving. Each authored run resolves sequentially and commits as one Undo;
late failure preserves both the document and bank. Native keyboard delivery,
record/save/run/Undo and exact persisted instructions were checked. The full
workspace run passed 3,633 tests; final app suites, strict lint and 823 rendered
workflow checks cover the final receipt and assertion updates. See
[qualification and limits](qualification/semantic-macros-2026-10-02.md).
Broader instructions, semantic selectors, occurrence contexts and full
layout/IME qualification remain DP-06 work.

The original [semantic frame-cut repeat](SEMANTIC_REPEAT.md) increment added `.` for the requested
count from the last committed `x` or `:delete-frames` operation, resolved at the
new current Edit cursor. A clamped first cut retains its original count. The
service observes actual saved revisions, preserves intent through marks and
Undo/Redo, and clears it after unsupported edits even when refresh fails.
Versioned feedback and requests reject stale repetition.
The selector extension above builds on that foundation. Other edit kinds remain open.
See [qualification and limits](qualification/semantic-repeat-2026-10-02.md).

[Resolved compound transactions](COMPOUND_TRANSACTIONS.md) stage ordinary edits
and copy/cut/paste register operations, then save one reversible history entry.
Per-step media admission prevents an invalid intermediate operation from hiding
behind a valid final document. Schema 54 retains capture checkpoints and every
leaf allocation, so an intermediate copy remains usable after Undo and reopen.
Bank-only programs preserve the edit history. This supplies a DP-06 execution
boundary; semantic text/role/occurrence selectors and repeat for other edits remain open.
See [qualification and limits](qualification/compound-transactions-2026-10-02.md).

Native sound edits, whole-Original insertion and first-Original initialization
now retain their saved revision before refreshing the workspace. A failed
refresh reports durable success and asks the user to reopen; stale views cannot
consume the receipt. Catalog registration retains its saved asset without
inventing an edit receipt. See [verification and limits](qualification/saved-receipts-2026-10-02.md).
This fixes a prerequisite for reliable semantic recording; broader dot-repeat
remains open. Macros now use the shared atomic Compound boundary.

[Project registers](NAMED_REGISTERS.md), introduced in SQLite schema 53, persist
a–z and default copies of Original moments and editable slices. Copies leave edit
history and Undo/Redo unchanged; cuts save deletion and registers atomically.
Reopening restores historical provenance under fresh runtime identities.
Versioned bank updates remain independent of stale UI confirmations, and new
placement waits for a pending save. Unique contents are bounded at 64 MiB.
Broader Macro instructions and dot-repeat for remaining editing operations are required for DP-06. The
[qualification](qualification/durable-registers-2026-10-02.md) records 3,534
distinct passing default tests across the full run and corrected targets,
3,896 rendered checks across 28 ordinary scenarios, and native save/reopen with
unchanged timeline history. No requirement or gate is complete.

The [compact Original layout](qualification/original-layout-2026-10-02.md)
returns picture space through a Sounds count heading, compact clocks and
measured transport rows. Focus and scale-change replays retain both cursors,
selection and copy while keeping complete controls visible. This advances
DP-05 and DP-20; full keyboard, physical-input, accessibility and visual
acceptance remain open. No schema changes or product gate completion.

The [editor keymap](KEYMAP.md) now loads optional Normal/Visual overrides from
macOS Application Support. Complete-map validation, logical or physical matching,
semantic labels and persistent fallback diagnostics share the production router.
Held custom motions retain their resolved action; ordered native text and IME
keep ownership after Command/Search entry. The remaining mode maps, strict
logical provenance, physical layout qualification, broader dot-repeat
and broader Macro instructions remain open. This does not complete DP-05, DP-06 or any gate.
The [qualification](qualification/configurable-bindings-2026-10-01.md) records
review, rendered replay and verification limits.

Native [frame cuts](EDITED_SLICES.md#frame-cuts-at-the-cursor) add `x`, counted
`12x` and captured `:delete-frames Nf` to DP-05's ordinary Sequence workflow.
One atomic cut updates history and the session copy, with the actual interval
reported after save. Group-end clipping is explicit; unsupported partial
composites and ineligible focus refuse without edits. The full grammar, nested
occurrence cuts and all product gates remain open or partial.
The [qualification](qualification/native-frame-cuts-2026-10-01.md) records
final app suites, three rendered regressions and the retained failing witnesses.

[Deadpan's identity assets](design/brand/README.md) now include a genuine layered
macOS icon, complete legacy fallback, logos and web formats. Native bare/bundled
startup and an AppKit icon readback pass; 15 logo PDFs render without clipping
and contain vector paths without embedded fonts or images. This is branding and
developer-bundle groundwork for DP-20. Signed, standalone distribution and the
full release gates remain open.

[Packaging](PACKAGING.md) adds `cargo xtask bundle`, which produces a
relocatable, hardened-runtime `Deadpan.app`. It contains release executables,
FFmpeg relocated to `Contents/Frameworks`, a pinned yt-dlp/Deno baseline the
running bundle prefers, generated notices, a CycloneDX SBOM and an `otool`
audit. `bundle-verify` passed on a scrubbed relocated copy with an ad hoc
signature, including tampered and deleted helper cases, and a real YouTube import also succeeded. This advances DP-22, DP-23
and Gate F. Developer ID signing and notarization are out of scope by owner decision
(2026-10-05); clean-machine acceptance remains open.

[Native saved-render recovery](qualification/render-history-2026-09-30.md)
adds bounded keyboard browsing through `Renders` and `:renders`. Saved jobs,
encoding checkpoints and previous destinations retain exact historical targets
through native pickers and project changes. The production replay passes 112
checks, publishes four real movies and reconciles an existing destination without
changing the current edit. IME confirmations cannot activate modal actions; Tab
reveals focused controls and Escape restores editor focus after the modal closes.
New owner-started workflows open their current-session status. Full mastering,
HDR, preparation controls and complete product acceptance remain open.
Native keyboard browsing and save-sheet cancellation also pass with all 20
stored tables unchanged. Native AX omitted the custom overlay controls, so its
accessibility acceptance remains open despite the passing harness tree checks.
The locked workspace suite passes 2,578 tests; 344 optional UI-feature tests,
strict lint, formatting and native Metal startup/shutdown also pass.

[Returning to the same Original](qualification/source-return-2026-09-30.md)
now preserves its cursor through catalog sound selection and retains the
displayed picture while refreshing. The production sound replay reproduces the
old reset and passes with distinct Original/Edit positions after the fix.
This closes the focused issue found in owner-preparation inspection; full sound,
keyboard, accessibility and UI acceptance remain open.

[Open-project commands](LIVE_PROJECT.md) now route structural edits, history,
primary geometry and Render through the native writer's authenticated local
endpoint. Exact owner/revision targeting, independent replies, retained commit
receipts and explicit preview refusal preserve the native session. Original
retention, relinking, registration and checkpoints now prepare on its bounded
import worker and complete through the captured owner. Registration preserves
the caller's exact revision, identities, insertion and selected streams.
Checkpoints use a consistent read-only SQLite backup and retain the actual
captured revision, including a published receipt if final directory sync fails.
Full mastering, HDR and expanded output/release
qualification remain required. No DP requirement or delivery gate is complete.

[Owner-preparation qualification](qualification/owner-preparation-2026-09-30.md)
records 28 actual CLI invocations, including nine failed stream preparations
followed by successful registration, exact/no-op/stale relinking and a consistent
checkpoint. Native catalog/picture refresh and keyboard navigation were inspected
without reopening; operational changes preserved authored history. The locked
workspace suite passes 2,572 tests; 338 optional UI-feature tests, strict lint,
formatting and native startup/shutdown also pass. Native relink/checkpoint controls, complete
preparation/recovery acceptance and representative preparation performance remain open.

[Open-project qualification](qualification/live-project-2026-09-30.md) covers
36 real CLI invocations against a native writer, three actual preview refusals,
concurrent editing during Render, exact cancellation, historical retry and
reconciliation. Independent readers pass all 768 picture planes and complete
authored audio in the initial/retry movies. The retained full workspace run and
scoped corrections cover 2,523 distinct passing tests; 330 optional UI-feature
tests, strict Clippy, formatting, native startup/shutdown, 79 visual Render checks
and 2,426 release replay checks pass. The saved-render boundary above adds native
persisted recovery and progress access for owner-started jobs. The endpoint crash
matrix remains open.

[Native and public Render](RENDER_JOBS.md#native-and-public-render) expose
the automatic SDR workflow through Cmd-E, `:render` and closed-project headless
commands. Camera, Gain and Room tone previews require an explicit decision and
bind rendering to the exact durable commit receipt. The production replay covers
real publication and later Undo; the public workflow covers checkpoint retry,
reconciliation, stale/collision refusal, SIGINT cancellation and JSON re-encoding.
[Qualification and retained evidence](qualification/public-render-2026-09-30.md)
record 2,463 passing workspace tests, 318 optional UI-feature tests, strict Clippy,
formatting, native startup/shutdown, 71 Render replay checks and 2,418 release
replay checks. Independent readers pass all 768 picture planes and complete
authored audio in both public exports. Final decision/result captures were inspected.
That qualification covers closed-project CLI commands; the open-project boundary
above extends the entrypoints without changing its media qualification scope.

The following milestone notes retain their original measured scope; the current
entrypoint status is described above.

[Durable automatic encoding](RENDER_JOBS.md#automatic-admission-and-recovery)
now retains an automatic job policy and an immutable decision per fresh encoding
attempt in database 42. Qualification stays Queued until the decision and
Encoding transition commit atomically. New encodes qualify again; checkpoint
retry and reconciliation preserve their original decision and freshly verify
the movie. Strict legacy adapters preserve engineering data and reject newer
vocabulary in old schemas. Native/public headless Render, complete output
qualification and the full DP-17/DP-18 requirements remain open.

[Qualification](qualification/automatic-render-jobs-2026-09-30.md) passes two
fresh native encodes, four fresh verifications, reopened checkpoint retry and
reconciliation. Independent readers check all 768 picture planes and complete
authored audio in both files. Every old cell in 19 database tables is preserved.
Final workspace coverage is 2,439 passing tests after one historical migration
expectation correction; all 310 optional UI-harness app tests, strict Clippy,
formatting and native startup/shutdown pass. The failed runs remain retained.

[Runtime-bound project encoding](qualification/encoder-runtime-2026-09-30.md)
now consumes a fresh automatic admission. The probe and project worker retain
matching mapped helper/library descriptors and require stable hashes, platform
facts and frozen SDR controls. The native example verifies a full 128-frame,
205,005-sample committed project and preserves every cell in 19 database tables.
Independent readers pass every picture plane and the complete authored audio;
final workspace coverage is 2,397 passing tests after correcting one transport
fixture. Strict Clippy, formatting and native startup/shutdown pass.
Serialized evidence cannot restore a live admission. Native/public headless
Render and full output qualification remain open; no DP
requirement or delivery gate changes status.

[Automatic encoder probes](AUTOMATIC_ENCODER_ADMISSION.md) now select a path
through fresh supervised deterministic encodes, complete file verification and
expected picture/audio checks. Ordered typed rejections remain in the decision;
non-capability and cleanup failures stop selection. The durable job/attempt
boundary is described above; public Render remains required for DP-17/DP-18.
[Qualification](qualification/encoder-admission-2026-09-30.md) records 2,384 passing
workspace tests, strict Clippy, formatting and native startup/shutdown. Four real
cases passed 190 pictures and 24 exact AVFoundation event positions. The 14x16,
16x16 and 64x64 probes explicitly failed decoder geometry admission and remain
unqualified; their failures and confirmed cleanup are retained.

[Typed encoder failures](qualification/encoded-failures-2026-09-30.md) preserve
exact native failure kinds through supervision and invalidate them after later
protocol, timeout or cleanup faults. Real media reproduces the hardware B-frame
rejection and separately verifies a no-B encode without authored changes. The
native matrix passes 872 pictures and 180 exact audio event coordinates. All
2,359 workspace tests, strict Clippy, formatting and native startup/shutdown pass.
This is DP-17/DP-18 groundwork; public Render remains open.

[Native marks](MARK_NAVIGATION.md) add persistent case-sensitive letter positions,
exact Original/Edit jumps, a keyboard-accessible list and bounded back/forward
navigation. Mark-only saves preserve editorial selection and history positions;
unrelated revisions expire transient Edit history while saved marks continue to
follow core transforms. Original history retains its qualified measured clock.
This remains partial DP-05/DP-20 work; full operator grammar, occurrence navigation
and complete physical-input/accessibility acceptance remain required.
[Qualification](qualification/native-marks-2026-10-01.md) records 448 UI-feature
and 412 default app/headless tests, formatting, strict workspace lint, 156 focused
rendered checks plus the expanded Kestrel audit, 3,589 full release checks, and
native keyboard/reopen verification with consistent SQLite backups. No release
gate is completed by these results.

[Native Trim](COMBINED_TRIM.md#native-trim) adds an implemented subset of
DP-02/DP-05/DP-20: `,v`/`:trim`, four ordered accepted In/Out/Slip/Roll values,
Ripple/Overwrite policy, paired Before/Proposed junction pictures, an admitted
Edit waveform and optional boundary audition. Apply requires the acknowledged
nonzero proposal and its current Proposed pair at the final viewer raster, then
saves one reversible transaction. Cancel preserves authored state; zero intent
has no proposal or history. Captured scope, target/right absence and both editor
cursors stay independent of inspection and heard position. Admission remains
limited to eligible direct Source/neutral unity Partition children of ordinary
Sequences; Original, Sounds, composites and Edit Visual ranges refuse entry.
The [native qualification record](qualification/native-trim-2026-10-01.md)
owns execution evidence and its limits. Broader target admission, the complete
editor workflow, physical keyboard/IME, accessibility and listening acceptance
remain open. No DP or gate status changes.

The following Trim/Slip prerequisite summaries retain their historical results
and checkpoint scope; those counts do not qualify the current native increment.

[Native Source Slip](SOURCE_SLIP.md#native-stopped-picture-preview) adds a
verified subset of DP-02/DP-05/DP-20. The `:slip`
workflow captures the selected beat and revision, reports exact clamps/no-op,
compares real stopped Before/Proposed pictures, and gates one Apply on the
current submitted picture. It preserves both cursors and retains a saved receipt
if workspace refresh fails. The app passes 464 feature-enabled tests plus 3
headless tests, and 428 default-feature tests plus the same 3 headless tests.
The focused replay passes 67 Slip checks; the full release replay passes 3,655
checks. Native cancel, apply, Undo/Redo and reopen pass, with both test instances
closed. See [qualification](qualification/native-slip-2026-10-01.md). At that
checkpoint, full Trim, waveforms/audition, unsupported target scopes and
physical-input/accessibility acceptance remained open. No requirement or gate
changes status.

[Combined Trim authoring](COMBINED_TRIM.md) is implemented,
including one accepted In/Out/Slip/Roll command, fixed-duration overwrite and
qualified store/CLI admission. Core 43/database 52 identify its vocabulary.
[Qualification](qualification/combined-trim-2026-10-01.md) records 3,366 passing
workspace tests and both documentation tests. After one equivalent stable-sort
cleanup, strict lint, 132 core tests and final formatting passed. Native Trim
was outside that backend checkpoint; no requirement or gate changes status.

[Combined Trim timing primitives](TRIM_DRAFT_FOUNDATION.md) advance
DP-02/DP-05. Complete-intent geometry, Source endpoint bindings and the root-sound
Trim map passed independent review and focused runtime checks, including 57 new
tests. The full workspace passed 3,333 unit/integration tests and both
documentation tests; strict workspace/all-target lint and formatting passed on
the same unchanged source inventory. The combined authoring command and overwrite
overlay are qualified above; native Trim was outside this foundation checkpoint.
No requirement or gate changes status.

[Adjacent Source Roll](SOURCE_ROLL.md) adds a backend subset for DP-02/DP-05.
It resolves one shared movement against both exact
Source limits, preserves combined duration, captures audio clocks once, retains
the independent sound bus and marks only the changed seam. Core 41/database 50,
store admission and cold/live headless dispatch are integrated.
[Qualification](qualification/source-roll-2026-10-01.md) records independent
picture, decoded-PCM and persistence checks. The full workspace passed 3,275
tests and both documentation tests with one stale doctor assertion; only that
test changed and its rerun passed. Strict all-target lint and formatting pass.
That backend checkpoint claimed no native Roll surface.
Full Trim requirements remain open; no requirement or gate changes status.

[Ripple Source edge trimming](SOURCE_TRIM.md) adds a verified backend subset
for DP-02/DP-05. The atomic command moves one In/Out edge with exact handle
clamping, retained physical owners, old-clock audio captures and one root sound
transform. Separate editorial audio intent adds the required fades at new Trim
edges and both Slip joins without changing raw sampling support.
[Qualification](qualification/source-trim-2026-10-01.md) records indexed-picture,
decoded-PCM and store/headless checks. The full workspace passed 3,250 tests and
both documentation tests with one outdated Slip assertion; only that test changed
and its rerun passed. Default app checks, strict all-target lint and formatting
also pass. The failed invocation and exact source comparison remain recorded.
Native Trim, overwrite, Roll and broader target admission remained open then; no
requirement or gate changes status.

[Exact Source windows](SOURCE_EDIT_WINDOWS.md) retain selected time before
whole-frame enclosure. [Atomic Source Slip](SOURCE_SLIP.md) shifts both linked
maps in one transaction under ordinary Sequences, preserving duration, effects,
bindings and root sounds. Stored admission is rechecked at commit; headless dry
runs expose exact handle clamping and no-op results. All 3,114 workspace
unit/integration tests, both documentation tests, formatting and strict all-target
Clippy with `ui-harness` pass. See [qualification](qualification/source-slip-2026-10-01.md).
Native Trim controls were outside that checkpoint; the command's explicitly
unsupported scopes remain open.

[Source effect clocks](SOURCE_EFFECT_CLOCKS.md) preserve existing camera paths,
gain envelopes and mute ranges when a physical Source grows earlier or later.
Core 38/database 47 retain explicit framing domains; gain translation uses the
existing owner coordinates. All 3,046 workspace unit/integration tests, both
documentation tests, formatting and strict workspace/all-target Clippy pass,
including lint with `ui-harness`. Physical growth, complete Trim commands and
native boundary previews remained open at that checkpoint. No
requirement or gate changes status.

[Retained Source origins](SOURCE_ORIGINS.md) add an exact translation between
current physical coordinates and captured audio clocks. Pure binding rebases
preserve sample grids, frozen layouts and chronological phase composition.
Decoded-PCM regressions pass for physical prefixes, fractional NTSC phase,
independent sample offsets and inverse restoration. Core 37/database 46 persist
the translation. All 3,019 workspace unit/integration tests, both documentation
tests, formatting and strict all-target Clippy pass. The later effect-clock work
above adds framing/treatment preservation. Atomic Trim authoring and native
controls remained open then; no requirement or gate changes status.

[Dormant linked audio](qualification/dormant-linked-audio-2026-10-01.md) retains
the Original's audio context in silent slices. Empty support emits silence without
requesting source PCM, and selection growth preserves the full affine mapping and retained
sample clock. Absent audio remains distinct. Core 36/database 45 and audio context
5 retain the new meaning while supported older grammars stay closed. This is
further Trim groundwork for DP-02/DP-05/DP-20; physical-origin translation, effect
owner clocks and the native Trim workflow remained required then. No requirement
or gate changes status.

Verification covers 2,169 distinct affected-crate tests and both documentation
tests, including no-read silence, independently expected PCM after selection
growth and durable linked context through reopen/Undo/Redo. Independent review
has no remaining findings. Workspace formatting and strict all-target Clippy pass.
No native app was opened for this increment; the final process check found none
running.

[Exact picture context](qualification/selected-video-context-2026-10-01.md)
retains the complete measured video span in new Original slices while keeping
their visible endpoints separate and exact. Rounded tails hold the last selected
picture; inverse source anchors reject hidden context. Audio placement and beat
duration remain unchanged. Core 35/database 44 reject unused development
databases 39 through 43 under the approved format policy, with old supported
grammars kept closed. This is a Trim prerequisite for DP-02/DP-05/DP-20;
native In/Out/Slip/Roll, ripple/overwrite and the complete visual Trim workflow
were outside that checkpoint.
Verification covers 2,956 distinct workspace unit/integration tests and both
documentation tests across the broad run and corrected storage run. All 270 plan
tests pass again after a private storage-layout adjustment; final strict
workspace lint and formatting pass. The evidence retains the failed attempts
and their corrections. No native app was opened, and no requirement or gate
changes status.

[Visual slice placement](spec/DEADPAN_SPEC.md#97-visual-slice-placement) is an
explicit DP-05/DP-20 requirement: select and refine a time range, see its source
endpoints and destination, audition both proposed joins, then commit or cancel
with the keyboard. Insert, replace and move require exact reversible semantics,
visible occurrence scope and stale-revision rejection. [Place slice](SLICE_PLACEMENT.md)
now previews linked Original copies at ordinary Sequence seams and direct
Source/ordinary Hold/supported-fragment interiors with decoded
endpoints, local In/Out refinement, a provisional timeline, Before/Proposed and
both-join audition. Interior placement captures its direct child and local
boundary, retains the split's original audio clocks and commits the split and
insertion as one command. The initial seam replay passed 215 checks including minimum
layout, complete Tab circuits, synthetic IME, cancellation, stale captures and
exact commit/undo. Actual decoded pictures and canonical nonzero PCM around both
joins match the committed result. Edited-slice copy/move,
picture/audio-only policies and Repeat/Retime occurrence targets
remain required; this does not complete DP-05 or DP-20.
[Qualification](qualification/slice-placement-2026-09-30.md) records native release
looping and pause/resume, unchanged SQLite rows after cancellation, all automated
checks and the measured debug-build audio starvation limitation.
[Interior placement qualification](qualification/interior-slice-2026-09-30.md)
adds Source/Hold/fragment insertion with one commit/Undo, exact retained suffix
PCM, failed-transaction rollback and 267 production replay checks. Raw count
batches and clamped motions across empty-group slots are covered by reproduced
failures and passing regressions. All 2,609 workspace and 362 UI-feature app
tests pass, with strict lint and formatting. Native release commit/Undo restores
all authored fields; the database records exactly one placement and one Undo.

[Visual replacement](qualification/slice-replacement-2026-09-30.md) adds an
independent Edit range, explicit Replace selection preview and fast `p/P`
replacement. The removed range stays fixed while source endpoints change.
One command preserves endpoint contexts and routes sounds directly from old to
final sample boundaries; one Undo restores the entire authored document. Core
34/database 43 use the approved development format break for databases 39–42.
The locked workspace passes 2,641 tests, the production replay passes 401 checks
and the UI-feature app passes 372 tests,
including pointer/keyboard switching, stale targets, exact comparison windows
and saved receipts surviving preview-refresh failure. Native release QA verifies
all four join pictures, one 14-frame replacement of `[30..60)` and one Undo.
SQLite backups confirm complete authored restoration and unchanged unrelated
tables. This remains partial DP-05/DP-20 work.

[Retained deletion](qualification/ripple-delete-2026-09-30.md) corrects the
native and CLI beat-delete path. Removing an ordinary Sequence child now keeps
each downstream structural audio owner's old sample entry and transforms the
root sound bus once. A real NTSC/44.1 kHz witness reproduced 6,405 changed samples
in a 6,406-sample suffix under historical Delete; the new command preserves the
complete decoded suffix. Core 34/database 43 stay unchanged, with old and new
commands replayable in the same history. The locked workspace passes 2,661
tests and the UI-feature app passes 375, with none failed or ignored.
Formatting and strict all-target workspace Clippy with the UI harness feature
pass. Visual range deletion is recorded below; role-only and temporal occurrence
deletion remain required. No DP requirement or gate changes status.

[Visual range deletion](qualification/delete-range-2026-09-30.md) adds one atomic
linked cut through ordinary Sequence scopes. Active and finished half-open
selections use `d`; empty selections cannot fall back to whole-beat deletion.
`:delete` captures its exact range or beat on entry, including absence. Endpoint
splits retain original clocks, downstream content retains its old sample entry,
and root sounds transform once. Six decoded-PCM tests cover NTSC Source/RoomTone,
earlier clocks, whole Repeat gaps, Preserve output and aligned cuts; persistence
and CLI tests retain exact one-command history and stale rejection after Undo.
The production range replay passes 98 checks, the slice regression passes 400,
and each passes all 11,904 Kestrel routing cases. Core 34/database 43 are unchanged.
Separate native QA verifies empty rejection, one cut, the exact rendered join
pictures and Undo. Consistent SQLite backups confirm one edit/Undo plus a
separate Redo/Undo picture check, full authored restoration and released locks.
The locked workspace passes 2,685 tests and the UI-feature app passes 380, with
none failed or ignored. Formatting and strict all-target workspace Clippy with
the UI harness feature pass on Rust 1.97.1.
Role-only deletion, temporal occurrence interiors, motion/text-object operators,
edited-content registers and copy/move remain open.

The [edited slice core](EDITED_SLICES.md) adds immutable capture and atomic seam
insertion through typed core/headless commands. Partial Source/Hold/Partition
windows retain complete owner contexts, historical clocks and bias-filtered
marks; each paste has independent authored/play/timing identities. Tests compare
exact picture plans and decoded NTSC PCM, repeated and partial copy-of-copy,
untouched destination content, historical media admission and durable Undo/Redo.
The store recaptures the named immutable revision before reusing its media, so
copies survive source deletion without accepting forged or unselected assets.
See [qualification](qualification/edited-slice-2026-09-30.md). Native edited
registers, placement, refinement and replacement, atomic move, cut-to-register and
temporal occurrence interiors remain open. No DP requirement or gate changes
status. Independent review is complete; all 2,717 locked workspace tests,
formatting and strict all-target workspace Clippy pass on Rust 1.97.1, with none
failed or ignored. Core 34/database 43 remain unchanged.

[Edited interior placement and replacement](qualification/edited-placement-2026-09-30.md)
adds `SpliceSliceAt` and `ReplaceSlice` through the same typed core/headless path.
Destination endpoint splits retain original lattices; replacement captures the
old suffix before removing selected children. Conditional clock allocation,
joint identity pools and one root-sound transform retain exact undo semantics.
Focused verification passes 126 core, 15 decoded-PCM, seven picture-plan,
39 store and two CLI tests. Independent review found no implementation defects.
All 2,739 locked workspace tests, formatting and strict all-target workspace
Clippy pass on Rust 1.97.1, with none failed or ignored. Native edited registers and
placement controls, atomic move and temporal occurrence interiors remained open
at that backend checkpoint. The native extension is recorded below.

[Native edited slices](qualification/native-edited-slice-2026-09-30.md) add
history-neutral `v`/motion/`y` capture to a shared session register. Accepted
copies survive later edits and Undo. `p/P` inserts or replaces atomically;
`:splice` refines the captured historical parent, displays composed copied
endpoints and previews the exact destination. Historical picture/audio access
uses store-issued admission with live session checks. Unsupported destinations
retain usable source endpoints, and saved receipts cannot consume a stale
visible selection. The production slice replay passes 547 checks plus the
11,904-case shortcut audit; the retained-project run adds one storage-root check.
Native keys verify copy/refinement, one insertion and Undo. SQLite backups prove
copy/cancel leave all 20 tables unchanged and both Undos restore complete authored
state. The corrected full release replay passes 3,110 checks; compact Sound,
Room tone and Gain viewers retain their existing minimum picture sizes.
All 2,766 locked workspace tests, formatting and strict all-target workspace
Clippy with the UI harness pass on Rust 1.97.1, with none failed or ignored.
Atomic move, persistent/named registers, cut-to-register, role-only placement and
temporal occurrence interiors remain open. DP-06 is now partial for its session
register; no DP requirement or gate is complete.

The [atomic MoveRange command](ATOMIC_MOVES.md) relocates current linked contents
within or between ordinary Sequence parents in one core/headless transaction.
Joint endpoint cuts retain complete owner contexts; whole moved nodes, Repeat
plays, marks and accepted providers keep their identities. Root sounds retain
their unchanged root clock and routes. Focused verification passes 101 core,
three picture-plan, nine decoded-audio, four persistence and one CLI test.
Independent implementation review found no further defect after correcting the
design's root-sound ownership rule. All 2,792 locked workspace tests, formatting
and strict all-target workspace Clippy with the UI harness pass on Rust 1.97.1,
with none failed or ignored. [Qualification](qualification/atomic-move-2026-09-30.md)
retains the initial test fixture failures. Native Move operation selection,
removal/insertion join previews, audition and final range selection were still
required at that checkpoint. The native increment below supplies them within
ordinary Sequence scopes. No DP requirement or gate changes status.

The [native Move workflow](qualification/native-move-2026-09-30.md) adds explicit
Copy/Move choice to `:splice`, local saved/proposed comparison at both joins and
one atomic commit selecting the complete moved range. Source endpoints remain
available for rejected destinations. Historical copies cannot remove current
content. Site audition uses bounded absolute-sample windows; reparenting without
timing changes compares identical global coordinates. Both service delivery
orders retain exact destination scope and selection. Qualification separates
decoded PCM/picture checks, Metal replay, native keys and remaining device limits.
Final visual replay passes 752 placement checks plus the shortcut audit; full
release replay passes 3,314. All 2,807 locked workspace tests pass, with none failed
or ignored. A final help-only sentence correction passes 414 app/harness tests,
formatting and strict all-target workspace Clippy. Native SQLite snapshots prove
copy/cancel and historical rejection preserve all 20 tables, one Move adds one
revision/history entry, and Undo restores complete authored state. No DP or gate
is complete.

The [nested fragment deletion check](qualification/nested-delete-2026-09-30.md)
extends `DeleteRange` to partial Source/ordinary Hold endpoints behind nested
unity Partition windows. Exact Split pools retain owner contexts and original
sample clocks; independent root sounds transform once. An isolated old-code
regression fails at admission. Current core, indexed-picture, decoded-PCM and
native service tests cover preserved content, marks, rejection bounds and one
durable deletion with reopen/Undo/Redo. Cut-to-register and empty structural
capture/paste were still open at that checkpoint. All 2,823 locked workspace tests
pass with none failed or ignored, along with formatting and strict all-target
Clippy. No DP requirement or gate changes status.

The [whole-child and cut register increment](qualification/structural-capture-2026-10-01.md)
adds exact selected-child capture, including nested empty Sequence groups.
Native `y` captures that child when no Visual range exists. Visual `d`, `dd`
and captured `:delete` prepare the copy privately, save one deletion and then
publish the historical copy. Failed capture or commit retains the previous
register; saved refresh failures retain the successful cut and reopen guidance.
An empty group pastes at an explicit sibling slot without changing picture or
sample time. Its placement card shows structure and named neighboring groups.
All 2,883 workspace tests pass; final formatting, strict all-target Clippy and
386 default-feature app/headless tests pass. Rendered cut/placement runs pass
150/820 checks, each plus the Kestrel audit. Native keys and SQLite snapshots
verify one saved cut, historical paste, cancellation and Undo; the QA app was
closed afterward. Full release replay passes 3,434 checks with no findings or
failed timing samples. Source identities and explicit coverage limits remain
in the qualification record. DP-06 remains partial.

The [shared render workflow](RENDER_JOBS.md#shared-workflow-and-native-ownership)
connects capture, encoding, retained checkpoints, fresh verification, publication
and reconciliation to the native project service. Close/switch/shutdown retain
the writer until worker release, and explicit subprocess cleanup evidence gates
terminal failure. Render progress and editor feedback have separate retained
state. The automatic policy is now connected to this shared workflow. Public
Render controls, full mastering/effects and HDR remain open, as do DP-17 and DP-18.

[Workflow qualification](qualification/render-workflow-2026-09-30.md) passes
21 real media assertions over two complete encodes, one cancelled encode, six
fresh verifications, checkpoint retries and destination reconciliation. Independent
readers pass 138 pictures, 221,021 authored sample frames and 414 complete planes.
All 2,347 locked workspace tests and strict all-target Clippy pass. Native service
tests cover editing and shutdown ownership; public Render interaction remains open.

The [publication journal](RENDER_PUBLICATION.md#durable-publication-journal)
records exact destination stages and authorizes renames only after checked
SQLite/database/WAL durability barriers. Explicit restart reconciliation requires
fresh checkpoint verification, recorded APFS identity and complete byte hashes.
It preserves committed movie knowledge, partials and foreign files. Database 41
adds publication tables while preserving schema-40 authored and operational cells.
Native Render, public headless rendering, automatic policy, complete audio/effects,
HDR and release qualification remain open. DP-17 remains Open.

[Publication recovery qualification](qualification/publication-recovery-2026-09-30.md)
passes 20 real SIGKILL cases and 40 fresh verifications across Source and Generated
fixtures. Exact authored/history cells are unchanged. Independent final-file
readers pass 138 pictures, 221,021 authored sample frames and 414 complete planes.
Process death is tested between completed host calls; physical power loss remains
unqualified. The full locked workspace passes 2,318 tests, strict all-target Clippy
and formatting on the same source inventory used for native qualification.

[Render checkpoint qualification](qualification/render-jobs-2026-09-29.md)
passes two real restart/reverification cases: 138 pictures, 221,021 authored sample
frames and 414 complete planes checked by independent final-file readers. Workspace
coverage totals 2,284 passing tests after three scoped target corrections, with zero
remaining failures or ignored tests. Strict workspace Clippy and formatting pass.
The original failed full run, corrected target runs and exact source differences
are retained. No native UI behavior changed.

The library [publication host](RENDER_PUBLICATION.md) now accepts a private verified
candidate, captures its historical revision and dependency evidence, and copies it
to an exclusive destination `.partial`. Exact descriptor readback preserves the
verified movie identity. It publishes a bounded local report before the movie's
atomic no-replace rename, preserves retryable candidates on earlier failure and
reports later failures as `PublishedUnconfirmed`. Report and movie publication
are separate commits. Native Render, public headless
render commands, automatic platform policy, complete mastering/effects, HDR and
release qualification remain open. DP-17 stays Open; no requirement or gate is
completed by this library boundary.

[Publication qualification](qualification/render-publication-2026-09-29.md)
passes seven fresh project movies through destination publication and independent
final-file readers: 304 pictures, 390,695 authored sample frames, 912 complete
planes and 18 exact audio marker observations. A separate audit checks all movie
and report hashes, historical document/Original receipts and effective Generated
intervals. All 2,248 locked workspace tests pass, with zero failed or ignored;
strict workspace Clippy and formatting pass. No native UI behavior changed.

The [finished-file verifier](FINISHED_FILE_VERIFICATION.md) now checks private
MP4 candidates in a separate supervised process: actual container/packet clocks
and edit lists, every decoded picture, complete fresh-IDR GOP comparisons, and
manual/ordinary AAC agreement over exact sample coordinates. Failure retains the
candidate for retry. [Current evidence](qualification/finished-file-verification-2026-09-29.md)
passes seven fresh project MP4s through normal and instrumented verifiers:
304 pictures, 390,695 authored sample frames and 21 GOPs per run. Independent
readers pass all 912 planes and 18 exact audio marker observations. The workspace
passes 2,222 tests, Clippy and formatting; 241 selected tests pass with native C
ASan/UBSan instrumentation.
Source header/table/packet limits still constrain capacity below the encoder's;
content, event-sync and runtime qualification remain separate. Library destination
publication is described above. Native Render,
automatic platform policy, full mastering/effects and HDR remain required. DP-17
stays open and no requirement or gate is complete.

The [encoded render worker](ENCODED_RENDER.md) now streams committed pictures
and canonical PCM into private H.264/AAC candidates with complete document
binding and clean-exit hash admission. [Actual project qualification](qualification/encoded-render-2026-09-29.md)
retains seven MP4s, complete decoded picture/PCM comparisons, exact edit-list
clocks, fresh-GOP checks and absolute audio markers through normal and sanitized
readers. Live edit/undo/redo, real cancellation, byte exhaustion and recovery pass.
This advances DP-16/17/18 groundwork. The production structural/decode verifier
is now described above. Native Render, automatic platform
policy, full audio/effects and HDR remain required. Statuses do not change; every
requirement and gate remains open or partial.

The [native SDR encoder and offline audio reader](NATIVE_ENCODING.md) now add
exact chronological I420/PCM admission, shared deadlines, descriptor-only MP4
fast-start and complete codec drain. [Normal and sanitized qualification](qualification/native-encoding-2026-09-29.md)
checks ten usable synthetic encoding cases, six deliberate failures and retained
hardware B-frame rejection per run. Three independent audio read modes preserve
absolute event timing; fresh decoders reproduce every observed GOP suffix.
This advances DP-13/16/17/18 groundwork. Isolated verification and library
publication now exist; native Render, automatic platform
policy, full audio/effects and HDR remain required. No requirement or gate is
complete.

The [render worker](RENDER_WORKER.md) now isolates committed SDR picture
preparation behind strict versioned messages and the shared checked process
supervisor. It binds the complete document and exact output contract, then admits
bounded raw I420 only after clean teardown and independent artifact checks.
This advances DP-16, DP-17 and DP-18 groundwork. Later boundaries add encoding,
structural/decode verification and library publication; complete audio/effects and native Render remain required.
[Qualification](qualification/render-worker-2026-09-29.md)
passes 2,140 workspace tests and 128 actual worker checks, including 82 complete
isolated frames and cancellation/recovery. No status changes.

[Committed encoder pictures](qualification/export-pictures-2026-09-29.md) now
produce exact timestamped SDR I420 from one immutable revision/range, preserving
authored framing through legal even output geometry and retaining one completed
frame. Actual Metal passes 118 checks, including all 30 accepted Generated frames
and complete-plane numerical comparisons. This advances DP-16 and the DP-17
foundation. Later boundaries add encoding, structural/decode verification and
library publication. Complete audio/effects and native Render remain required;
statuses do not change.

[Accepted Generated Hold pictures](qualification/generated-pictures-2026-09-29.md)
now use the same model-independent cold reader in native preview and captured
revision preparation. It verifies six retained objects, strict durable
provenance, both asset records and freshly decoded canonical picture timing.
Real conversion/acceptance tests cover relocation, stale requests, historical
revisions, prefix reuse and damaged dependencies. Native replay checks every
sampled frame through production keyboard navigation and Metal, with visible
Picture/Sound policy at both window sizes. This advances DP-12, DP-16, DP-19
and DP-20 without completing them. App generation/acceptance, full-size media,
model corpus, offline export and release qualification remain required.

Specification 1.1 applies them to [one Original](SINGLE_ORIGINAL.md): the full
video is the initial edit, native projects live in Documents/Deadpan, same-video
moments can be reused, and external media contributes sound only. Accepted AI
extensions remain in scope. Generic backend and legacy multi-video projects are
preserved. [Current design targets](design/README.md) replace the earlier
multi-video workspace; concept screens do not establish completed capabilities.

**Open** means required behavior has no qualifying implementation. **Partial**
identifies concrete groundwork while acceptance remains unmet. **Complete**
requires linked code, passing relevant tests, and a demonstrable acceptance
result. No requirement or delivery gate is complete. [Development](DEVELOPMENT.md)
defines the repository gate and the [UI feedback loop](UI_FEEDBACK.md) defines
application replay, image review and separate responsiveness measurements.
Neither a passing harness nor a screenshot establishes full acceptance. Current
interaction suggestions and personal shortcut conflicts are tracked in
[Interaction review](INTERACTION_REVIEW.md); each implemented fix needs its own
evidence. The initial optional UI harness passed nine visual replay scenarios
and its shortcut audit; its release run missed Repeat/Hold feedback targets.
The [expanded qualification](qualification/workspace-layout-2026-09-27.md)
records fourteen-scenario composite coverage and fresh release p95 values of
4.67 ms for navigation, 6.85 ms for Repeat and 9.11 ms for Hold, all within their
existing targets. The earlier misses remain retained. Native accessibility,
physical display behavior and full-size workloads remain open.

The [footer qualification](qualification/footer-layout-2026-09-27.md) resolves
command-exit layout gaps, clipped Original shortcuts and immediate-Escape text
loss. It retains failing development cases, controlled async test delivery,
final workspace checks, same-frame replay assertions and reviewed Metal captures.
The minimum-size picture still needs more room; no requirement or gate closes.

The [repaint-wait correction](qualification/repaint-wake-2026-09-26.md) replaces
harness polling with bounded egui wakeups and separates worker phases from UI
delivery. That increment's feature lint and 193 unit plus 2 integration tests
passed, while both GPU replays stopped before scenarios because Metal was
unavailable. The later
[layout qualification](qualification/workspace-layout-2026-09-27.md) restores
actual Metal evidence for all fourteen current scenarios through a full run and
affected-path follow-ups. It fixes common Camera visibility, first-frame error
and speed-hint clipping, and pinned sound controls. Default app tests (208) and
feature tests (236), both Clippy configurations and the final focused replays
pass. These checks do not complete the full editing grammar or native
accessibility acceptance.

[Authored framing and Camera](FRAMING.md) implement
per-node static/enveloped framing, retained effect clocks and intermediate clips,
core-18/database-24 history, shared native/offline spatial rendering, and a
temporary keyboard Camera mode. The [new Camera target](design/README.md) retains
the intended saved-target and region workflows, which are not implemented yet.
[Qualification](qualification/framing-2026-09-24.md) records headless, actual Metal,
migration and native aesthetics/keyboard evidence. This work does not complete
DP-08 or any gate.

[Captured framing](CAPTURED_FRAMING.md) adds retained canvas fits and intermediate
clips to Hold recipes, independent of provider changes and the new Hold's Camera
settings. Root pause insertion samples descendant framing and inherits the root's
live framing once. Core 19/database 25 preserve earlier history through a frozen
core-18 adapter. Its [qualification record](qualification/captured-framing-2026-09-26.md)
separates headless checks from blocked Metal and native visual review; this work
does not complete DP-08 or any gate.

[Exact Original moments](SOURCE_MOMENTS.md) derive selected VFR picture intervals
and preserve fractional audio boundaries with a separate audible selection over
the full measured sample span. Core 20/database 26 and audio-context schema 2
retain that intent. Native selection and Sequence-slot paste now build on this layer; persistent
registers, named moments and the complete general splice remain required. [Verification](qualification/source-moments-2026-09-26.md)
records the tests, review and remaining acceptance. No requirement or gate changes status.

[Repeat-gap definition clocks](AUDIO_DEFINITIONS.md) provide current gap recipes
on intrinsic PointCeil or explicit root/point placements, including one-play
Repeats without a rendered gap. The bounded frozen-layout support query resolves
actual stable gap occurrences without crossing an opaque Preserve clock.
Authored gap bindings now consume these operands; general atomic splice
remains open. Verification of the original operand increment is recorded in
[the gap-clock qualification](qualification/gap-clocks-2026-09-26.md).

[Compact audio reanchors](AUDIO_REANCHORS.md) retain per-occurrence visible
allocation entries and chronological sample-phase changes without expanding
Repeats. Core 21/database 27 preserve these steps through copied scopes and
durable history; older histories gain no invented steps. General cursor splice and the full register workflow remain open.
[Qualification](qualification/audio-reanchors-2026-09-26.md) records verification
and limitations. No DP requirement or gate changes status.

[Authored gap bindings](GAP_AUDIO_BINDINGS.md) add core-22/database-28 ownership
for a Repeat's gap recipe, separate from its full timeline. Capture includes
unplayed gaps; stable existing gaps retain their sampling phase while newly born
gaps use their canonical definition clock. Root, point and physical-domain
evaluation share the PCM path, current policy and source admission. Complete
range splice and persistent register support remain required.
[Qualification](qualification/gap-bindings-2026-09-26.md) records passing focused
checks, strict history replay, independent review and the full-run failures with
their diagnostic follow-up. No requirement or gate changes status.

[Editable Repeat gaps](REPEAT_GAP_BRANCHES.md) add core-23/database-29 sparse
owned branches, including zero-gap suppression, dormant final branches,
occurrence-safe copies and exact default-gap materialization. The current Node
recipe retains historical gap clocks without freezing its raw audio policy.
Core, plan and decoded-PCM tests cover these paths; complete project-boundary
splice, persistent registers and full media acceptance remain open.
[Qualification](qualification/gap-branches-2026-09-26.md) records current CLI
history checks, independent reviews, strict migrations and workspace/harness
results, including the remaining sandbox-denied socket check.

[Composite suffix insertion](INSERT_TIME.md) adds core-24/database-30 pause edits
at existing root Sequence seams, with current-clock entry dispatch through
Repeat/gap branches and Preserve outputs. Old-admitted edits keep their prior
reducer, and older history cannot acquire the broader admission. Core, picture,
decoded-PCM, native service and migration checks are recorded in
[qualification](qualification/composite-insertion-2026-09-26.md). The new UI
replay steps are present but their visual run is blocked by the sandbox's missing
Metal adapter. Arbitrary interior splice and Visual replacement remain open;
no requirement or gate changes status.

[Interior pause insertion](INSERT_TIME.md) adds core-25/database-31 atomic cuts
inside root Source/ordinary Hold fragments before composite suffixes. Separate
pre-Split sampling and post-Split placement records retain exact rounded audio
entries, captured framing and one-step undo. Frozen core 24 retains its narrower
contextual admission. [Qualification](qualification/interior-insertion-2026-09-26.md)
records this increment's checks and limitations. Arbitrary nested insertion and
Visual replacement remain required; no requirement or gate changes status.

[Nested Sequence pauses](INSERT_TIME.md) add core-26/database-32 admission for
cuts inside ordinary groups. The Hold belongs to its actual Sequence, live
ancestor framing applies once, and every later sibling resumes from its own
audio entry. The native completion records the exact cursor and selects the
visible enclosing child. Repeat/Retime interiors, rational owner clocks and
Visual replacement remain required. See
[qualification](qualification/nested-sequence-2026-09-26.md) for the checks and
limits. No requirement or gate changes status.

[Sequence group navigation](GROUP_NAVIGATION.md) adds native Enter/Backspace,
breadcrumbs, current-depth inspector edits, scoped Original reuse and Camera.
Service requests retain their exact scope across preparation and completion;
history reconciles paths without retargeting the cursor. The contributed UI
harness now covers nested navigation and editing. Repeat/Retime occurrence
navigation and the full keyboard editing grammar remain open.
See [group navigation qualification](qualification/group-navigation-2026-09-26.md)
for actual checks, review and the unavailable GPU replay.

[Original moment selection and paste](SOURCE_MOMENTS.md) add native v/y and p/P,
an identity-bound session copy, measured temporal range bar, and exact explicit
Sequence-slot insertion. Core 27/database 33 retain shifted audio entries and
admit prepared receipts atomically with history/relevance. Persistent/named
registers, Visual replacement, arbitrary occurrence/cursor splice and native
visual/performance acceptance remain open. See
[paste qualification](qualification/moment-paste-2026-09-27.md) for actual evidence.
No requirement or gate changes status.

[Exact boundary descent](STRUCTURAL_SPLICE_DESIGN.md#exact-boundary-descent)
now exposes the complete revision-bound owner path through Sequences, Retimes,
stable Repeat plays and implicit/owned gaps. Core and headless queries retain
fractional coordinates and shared work bounds without editing the project.
This is a prerequisite for arbitrary nested insertion, not its implementation.
[Qualification](qualification/boundary-location-2026-09-26.md) records the scope
and verification. No requirement or gate changes status.

[Derived-clock framing](FRAMING.md) evaluates exact rational owner extents with
bounded numeric arithmetic while preserving existing integer callers. It is a
prerequisite for the [nested splice representation](STRUCTURAL_SPLICE_DESIGN.md#derived-owner-clocks),
not support for fractional authored frames or a new editing command. Structural
clock maps, retained DSP contexts and arbitrary nested splice remain open.
[Qualification](qualification/framing-clocks-2026-09-26.md) records the independent
reference cases, reviews, regression checks and unchanged schema boundary.

[Live audio input tapes](AUDIO_INPUT_TAPES.md) project exact current-tree windows
onto one PointCeil grid and read their PCM through the shared renderer. They
retain sampling support and scoped identities. Checked `AudioStageProjection`
views now supply nested intrinsic operands, independent output-policy clocks
and request-local PCM memoization; a PointCeil tape can schedule that output
around an inserted pause. [Projected root placement](AUDIO_PROJECTED_ROOT.md)
separately supplies absolute RoundEven allocation and repeated resume that
preserves phase for one physical projection. Persisted splice routes, effective owner
clocks, aggregate output scheduling and arbitrary nested editing remain open.
These evaluation APIs do not complete a requirement or gate.
[Qualification](qualification/audio-input-tapes-2026-09-26.md) records its exact
mapping tests, real PCM comparisons, independent review and repository checks.
[Intrinsic projection qualification](qualification/preserve-projections-2026-09-26.md)
records nested-history PCM, output-policy clocks, admission and memory limits,
review corrections and the unchanged authored-schema boundary.
[Root projection qualification](qualification/projected-root-2026-09-26.md)
records independent sample-phase and policy tests, admission review and the
required repository checks.

[Scoped sound mixing](SOUND_EVENTS.md) adds borrowed ordered voices, exact gates
and per-voice silence on a common intrinsic grid. Explicit aggregate Preserve
input uses one canonical preparation and retains its independent output-policy clock. This is
raw preparation before creative voice effects and mastering. Those borrowed
operands alone provide no persisted recipes, edit transforms, native placement,
voice treatments, final bus integration or preview/export evidence. Catalog import
alone does not place a sound, and this prerequisite does not promote a DP or gate.
[Qualification](qualification/scoped-mix-2026-09-27.md) records decoded PCM,
scope/policy checks, review corrections and remaining acceptance work.

The sound increment adds separate catalog audition through canonical playback,
a bounded exact recipe-route kernel and a CLI LRU source cache. Sound playback
uses measured sample endpoints and preserves the stopped picture, edit clocks
and selection. The route kernel is not yet installed in authored sound events;
no placement, migration, DSP lattice or final mix is implied. The
[audition board](design/boards/sound-audition-board-v1.png) and exact prompt are
retained targets. [Qualification](qualification/sound-audition-2026-09-27.md)
records reviews, actual PCM, harness checks and acceptance limits. All DP and gate
statuses remain unchanged.

The [sound-clock preparation increment](qualification/sound-clocks-2026-09-27.md)
adds physical sample-grid history to exact routes, separate current Hold issuer
queries, and bounded LRU eviction to playback's source cache. Window/Keep
projections preserve both the complete recipe and the old selection's audible
mask, so displaced rounding cannot expose audio past a cut. These remain
preparation APIs; that increment did not add authored sound persistence,
placement, voice processing or final mixing. It left core 28/database 34 unchanged. No DP or
gate status is promoted.

The [catalog source-voice increment](qualification/source-voices-2026-09-27.md)
adds a checked independent audio operand with exact natural-rate source placement
and separate input/output Hold policy. It reuses the tape, source admission and
canonical Preserve readers without adding authored Source nodes. This does not
install sampled routes or persisted events, grant allowances, implement voice
effects or populate the final bus. All DP and gate statuses remain unchanged.

[Retained sample routes](qualification/routed-voices-2026-09-27.md) now have checked
PCM preparation handles for independent source input, intrinsic Preserve output
and captured root output. Reads preserve
the old physical samples and complete filter/DSP history instead of reconstructing
phase from the edited frame clock. Captured output policy and old selected masks
stay separate from current consuming Hold gates. These remain preparation APIs;
that increment did not add persisted sound commands, allowances, voice effects
or final bus integration. It left core 28/database 34 and all DP/gate statuses unchanged.

The [playback test scheduling record](qualification/playback-waits-2026-09-27.md)
tracks recurring cold-preparation timeouts separately from product failures.
The test-only PCM reservation includes detached worker teardown; production
timeouts, PCM assertions and all requirement/gate statuses remain unchanged.

The [root sound-event increment](SOUND_EVENTS.md#persisted-root-sounds) adds
qualified, durable `SetSound`/`DeleteSound` recipes in core 29/database 35.
Events keep picture duration unchanged and enter the real pre-master bus with
independent sample phase, edges and gain before one shared limiter. Root
RoundEven evaluation, reopened history, scalar decoded-PCM sums, cached source
revocation and source-only audition are tested headlessly. Temporal edits are
guarded except for the root ripple subset below. Nested ownership, Hold allowances,
voice effects, listening and preview/export acceptance remain required. Native
placement is covered by the later increment below; no DP or gate is promoted.

The [root-sound qualification](qualification/root-sounds-2026-09-27.md) retains
decoded-PCM witnesses, review corrections, migration provenance and the original
test-gate failure. The [sound-placement board](design/boards/sound-placement-board-v2.png)
and exact prompts define the intended native interface; the later native
qualification below records implemented placement controls.

Core 30/database 36 add [persisted root ripple edits](SOUND_EVENTS.md#persisted-root-ripple-edits).
InsertTime, SpliceSource and ordinary Sequence Delete retain complete sound
recipes and chronological physical sample clocks; non-root Split keeps the
root bus unchanged. Parameter changes retain routing, and explicit ReplaceSound
clears it. Actual database-35 history migrates without invented routes, while
forged sound-bearing legacy Split histories remain rejected. Root Split,
temporal occurrence edits, nested ownership, remaining structural sound edits,
allowances and effects remain open. All DP and gate statuses
remain unchanged. The [routing qualification](qualification/root-sound-routing-2026-09-27.md)
records review corrections and verification scope.

The [native sound-placement increment](qualification/native-sound-placement-2026-09-27.md)
connects that guarded root subset to catalog placement, a distinct placed-event
selection and inspector, exact frame nudges, fine sample entry, gain, edges,
removal and durable history. Picture duration and the retained beat stay intact.
Production replay covers focus entry and command targets across delayed writer
completion. The [ImageGen board](design/boards/sound-placement-board-v2.png)
guides the appearance; it does not implement nested ownership, scoped Hold
allowances, effects, listening or export acceptance. Every DP and gate remains
open or partial.

Core 31/database 37 add [persisted root sound allowances](SOUND_EVENTS.md#persisted-root-sound-allowances).
One sound can pass through one concrete silent Hold or Repeat gap while other
contributions and occurrences remain suppressed. Structural edits retain or
remap the relation, old histories gain no permission, and source admission runs
for allowance-only transactions. Native inspector controls and
`:sound-allow` / `:sound-silence` capture the sound, Edit frame and exact issuer.
Retained route gaps remain empty. Nested ownership, send/tail rules, the full
voice graph, listening and preview/export acceptance remain required; no DP or
gate is promoted by this subset.
The [allowance qualification](qualification/sound-allowances-2026-09-27.md)
records review corrections, migration provenance, actual PCM and UI evidence.

Core 32/database 38 add [atomic Hold audio policy changes](ROOM_TONE_AUDIO.md).
The direct and occurrence setters retain timing and picture, validate new
source choices against measured admission, and retire obsolete silence
permissions with exact undo. Frozen core-31 replay preserves prior allowances
without admitting the new command. The [authoring record](qualification/hold-audio-2026-09-27.md)
tracks migration, source admission and actual PCM verification. The
[room-tone board](design/boards/room-tone-board-v2.png) supplies the target for
native range selection, distinct source/pause audition and explicit application.
The [native workflow](ROOM_TONE_AUDIO.md#native-selection-and-audition) now
implements that ordinary-Hold subset: measured inward source-sample boundaries,
editable fields, identity-bound preparation, separate source audition, Apply,
silence and undo. Waveforms, Repeat-gap/fragment controls, physical input and
listening acceptance, full mixing and export remain required. No DP or gate is
promoted. See [native qualification](qualification/native-room-tone-2026-09-27.md).

[Gain recipes and owner clocks](AUDIO_GAIN.md) now provide bounded standalone
gain/mute evaluation and checked structural clock inspection. Core 33/database
39 now attach those recipes to nodes with direct/occurrence reversible commands,
frozen core-32 history replay and context-schema-4 treatment evidence. The
canonical authored bus applies gain after time/pitch and edges, before the
limiter; `inspect-audio --authored-bus` exposes bounded pre-limiter PCM. The
[gain design board](design/boards/clip-gain-board-v2.png) defines the native target.
The native increment adds captured beat trim/mute commands and an unsaved editor
for exact owner-output envelopes and mute ranges. Before/Draft comparison uses
separate proposed-content identities and one retained heard-sample window; only
Apply writes history. Verification is recorded separately in the
[native-gain qualification](qualification/native-gain-2026-09-28.md).
Exact-field and comparison replay, both app configurations and a native macOS
command/focus/text/cancellation pass are verified within that recorded scope.
The complete release replay passes 2,156 checks, retaining separate cold and
warm timing populations and the small-fixture/offscreen qualification boundary.
Waveform editing, physical keyboard/IME and acoustic qualification, full voice
processing and the complete DP-09 workflow remain required.
The [measured beat overview](WAVEFORMS.md) has scoped
[qualification](qualification/gain-waveform-2026-09-28.md), with 32 focused
waveform tests and all 2,062 normal workspace tests passing. After an app-only
visual alignment fix, final normal/optional app tests pass 267/302 cases,
and painted Gain/room-tone replays pass 291/221 checks plus their routing audits.
The full release replay passes 2,348 checks across 18 scenarios. This is a bounded
pre-effects reference for one
captured committed owner, not a final-mix or timeline-wide waveform editor.
The [compact workspace increment](qualification/compact-workspace-2026-09-28.md)
also consolidates empty Placed sounds into a visible Beats-heading target at
short window heights. Copied-range controls retain a measured 143-point minimum
picture in the qualified Hold state; nested navigation, focus, scale changes
and first placement/undo have actual painted replay evidence. Full UI and
performance acceptance remain open.
See the [authored-gain qualification](qualification/authored-gain-2026-09-28.md)
for review corrections and verification limits, and the
[earlier gain/clock record](qualification/gain-clocks-2026-09-27.md) for the pure
recipe/query checkpoint.

[Original and edit audition](PLAYBACK.md) connects immutable qualified originals and
canonical limited PCM to the native device, with Space Play/Pause, a monitor
level, exact audio-clock picture scheduling and explicit interruption. Bounded
preparation runs separately from device control. Original Space playback uses
the complete measured A/V union; Shift+Space loops the selected Original moment
or edited beat with adjustable context. Delivery remains monotonic across laps,
and read-only audition leaves history unchanged. Full mastering,
acoustic synchronization, long-source/performance qualification and
preview/export equivalence remain required. This increment does not complete a
requirement or gate; [qualification](qualification/playback-2026-09-24.md) records
its native, headless and review evidence. Older evidence below describes its own
historical boundary.
The [Original/selection increment](qualification/original-audition-2026-09-27.md)
records the separate full-source and selected-picture endpoint policies, real
PCM and native routing regressions, and contributed harness coverage. All product
requirements and gates retain their existing open/partial status.

[Master gain research](qualification/audio-limiter-gain-search-2026-09-24.md)
retains independently audited finite-fixture solutions, the corrected unwanted
muting failure, and a longer stress failure in a faster finite-context design.
Neither of those earlier candidates is adopted. The [shared limited reader](AUDIO_MASTERING.md)
now implements a pinned finite oversampled gain path over the current edge-faded
bus, consumed by playback and `inspect-audio --limited`. It uses canonical tiles,
complete source provenance, one cumulative preparation budget and final-f32
verification, with [qualification evidence](qualification/audio-limited-2026-09-24.md).
Full voice/group processing, gain-motion/listening acceptance and
encoded output remain open. No requirement or gate changes status.

[Pause insertion](INSERT_TIME.md) adds atomic root Source/Hold splices with
sample-preserving resume for every shifted fragment, including earlier splits
and repeated pauses. Native `,h`, counts and exact `:hold` duration entry resolve
a measured freeze frame, retain the Original baseline and select the committed
pause. Core 17/database 23 freeze prior command history. Arbitrary nested scope,
compact Repeat-gap ownership, active generation context resolution, playback and
export remain required. [Qualification](qualification/insert-time-2026-09-24.md)
records exact timing, native interaction and actual baseline migration evidence.
No requirement or gate changes status.

[Logical mark bindings](MARK_FRAGMENTS.md) preserve one named mark across physical
bindings, with atomic loss/copy transforms, biased Partition seam visibility and
exact named resolution. Core 13/database 19 introduced frozen mark grammars.
[Qualification](qualification/mark-fragments-2026-09-23.md)
records review, migration and test evidence for this layer.

[Structural Split](STRUCTURAL_SPLIT.md) now cuts explicit beats and isolated
occurrences without changing duration or output, retains full processing contexts,
and refines repeated cuts without increasing wrapper depth. Native `s`/`:split`
captures an interior boundary in the selected child and selects the committed right fragment.
Core 14/database 20 freeze prior history, including multi-binding marks.
[Qualification](qualification/structural-split-2026-09-23.md) records parity,
history and native review. Arbitrary Hold insertion, shifted-fragment sample
resume, minimal nested range planning and the full editing workflow remain open.

[Audio sampling clocks](AUDIO_SAMPLING.md) now separate allocation grids, exact
sample maps and retained envelope progress in all source/stage readers. Contract
tests exercise fractional-rate resume composition and envelope exhaustion without
changing filter support or rate. These are derived plan values, not persisted
Hold edits. Authored reference domains, genuine seam fades and arbitrary-boundary
insertion remain open.
[Qualification](qualification/audio-sampling-2026-09-23.md) records 985 passing
tests, the full gate and the reviewed wide-envelope compatibility fix.

[Frozen audio reference layouts](AUDIO_REFERENCE.md) retain old timing, compact
play identity, exact Source placement and Hold/gap policy without live sibling
dependencies or media ownership. Explicit root/Preserve clocks query old
audibility, and a bounded root-resume consumer combines it with current silence.
Actual DSP regressions cover both one-sample NTSC suppression mismatches.
This is a capture/query/PCM foundation, not an authored insertion or persistence
change. [Qualification](qualification/audio-reference-2026-09-23.md) records
1,011 passing tests, the full gate and independent review; schema versions
remain unchanged.

[Sampled-root transfer](AUDIO_SIGNAL_TRANSFER.md) converts admitted raw root PCM
onto an explicit point grid, retaining old silent samples before interpolation
and exact output suppression afterwards. StageAudio keeps full preparation
contexts and shares work, provenance checks and one deadline across all halo
reads. Creative fades remain after time mapping. Tests include real decoder and
canonical DSP paths. [Qualification](qualification/audio-transfer-2026-09-23.md)
records 1,021 passing tests, the full gate and independent review. Authored live-to-frozen bindings, policy replacement,
compact Repeat lifecycle, atomic Hold insertion and application playback remain
open. This conversion introduced no authored-state change and did not complete a gate.

[Physical reference-domain lookup](AUDIO_REFERENCE.md#physical-processing-domains-and-root-maps)
now retains opaque Preserve and meaningful context independently of visible
Partition allocation. Borrowed root maps compose the active cut and independently
anchor later domains. Tests cover NTSC phase, compact repeats, clock ownership,
real canonical DSP and fractional transfer. This does not author an insertion:
persisted sample bindings and their live edit lifecycle remain required. [Qualification](qualification/audio-domains-2026-09-23.md)
records 1,026 passing tests, the full gate and independent review; no requirement
or gate changes status.

[Authored audio copy lineage](AUDIO_LINEAGE.md) now survives Split, occurrence
isolation and durable history. Raw audio changes detach affected contexts and
ancestors while unrelated copies retain their relationships. Frozen reference
plans compare explicit lineage separately from physical identity and require
compatible clocks, phase and stable occurrence paths. Core 15/database 21 migrate
actual old copy history without inferring lineage from an initial snapshot.
[Qualification](qualification/audio-lineage-2026-09-23.md) records lifecycle,
strict migration and bounded reference tests. Live sample bindings, policy
replacement, atomic Hold insertion and application playback remain open.

[Retained audio contexts](AUDIO_CONTEXT.md) now capture complete raw audio trees,
exact media inputs and immutable asset contracts. Direct audio-only compilation
retains Source absence, RoomTone, nested Preserve, edges and compact Repeat
structure. The headless host authenticates each context against its exact
historical revision before verified media reads, including after undo and alias
reuse. This adds the old signal body; authored live bindings, phase composition,
policy replacement and atomic Hold insertion remain open. Core 15/database 21
are unchanged. [Qualification](qualification/audio-context-2026-09-23.md) records
the decoder/DSP parity, strict ingress and host-admission evidence.

[Physical audio domains](AUDIO_PHYSICAL_DOMAINS.md) now render retained Source,
RoomTone and Preserve context outside visible Partitions on the original signed
root grid. Both processing and policy queries stay inside the captured physical
subtree. Point-grid transfer preserves fractional phase and old silence through
one shared preparation allowance. Headless inspection reads actual qualified
historical media. [Qualification](qualification/audio-physical-domain-2026-09-23.md)
records hidden-sibling, negative-coordinate, decoder/DSP and admission tests.
Authored bindings, retained evaluation ownership, lifecycle/policy replacement
and atomic Hold insertion remain open; no requirement or gate changes status.

[Authored audio definitions](AUDIO_DEFINITIONS.md) now expose the actual Repeat
default independently of audible occurrences, with scoped point-grid queries,
real source/DSP rendering and historical headless admission. This resolves the
all-overridden-default counterexample for a future new-play operand.
[Qualification](qualification/audio-definition-2026-09-23.md) records exact
grid, silence, cache and media tests. Authored birth rules, binding lifecycle
and Hold insertion remain open; no requirement or gate changes status.

[Owned recipe clocks](OWNED_AUDIO_CLOCKS.md) evaluate current physical definitions
in explicit signed root placements through the shared reader. Source alignment
and silence-policy edits affect that revision's raw recipe; explicit historical
reads retain their old meaning. This supplies the evaluation boundary for the
proposed owned-tree binding model, without a second frozen raw-body graph.
Persisted anchors, compact Repeat birth/survivor rules, command lifecycle and
atomic Hold insertion remain open. No requirement or gate changes status.

[Owned-clock qualification](qualification/owned-audio-clock-2026-09-23.md)
records 11 new tests, the 1,108-test full gate and two independent reviews for
this evaluation boundary.

[Owned audio bindings](OWNED_AUDIO_BINDINGS.md) add the core 16/database 22
representation for timing lattices, stable Repeat scope, explicit births and
bounded symbolic phase. Split/isolation copy live arguments while retaining old
aliases; removal and inverse patches retain atomic ownership. PointCeil owned
evaluation preserves selected origins and nested preparation budgets. InsertTime
now captures bindings for supported root splices. Normal StageAudio rendering
now consumes root/point bindings, current and retained-placement policies, exact
resume phase and virtual post-mapping fades. A pure capture helper retains
compact birth scope and existing bindings, including configured Repeat gaps
through the separate [gap ownership map](GAP_AUDIO_BINDINGS.md).
Context-schema-1 capture and source-only SequenceAudio still reject nonempty
bindings. Full command lifecycle and arbitrary Hold insertion remain open.
No requirement or gate changes status.

[Binding qualification](qualification/owned-audio-bindings-2026-09-23.md)
records 1,140 passing tests, the complete repository gate and three independent
reviews for this representation and evaluation boundary.

[Consumer qualification](qualification/owned-audio-consumer-2026-09-23.md)
records 1,172 passing tests and the full gate for retained PCM,
post-mapping fades, compact capture, independent grid policies and shared
preparation admission. Review fixed tiny-clip fade changes, crop leakage,
dense policy inventory rejection and unintended muting of Preserve decay.
That qualification covers the engine milestone. The subsequent
[pause insertion](INSERT_TIME.md) command connects it to native authoring.

Current measured evidence: [editing foundation verification](FOUNDATION_VERIFICATION.md), [picture plan and migration verification](PLAN_MIGRATION_VERIFICATION.md), [exact boundary verification](ANCHOR_VERIFICATION.md), [persistent mark verification](MARK_VERIFICATION.md), [sparse override verification](OVERRIDE_VERIFICATION.md), [nested occurrence verification](OCCURRENCE_VERIFICATION.md), [native media qualification](qualification/media-2026-09-20.md), [compatible FFmpeg/Rust qualification](qualification/media-compatible-2026-09-20.md), [audio candidate qualification](qualification/audio-2026-09-20.md), and [canonical audio qualification](qualification/audio-canonical-2026-09-20.md).

The [native host conversion qualification](qualification/media-host-conversion-2026-09-21.md)
adds actual sampled/native model-output conversion, exact RGB comparisons,
bounded failure tests, and adapter sanitizer evidence. Complete media/provenance bundles are now implemented through
[host qualification and Ready storage](GENERATION_BUNDLES.md).
Schema 9 adds [explicit durable acceptance](GENERATION_ACCEPTANCE.md), measured
source spans and six-object admission. Application acceptance remains open.
The [bridge sampling qualification](qualification/media-bridge-2026-09-21.md)
adds paired native/sampled masters from one immutable input, exact integer
interpolation, and comparison with the earlier captured-model pixels. This
validates media derivation; it does not qualify candidate acceptance or seams.
The [native bundle qualification](qualification/model-bundle-2026-09-21.md) adds
a real protocol-2 local generation, required provenance binding, exact host-derived
masters, three-object publication/readback, and schema-8 Ready persistence.
That earlier run did not exercise authored acceptance or application integration.
Host qualification also retains the exact context manifest and both prepared
input byte streams before generation, binds them to worker provenance, and
returns all six objects for durable publication. [Retained conditioning evidence](qualification/conditioning-2026-09-21.md)
records a fresh local model run and relocated six-object readback. This is byte provenance, not
source-clock, image-decoding, color or seam qualification. Admission-bearing Ready
and acceptance now verify all six objects. A captured real generation has been
accepted, relocated, undone/redone/reverted and independently decoded in a
[developer probe](qualification/acceptance-2026-09-21.md). History reference
inventory and application rendering remain open.

[Source preview qualification](qualification/source-preview-2026-09-21.md) adds
persistent source decoding, identity-verified measured indexes, a shared SDR GPU
baseline and a native preview with frame navigation. Actual Metal comparisons,
source/session sanitizer tests and native visual/keyboard observations are
recorded separately. Project import, editorial playback and export remain open.

[Video admission qualification](qualification/video-admission-2026-09-21.md)
adds closed MP4/Matroska checks before demux allocation, FFV1 configuration
expansion limits, controlled first-frame probing and per-packet bounds. It
preserves the measured source fixtures and an accepted generated master. The
full required format matrix and authored import remain open.

[Original ownership qualification](qualification/original-media-2026-09-21.md)
adds managed complete-file retention, APFS clone/copy, linked locations,
identity-checked relinking, private snapshots and schema-10 migration. A bounded
native audio inventory reports observed metadata; that earlier ownership change
did not establish decoded audio bounds or complete authored import.

[Source audio](SOURCE_AUDIO.md) now provides a separate persistent AAC/PCM
decoder, identity-bound measured sample indexes and bounded private PCM range
reads. Video/audio share one verified original snapshot. Explicit skip/discard
and measured terminal-duration evidence remain distinct from unknown priming and
container claims. [Qualification](qualification/source-audio-2026-09-21.md)
records exact PCM and AAC fixtures. The [independent audio mapping](SOURCE_AUDIO_MAPPING.md) represents exact natural-rate
durations and offsets, with reversible commands and schema-11 history migration.
[Mapping qualification](qualification/audio-mapping-2026-09-21.md) records exact
boundaries, headless history behavior and old-binary fixture reproduction.
[Exact source picture mapping](SOURCE_VIDEO_MAPPING.md) now separates original
video speed from rounded beat duration, retaining selected-span endpoint policy
in picture plans and exact inverse anchors. Reversible mapping commands and
schema-12 migration preserve prior audio and picture decisions.
[Picture-mapping qualification](qualification/video-mapping-2026-09-21.md) records
VFR boundary cases, actual decoded pixels and old-binary migration evidence.
Authored import, resampling, playback, device output and full format qualification remain open.

[Measured import timing](SOURCE_IMPORT_TIMING.md) adds exact independent stream
starts, common-origin candidates, complete available-audio preservation, outward
source enclosure and measured cadence/geometry candidates. Core schema 8 and
database schema 13 retain earlier mappings through frozen history replay.
[Import-timing qualification](qualification/import-timing-2026-09-21.md) records
actual media and migration evidence. These candidates do not establish import
readiness or adopt a project's presentation basis.

[Source registration](SOURCE_REGISTRATION.md) adds live selected-stream
qualification, durable measured indexes and revision-bound receipts, plus atomic
asset registration and optional full-source insertion through the headless host.
Core schema 9/database schema 14 preserve older histories without inventing source
evidence. [Qualification](qualification/source-registration-2026-09-21.md) records
actual-media, rollback, relocation, historical alias reuse and migration checks.
The native workspace connects registration and explicit whole-source insertion;
complete import/relink/format acceptance remains open.

[Automatic presentation basis](PRESENTATION_BASIS.md) selects measured cadence
and geometry on the first primary picture insertion in an untimed project.
Audio or other timed editing locks the existing clock; later primary geometry
adoption is a separate previewable, undoable transaction that preserves timing.
Core schema 15/database schema 21 retain this policy and migrate older histories
as explicit choices. [Qualification](qualification/presentation-basis-2026-09-21.md)
records real-media insertion, unchanged sample/mark coordinates, rollback,
headless previews and authentic schema-14 history migration. Native canvas
previews, framing-effect reevaluation and rendered-output checks remain open.

[Background import preparation](IMPORT_PREPARATION.md) separates complete-file
verification and source receipt preparation from the project writer. Opaque
results are tied to a live writer session; final namespace/version checks reject
changed originals. Current-revision insertion and atomic persistence retain the
existing command path. The native workspace now connects bounded import preparation; measured large-media
latency remains open. The [qualification report](qualification/import-preparation-2026-09-21.md)
records the headless concurrency, freshness and rollback evidence.

[Native project workspace](NATIVE_WORKSPACE.md) connects creation/reopen,
managed/linked import, explicit whole-source insertion, saved history and exact
Source/Sequence frame inspection. Root-beat Repeat wrapping/setters, deletion and
existing Hold-duration edits now share the service/core/store command path.
The [root editing qualification](qualification/native-editing-2026-09-23.md)
records durable edit/history, selection, concurrent import and native keyboard
checks, including pointer/text-focus regressions.
The [initial qualification report](qualification/native-workspace-2026-09-21.md)
records actual media, deterministic service interleavings, headless keyboard/focus
checks and native panel/picture observations. Full editing, playback, generated
provider preview, export and release acceptance remain open.

[Single-original initialization](SINGLE_ORIGINAL.md) adds the optional schema-17
profile, full measured baseline and protected undo floor, with strict historical
validation. The native host creates these projects in the system Documents
library and separates Original identity from audio-only catalog registration.
[Profile tests](../crates/deadpan-store/tests/single_source.rs) exercise baseline,
history, rollback, immutable Original and forged-profile rejection. Native
[service tests](../crates/deadpan-app/src/project/tests.rs) cover creation, sound
selection, recovery and backed-up opening of authentic schema-16 projects.
The [single-Original review](qualification/single-original-2026-09-23.md) records
the actual native workflow, imagegen comparison and verification limits.
Generic projects migrate without invented profiles. Full SFX placement/mixing,
range reuse and the remaining editorial session are still required (YouTube
acquisition is now [headless and native](YOUTUBE_IMPORT.md)); audio-only sequential insertion is not a sound overlay.

[Transparent partitions](AUDIO_PARTITIONS.md) add a retained audio-context
building block for splices. Core 12/database 18 separate allocation, exact source
filter support and full envelope width; ordinary authored trims retain their
old meaning. Real PCM tests compare retained partitions against the original,
including fractional rates, short fades, nested pitch stages and RoomTone.
The [qualification report](qualification/audio-partitions-2026-09-23.md) records
tests and review for that layer. The later Split implementation uses these
retained contexts. InsertTime now applies exact resume anchors to supported root
Source/Hold splices; arbitrary nested insertion remains open.

[Preview presentation](qualification/preview-presentation-2026-09-21.md) separates
requested, decoded and displayed identities, retains pending GPU work, rejects
late replies and labels the submitted Source/Sequence position. Headless
interleavings, real decoded freeze frames and native keyboard/appearance checks
cover this boundary. Continuous playback scheduling, full accessibility, physical
display timing and forced GPU-loss qualification remain open.

[Structural audio plans](AUDIO_PLAN.md) now expose bounded sample intervals with
original coordinates and distinct authored policies. The [native DSP boundary](AUDIO_DSP.md)
binds the canonical stretch schedule to owned Rust PCM. [Qualification](qualification/audio-foundation-2026-09-21.md)
records independent interval references, 50 prior PCM hash matches, native ABI
sanitizers and the 648-test repository gate. [Source preparation](AUDIO_PREPARATION.md)
now adds exact-phase resampling, explicit stereo speaker mixing and full-index
verification of original PCM sessions. Its [qualification](qualification/audio-preparation-2026-09-21.md)
records real PCM/AAC tests, signal checks and measured worker cost.
[Plan-driven source PCM](SOURCE_STAGE_AUDIO.md) now connects immutable spans to
historical source receipts and original bytes, including silent Holds, repeats,
fractional placements and FollowSpeed retimes. Its
[qualification](qualification/sequence-audio-2026-09-21.md) records real CLI/app
headless parity and cold-reader identity after undo and asset-alias reuse. The
[exact-rate DSP extension](qualification/audio-exact-rate-2026-09-21.md) now keeps
consumption speed independent of rounded storage/output counts.
[Continuous stage preparation](AUDIO_STAGE_PREPARATION.md) connects that adapter
through seams, nested retimes, exact fractional grids and immutable source
provenance. Its [qualification](qualification/audio-stages-2026-09-21.md) covers
PCM parity, Hold policies, cache changes and shared work admission.
[Room-tone preparation](ROOM_TONE_AUDIO.md) now loops explicit retained source
ranges through exact short crossfades, including retimes and repeat gaps. Full
voice processing, native audio playback, effects and export remain open.
[Prepared output](AUDIO_OUTPUT.md) adds a bounded generation-aware queue and a
narrow macOS device boundary, with explicit prepare/activate, starvation and
fault states. Its [hardware qualification](qualification/audio-output-2026-09-21.md)
is separate from application playback, full device/lifecycle coverage and
acoustic or stress acceptance.

[Informational audio measurement](AUDIO_METERING.md) adds shared integrated
loudness and true-peak meters, generated standard cases and an independent
[reference qualification](qualification/audio-metering-2026-09-21.md). Meters
preserve PCM; the failed limiter prototypes remain historical evidence. The
current [limited reader](AUDIO_MASTERING.md) separately applies gain and reports
reduction through the headless API. Reduction UI and final mix integration remain
open.

[Worker cleanup verification](qualification/worker-cleanup-2026-09-21.md) records
a failed macOS CI run and the shared membership-confirmed teardown for media
and model workers. Deterministic signal/ownership/pipe regressions and actual
forking descendants cover these fixes; process-group cleanup is not a sandbox.
[Conditioned limiter research](qualification/audio-limiter-masks-2026-09-21.md)
retains a 42-fixture pass and two subsequent suppression-mask counterexamples.
The prototype remains unadopted; this does not advance mastering acceptance.
[Post-mask gain research](qualification/audio-limiter-postmask-2026-09-22.md)
retains 16 further outputs and a complete-sinc audit. Two distinct rapid-mask
outputs still fail the broader reconstruction check despite passing both finite
meters. Fixed fades that excessively quiet tiny fragments are rejected as a
default; production limiting remains open. The shared edge stage described
below uses shortened fades with an explicit tiny-fragment contract.

[Audio boundary provenance](AUDIO_PLAN.md) now retains exact structural,
placement and Repeat-gap edge owners, including coincident constraints and each
owner's own occurrence path. Root query crops preserve this metadata, and copy
work is bounded. [Authored edge policies and shared fades](AUDIO_EDGES.md) add
reversible node/occurrence commands, strict core-11/database-16 migration, and
sample-centered 2 ms fades shortened within each root allocation after continuous
time mapping. The explicit headless stage preserves silence and read partition
invariance. Native controls, listening qualification, simultaneous voices,
downstream effects/mastering, playback and export remain open.

## Product requirements

[Structural speed editing](RETIME_EDITING.md) adds exact native Retime creation
and parameter adjustment with explicit preserve/tape pitch, retained input range,
atomic history and a closed core-28/database-34 migration boundary. Full range
operators, variable speed, independent pitch, rendered GUI acceptance and encoded
preview/export equivalence remain required; this does not complete DP-07 or DP-09.

| ID | Requirement | Status | Implementation / tests now | Required acceptance evidence still outstanding |
| --- | --- | --- | --- | --- |
| DP-01 | Documents library, one-Original initialization/baseline, reopen, autosave, undo/redo, migration, recovery. | Complete | [`deadpan-store`](../crates/deadpan-store/): durable packages/history, atomic mark transforms and generation relevance, writer ownership, WAL checkpoints and interrupted-attempt recovery. [Current development formats](DEVELOPMENT_FORMATS.md) reject obsolete development formats before writes, and the native app now explains the refusal. [Recovery](RECOVERY.md): writer session markers detect an unclean exit ([process-kill test](../crates/deadpan-store/tests/recovery.rs)), `open_recovery` reports interrupted render/publication/generation attempts, the app offers to reopen the project a crashed launch left open and shows what was recovered, a missing Original opens degraded and is relinked/restored only from identical content, and real ENOSPC/EROFS/EACCES failures on APFS disk images leave the project valid with truthful "Not saved" messaging ([store](../crates/deadpan-store/tests/storage_failures.rs), [app](../crates/deadpan-app/src/project/tests/recovery.rs), destination publication). Closing with open previews asks first. Replays `recovery`, `relink` and `storage-failure` pass 37 checks (2026-10-05). [Backups](BACKUPS.md): verified rotating backups (every 15 minutes while editing, on close, on request, before restores and migrations) through the SQLite backup API off the writer, restore as a reversible new session from `:backups` and `project restore` (plus `--damaged` for a database that no longer opens), read-only viewing of packages a newer build saved (header **Read-only**, edits refused, bytes unchanged), the [release migration policy](BACKUPS.md#release-migration-policy) with an implemented backup/copy/validate/promote runner, a synthetic N to N+1 step and the production 66 to 67 step (verified on a package the schema-66 build created, including its render history), bookmark-based automatic relinking of moved linked originals and sounds after content verification ([moved files](RECOVERY.md#moved-linked-files)), measured history size, real disk-image ENOSPC tests for generated media, render candidates, proxy builds, model downloads and offline folder/archive imports (including unchanged verified previous versions and successful retry after freeing space), and [process kills](BACKUPS.md#process-kills) during commits, AI attempt states, backups, checkpoints, restores, migrations and proxy publication (which found and fixed an unrecoverable hot rollback journal). Store `backups`, `bookmarks`, `chaos_kills`, `storage_failures_media` and `migration::tests`; app `project::tests::backups` (5); CLI `backups`, `relink_moved`, `proxy_disk_full`; replays `backups` 22, `storage` 15, `recovery` 14, `relink` 10 and `storage-failure` 13 checks (2026-10-06). | Native create/open/history have [workspace evidence](qualification/native-workspace-2026-09-21.md). History is retained intact per specification 20.2, with its size reported in storage ([history](BACKUPS.md#history-limits)); there is no undo-depth limit. The [backup policy](BACKUPS.md) is now configurable and verified through persistence tests, a 31-check replay and native Save/restart. [Read-only command completion](qualification/readonly-controls-2026-10-08.md) returns captured failures to pending native controls while preserving registers and drafts; 25 focused tests, strict Clippy and a 50-check production replay pass, with package bytes unchanged. [Real Render host kills](qualification/render-host-crash-2026-10-08.md) during encoding and verification pass: workers exit, attempts recover as Interrupted, authored/history cells remain identical, and a retained checkpoint retries with one fresh verifier and no new encoder. [Offline cross-volume relinking](qualification/relink-volumes-2026-10-08.md) passes with two private APFS volumes, actual detachment, wrong-byte refusal, unchanged history and identical restored decoded pictures. [Named takes](TAKES.md) now save, update, rename and delete revision-bound versions, and restore a complete authored snapshot as one undoable edit through the native `:takes` panel and authenticated CLI. Catalog changes preserve the current edit and Redo; restores retain accepted providers without restarting old generation work. [Qualification](qualification/named-takes-2026-10-08.md) records store/service/CLI tests, independent review, UI replays and native-input limitations. Native failed-Open recovery now verifies a selected backup and restores it through a separate keyboard action, retaining old database files and refusing changed captures; an unreadable manifest requires a typed identity ([qualification](qualification/damaged-recovery-2026-10-08.md)). [Typed storage errors](qualification/storage-errors-2026-10-08.md) now preserve disk-full, quota, read-only and permission classifications through wrapped failures; alerts retain their affected session, so a failed Open of another package keeps the current project saved. [Native Quit](qualification/native-quit-2026-10-09.md) now confirms unsaved previews, preserves them on cancellation and drains active encoding on exit; release native input, 1,097 app/adapter tests and release recovery replays pass. [Concurrent inspection](qualification/project-inspection-2026-10-09.md) lets a second app window view a fixed revision without taking the writer or retaining a live WAL read transaction; explicit reopen refreshes or takes ownership after the writer closes. [Explicit linked creation](qualification/linked-original-2026-10-09.md) now keeps the Original at its chosen location through native New, incomplete-project retry and `create-original --linked`, with full-source baseline tests, native bookmark relocation and portable export while the external path is unavailable. [Final DP-01 acceptance](qualification/project-acceptance-2026-10-09.md) reconciles every §20.1–20.5 obligation with code and current or retained qualification evidence; local acceptance is complete under §29.1. Remaining physical/provider checks are on the owner list. Export from an unsupported newer schema remains unavailable; §20.4 requires an explained read-only view and §20.5 permits supported render snapshots, so that limitation is not a new-schema export requirement. Physical power loss remains To verify (owner) under §29.1. |
| DP-02 | Exact frame/sample/source-time model including VFR. | Complete | [Correctable source interpretation](qualification/source-interpretation-2026-10-11.md) preserves immutable source admission and exact picture/sample clocks across revision-bound range/chroma changes and Undo. Typed rational clocks, VFR intervals, [independent picture mappings](SOURCE_VIDEO_MAPPING.md) and explicit selected-span endpoints in core and plan. [Original moment candidates](SOURCE_MOMENTS.md) retain exact VFR and fractional-sample selection boundaries. [`SourceSession`](../crates/deadpan-media/src/source_session.rs) builds original-PTS indexes from private verified media and performs persistent exact seeks. [Registration](SOURCE_REGISTRATION.md) retains validated indexes and exact common origin by historical revision. [Native source evidence](qualification/source-preview-2026-09-21.md) verifies actual encoded VFR endpoints: the legacy fixture ends with 1,001 ticks and an independently derived fixture with 3,003 ticks. Both retain all measured PTS and half-open source endpoints; an audio tail never extends the video interval. The [10,000 fractional-rate edits](qualification/aac-preroll-2026-10-08.md#10000-fractional-rate-edits) pass durable commands, all five sample-allocation residues, independent Original/resume PCM, reopened durable history, every shared audition/offline sample and real Render. All 10,100 pictures and 34 audio windows pass at exact endpoints with zero measured offset. [Pixel-aligned MP4 clean apertures](qualification/clean-aperture-2026-10-09.md) preserve independent SDR/HDR pixels and clocks through source admission and real Render; all 120 output pictures and four audio windows pass. The [eleven-case VFR recipe matrix](qualification/vfr-recipes-2026-10-09.md) verifies full Original endpoints, freeze, Repeat gaps, nested Retime, both speed policies, reverse, ping-pong, cutaway bounce and bleep through 867 exported pictures and 28 audio windows. Exact Hold origins fix reverse/bleep phase and preserve project-frame bounce endpoints. [Fractional apertures](qualification/fractional-aperture-2026-10-09.md) retain exact bounds through source qualification, rendering and analysis, with independent SDR/HDR pixels and real Vision tracking/face checks. The [interlaced-source qualification](qualification/interlaced-sources-2026-10-09.md) adds exact field cadence, temporal seek context and 62 verified exported pictures. [Explicit H.264 telecine](qualification/telecine-sources-2026-10-09.md) retains two/three-field timing through threaded and reordered decoding; the expanded debug/release Render matrix verifies 155 pictures and eight zero-offset audio windows. [SDR HEVC/High10](qualification/sdr-source-codecs-2026-10-09.md) retains full/limited-range eight/ten-bit pixels and exact B-frame timing; six public Render cases verify 72 pictures and six zero-offset audio windows. Bounded HEVC draining prevents stale pictures after repeated seeks, including existing HDR. | [Final exact-timing acceptance](qualification/exact-timing-acceptance-2026-10-11.md) reconciles every §4 obligation with the implemented clock contract and accumulated media evidence. The added combined-edit matrix passes 40 exports across 20 H.264/HEVC/VP9/AV1/ProRes, VFR, HDR and interlaced fixtures: 1,040 independently expected pictures, 36 zero-offset signal windows and exact terminal sample counts. Complete under §29.1. New operations still need timing evidence; unfinished effects, renderer/device qualification and performance remain with DP-07/DP-16/DP-17/DP-24. The measured 10,000-command workload uses six live nodes and does not qualify 10,000-node performance. |
| DP-03 | Structural Source/Sequence/Hold/Repeat/Retime primitives. | Partial | Validated tree, reversible commands, and [`deadpan-plan`](../crates/deadpan-plan/) picture mapping and [bounded structural audio queries](AUDIO_PLAN.md) through nested primitives, sparse overrides and compact repeat indexes. Audio keeps absolute sample allocation, original source coordinates, pitch stages and distinct Hold policies. Committed [golden renders](GOLDEN_RENDERS.md) hash every export-path I420 frame and 8192-sample limited-bus PCM block of seven representative structures (base, Repeat with gap, nested Repeat with occurrence overrides, Retime, freeze Hold, framing, escalation); exploded Repeats must hash identically. [Explode and duplicate](EXPLODE_DUPLICATE.md) recipes pass the CLI preview-versus-export suite. | Semantic range selectors, incremental fragment reuse, cross-platform golden qualification (current goldens are Apple Silicon/Metal and pinned-FFmpeg evidence), and full preview/export integration. |
| DP-04 | Stable anchors, attachments, nested occurrences, single-play overrides. | Partial | Compact stable play IDs and exact revision-aware boundary/range queries. [`marks.rs`](../crates/deadpan-core/src/marks.rs) adds persistent marks, ownership/loss policies, biased structural transforms, and named-mark selection with [integration/property tests](../crates/deadpan-core/tests/marks.rs). [Sparse overrides](OVERRIDE_VERIFICATION.md) and [automatic nested occurrence edits](OCCURRENCE_VERIFICATION.md) preserve variable durations, owned marks, exact picture mappings, and atomic history. [Independent sound clocks](OWNED_SOUND_VOICES.md#copying-retained-sound-clocks) retain attached PCM through supported ordinary Sequence moves and whole-owner copies, with fresh historical aliases, durable registers/history and current gate/gain policy. [Explode](EXPLODE_DUPLICATE.md) turns a Repeat into independent plays with fresh identities, keeping overrides, gaps, escalation, captions, occurrence marks and Hold permissions and closing each retained clock for its concrete play (bit-identical decoded PCM and golden hashes); Duplicate copies a beat, span or range with its owned attachments through the paste path; `EditScopedMany` edits chosen plays (plays 2-3 only) or a ranged gain inside one play. A [property suite](STRUCTURAL_PROPERTIES.md) runs random sequences of 27 structural command families with exact inverse, duration, mark-policy and retained-clock invariants. | Temporal transforms for attachments inside retained beat-sound journal scopes (explode refuses there), structural cuts applied to several plays at once, and native physical-input, IME and VoiceOver qualification of these commands. |
| DP-05 | Complete normal/visual/operator/command/camera/trim keyboard flow. | Partial | [Native workspace](NATIVE_WORKSPACE.md) adds counted navigation, persistent prefixes, pane focus, search, command entry, text/IME suppression, Sequence Enter/Backspace and breadcrumbs, current-depth `s`/`rr`/`dd`, typed `y`/`d` frame, beat and group-boundary selectors with `yy` whole-beat copy and [group objects](STRUCTURAL_SELECTIONS.md#group-object-workflow) for `ig`/`ag` operators and Visual editing, [counted frame cuts](EDITED_SLICES.md#frame-cuts-at-the-cursor), exact Hold commands and history. [Bounded explicit Repeat input](qualification/repeat-input-2026-09-27.md) retains eight batched wraps as separate commits and undo steps, preserving partial keys and reporting overflow/cancellation. [Same-frame footer and text focus](qualification/footer-layout-2026-09-27.md) preserve final text through immediate Escape and combined resize/input, keep pending keys single, and anchor the painted mode before presentation. `,i` reuses the Original without taking Kestrel's Cmd+Return. [Expanded UI qualification](qualification/workspace-layout-2026-09-27.md) checks 3,472 production routing cases against 62 global reservations and replays real editing/focus, Original moment, audition and Retime paths with visible controls and hints. A keyboard-only [`full-session` replay](UI_FEEDBACK.md) runs start screen to reopen (⌘N, Original/Edit navigation, `v`/`y`/`p`, `,v` Trim, `d5l`, `,h`, a Repeat gap ladder, `]r`/`[r` play and gap navigation, Camera target and `,z`, `:caption`, ⌘I and `,s`, gain/mute, undo/redo, `qa`…`@a` and `.`, scripted AI accept, ⌘E render with independent `verify-export`, `:close` and ⌘O) and checks that each step's key is painted where the action lives; it found and fixed eight teaching gaps, including command completion for keyless commands. `]r`/`[r` complete keyboard play stepping in [Repeat contents](SCOPED_EDITING.md). The [`layouts` replay](KEYMAP.md#logical-and-physical-keys) covers QWERTZ/AZERTY motions, counts and command entry and IME composition in the command line, caption text, word corrections and YouTube URL fields, with router fixes for layout fallback keys, a physical-position fallback for non-Latin letters (Cyrillic, Greek, Hebrew), typed-text reading in the Camera, Trim, Slip and Place slice routers, visible alternatives when Kestrel reserves a layout's `[` or `@`, and IME Preedit plus Escape in one batch. The extended Kestrel audit (live source digest matched) passes 21,845,328 routing cases against 62 reservations with no conflicts, including `]r`/`[r`, typed-text routing and Option-produced brackets. `:close` uses File › Close Project readiness and never discards drafts. | Remaining operators, sound-bearing Repeat transforms and occurrence-local temporal edits, plus implicit Repeat gap recipes. `ib`/`ab` and shared layout routing for Corrections, URL and Marks are implemented and tested. Physical keyboard delivery, dead keys, real OS IME and VoiceOver acceptance are To verify (owner). |
| DP-06 | Registers, macros, semantic dot-repeat, configurable bindings. | Partial | [Persistent registers](NAMED_REGISTERS.md) retain Original ranges, edited slices and Macro programs across edits, Undo and reopen. [Whole-node selectors](qualification/explicit-node-selectors-2026-10-22.md) retain exact authored identity through staged ordinary-Sequence resolution, empty-node selection, recording/dot and structural paste, using the actual immediate parent. [Whole-node unions](qualification/discontiguous-node-selectors-2026-10-22.md) and [interval unions](qualification/discontiguous-range-selectors-2026-10-22.md) preserve separate components, one immutable capture and shared marks through atomic Cut, durable registers and seam paste. Interval selection records signed cursor-relative pairs and retains canonical finished geometry without a hull. [Corrected transcript-search sets](qualification/transcript-search-selectors-2026-10-22.md) retain raw query intent and complete forward phrase coverage, publish against transactional proposal/correction authority, and clear stale native geometry while preserving historical captures. [Semantic macros](SEMANTIC_MACROS.md) record frame/beat/group motion, Visual selections, frame/beat/range cuts, beat/range/group copies, typed operator-motion and `ig`/`ag` selectors, Original or Edited paste/replacement and named calls. [Original mark macros](qualification/original-mark-macros-2026-10-18.md) resolve frame/start/end motion, letter marks and calls on the active measured Original clock, preserving the independent Edit context and native jump trail; remote mark-only saves preserve coalesced selection and inspector metadata. [Whole-Original reuse](qualification/whole-original-macros-2026-10-16.md) records `,i` with full measured A/V timing. [Place slice Apply](qualification/place-slice-macros-2026-10-18.md) records exact register paste/replacement proposals through one semantic transaction, with explicit refusals for unsupported placements and invocation-relative replay. [Ranged Zoom/Creep](qualification/ranged-framing-macros-2026-10-18.md) retains scale/curve and Keep or OriginalCenter intent, resolves each staged owner/range and establishes invocation-relative dot. [One-play framing](qualification/one-play-framing-macros-2026-10-18.md) captures explicit current Play authority and a separate presentation, maps both through nested isolation, preserves intrinsic owner and Follow clocks, and returns durable scoped receipts; saved Zoom/Call programs retain no concrete owner or play IDs. [Single-play pauses](qualification/scoped-pause-macros-2026-10-20.md) retain freeze/black length intent and exact inserted-Hold continuation through recording, dot and named calls, preserving the ordinary context. [One-play Hold lengths](qualification/scoped-hold-duration-2026-10-20.md) record exact resize intent and retain the actual mapped Hold at its start through dot, nested calls and subsequent edits. [Shared](qualification/shared-hold-duration-semantics-2026-10-21.md) and [mixed Repeat body Hold lengths](qualification/mixed-hold-duration-semantics-2026-10-21.md) preserve literal Default/Play choices, actual isolation and dormant authoring through native controls, recording, calls and direct-edit dot. [Selected-play Hold lengths](qualification/selected-hold-duration-semantics-2026-10-21.md) map every captured counterpart through staged edits and named calls, resolve the final primary boundary after all leaves, and retain duration-only dot intent for an explicitly reselected batch. [Selected fixed-Retime lengths](qualification/selected-fixed-retime-hold-duration-2026-10-22.md) extend either selected axis through one unclocked fixed window, retaining literal other-level choices, staged counterparts, cropped continuation and duration-only macro intent. [Owned-gap Hold lengths](qualification/owned-gap-hold-duration-semantics-2026-10-22.md) follow exact preceding Play authority, including dormant final definitions, with actual mapped counterparts and ordered primary continuation. [One-play Hold sound](qualification/scoped-hold-audio-2026-10-20.md) records exact prepared RoomTone or Silence, follows the actual isolated Hold through calls and dot, and preserves both cursors, picture and duration. [Named-mark ranges](qualification/named-mark-selectors-2026-10-22.md) retain exact addresses through selection, native operators, named calls and dot, resolving each staged document with existing clock/scope and role limits. They expand with bounded work and commit one [resolved Compound](COMPOUND_TRANSACTIONS.md) with retained intermediate captures; copy-only runs preserve timeline history. [Headless Macro commands](SEMANTIC_MACROS.md#headless-inspection-save-and-run) inspect, save, preview and execute against explicit revision/bank/context in closed and live projects. [Semantic cut repeat](SEMANTIC_REPEAT.md) re-resolves frame, beat, motion or current Visual targets while retaining the requested count/direction and register; exact saved proofs survive refresh failure. [Configurable Normal/Visual bindings](KEYMAP.md) share bounded loading/compilation, execution metadata, prefix teaching and audit enumeration; invalid files retain the shipped map with a diagnostic. Macros also record pauses (`,h`, `:hold`) as exact `InsertPause` lengths, framing (`,z`, `,c`) and [gag recipes](GAGS.md), and gain/saturation changes, J-/L-cuts and role-only deletes; `.` repeats those creative edits, reverses, tails and gags with their exact parameters at the new selection ([semantic repeat](SEMANTIC_REPEAT.md)). The native `,h`, `:hold-duration`, `s`/`:split` (a `Split` instruction since 2026-10-15; Trim and Slip Apply record `Trim` and `Slip` instructions while a macro records), `:retime`, `:pitch`, `+`/`-` (also over an Edit range), `:gain`, `,m`, `:audio-lag`, whole-beat `,z`/`,c`, cutaways and captions (beat or range), `:edge` and `:gag-set` now commit through that semantic Apply outside recordings too, so each is recordable and dot-repeatable with one Undo (`creative-dot` replay). A single [action registry](KEYMAP.md#action-registry) describes every action once and drives verb admission, completion, the searchable contextual Keys sheet and the generated [COMMANDS.md](COMMANDS.md); the fixed Camera, Trim, Slip, Place slice, room-tone, Gain, corrections, Marks, Jobs, Storage and Keys-sheet keys are declared there and verified against their routers, with a pre-registry snapshot proving no binding or verb changed. The searchable Keys sheet is an action palette: one highlighted result (Up/Down, Ctrl N/P), and Enter or a click runs it as a typed command, opens the command line with the verb and usage when it takes arguments, or performs a key-only action through its binding's own trie rule (prefixes enter their pending path), while actions unusable here stay listed and explain where and how they work (`palette` replay, `navigation::palette` and `help_scroll` tests). Grouped verbs and editor directions now have independently selected palette forms, retaining the exact chosen command or binding through deferred execution ([qualification](qualification/palette-forms-2026-10-17.md)). | Remaining Macro editing instructions (broader Place slice placement intent and general occurrence edits), role selectors beyond ranges inside one beat and remaining temporal occurrence scopes; dot-repeat of other one-play edits and target-sampled ranged or face zoom; a schema-driven argument grammar, strict logical provenance, physical layout/IME qualification and full parser/transaction/replay acceptance. |
| DP-07 | All time/delivery operations in Section 8. | Partial | Core commands insert/delete/move/group/ungroup nodes, wrap/update structural repeats and [Retimes](RETIME_EDITING.md), and change Hold duration/provider. [Ordinary Hold clocks](qualification/hold-duration-clocks-2026-10-20.md) preserve surviving sample phase through unbound suffixes and shared Repeat resizes without accumulating silent-owner captures. [Fixed affine windows](qualification/fixed-retime-hold-clocks-2026-10-20.md) retain moved interior Source and Repeat clocks at unchanged root duration, preserving outer placements and dormant default bindings. [Processing-input Hold clocks](qualification/processing-hold-clocks-2026-10-20.md) retain per-owner PointCeil phases through Preserve/Shift, including historical output bindings, default/owned plays and RoomTone gaps, while rebuilding current canonical DSP. [Exact-pause speed updates](qualification/clocked-speed-update-2026-10-20.md) let an eligible ordinary tape-speed stage change output duration while retaining its exact selected input, descendant clocks and historical audio carriers; native preview, inspector and semantic macros share that resolution, and both shorter and longer release exports pass all picture and complete-audio checks. [Selected crop carriers](qualification/source-crop-carriers-2026-10-20.md) admit exactly empty historical audio support and return suppressed zeros for direct silent Holds while preserving nonempty Source/RoomTone transfer limits. [One inner Repeat level](qualification/inner-repeat-speed-2026-10-20.md) also permits exact-pause speed updates around default/owned plays and independent gaps, checking every historical rate class without expanding plays; per-carrier PCM and a 22-picture complete-audio release export pass. [Atomic linked range moves](ATOMIC_MOVES.md) retain whole-unit identities and exact audio entries within and between ordinary Sequence parents; native `:splice` previews both joins and selects the complete committed range. Pure Split retains complete contexts and logical marks. Native current-depth Split/Repeat/Retime/delete/Hold-duration edits use that path and refresh the stopped-frame picture plan. Atomic InsertTime retains sample phase through Source/Hold fragments under ordinary Sequence groups and freezes a measured original picture below live ancestors. [Repeat escalation](REPEAT_ESCALATION.md) adds exact per-play gain and centered scale steps (additive or multiplicative) to an editable Repeat through one reversible `SetRepeatEscalation`, validated against every play, applied by the shared picture plan and authored audio bus, and set natively with `:repeat 3 gain-step=3dB zoom-step=0.08` (core, plan, real-PCM and replay tests). [Gag recipes](GAGS.md): `:gag long-answer`, `escalator`, `non-sequitur`, `one-more-time` (gaps that shorten after each play) and `nothing-happens` (room tone from a copied Original moment cut to true silence) expand versioned recipes to ordinary beats under a group pinning recipe, version and parameters, as one Undo (core expansion/inverse and `gags`/`recipes` replay tests). `SetRepeatGaps` sets a Repeat's default gap and independent gap Holds as one retained-clock transaction, and `:repeat N gap=120ms gap-step=-40ms gain-step=3dB zoom-step=0.08` changes count, gaps and steps together (or wraps a plain beat) as one recorded instruction; the One More Time recipe export matches its preview. [Reversed pauses](REVERSE_AND_TAILS.md): `:reverse 8f` (reverse hiccup) and `:ping-pong 12f` insert a Hold whose `Reverse` picture and sound play the preceding Original passage backwards (the ping-pong without repeating its turning picture), resolved by the shared pause resolver, recordable, and exported with their reversed click located independently. `:gag are-we-done` adds the sixth recipe. `:lift` cuts a Visual range and refills its time with a silent black pause, and `,b`/`:bleep` refills it with the same pictures played forward over a synthesized tone (`HoldVideo::Play`, `HoldAudio::Tone`); both are one recorded compound and export-verified. `:gag-inspect` lists a recipe's exact expansion before applying it, One More Time takes seeded variation (`vary=20% seed=7`) whose seed is pinned and whose resolved gaps are stored, and `:recipe-save a` / `:recipe a` / `:recipe-inspect a` keep a modified group as a project-local recipe ([gags](GAGS.md); `recipe-library` replay; the varied export matches its preview). A pause for All plays (`:scope all`, then `,h`/`:hold`) goes into the shared definition (also of nested Repeats) and grows every default occurrence, retaining every play, gap and suffix on its own sample clock (decoded PCM, core and `scoped-plays` replay; [pause insertion](INSERT_TIME.md)). [Mixed and selected-play pauses](qualification/mixed-repeat-pauses-2026-10-20.md) retain each explicit Repeat choice, exact per-play clocks and independently captured Holds through one atomic edit and one Undo; actual native, PCM and release-export checks pass. [One-play Hold duration](qualification/scoped-hold-duration-2026-10-20.md) resizes the existing Hold with exact isolation and downstream sample-clock preservation, including RoomTone and admitted generated-picture fallback/preparation. [Mixed Hold duration](qualification/mixed-hold-duration-semantics-2026-10-21.md) retains literal All plays/This play body scope, maps generation requests and accepted-media preparations through actual isolation, and verifies all 240 emitted pictures and 384,384 audio samples at zero offset. [Selected-play duration](qualification/selected-hold-duration-semantics-2026-10-21.md) resizes captured Hold bodies in one atomic history entry, preserving each physical audio clock and mapped counterpart even when total duration is unchanged; four selected-body release exports also verify all 240 pictures and 384,384 audio samples at zero offset. [Selected fixed-Retime lengths](qualification/selected-fixed-retime-hold-duration-2026-10-22.md) preserve physical phase and accepted-provider history at fixed extent, including Undo closure of clone-born intents; sixteen release exports verify 1,312 pictures and 2,101,304 presented samples at zero offset. [Owned-gap lengths](qualification/owned-gap-hold-duration-semantics-2026-10-22.md) extend mixed/selected duration to live and final dormant gaps, retaining per-owner sample phase and accepted-provider history; six release exports verify 411 pictures and 658,257 samples at zero offset. [Nested FollowSpeed pauses](qualification/nested-speed-pauses-2026-10-18.md) keep each live speed stage, exact intermediate clocks, individual suffix sample entries and the authored project duration, including further pauses through the inserted adapter. Black-frame punctuation and the remaining rows are tracked in [Section 8 coverage](SECTION8_COVERAGE.md). | Reversing composite structure or at other speeds, variable rate, attached-sound transport when a processing subtree changes, speed updates outside the qualified exact-pause affine family, Hold insertion beneath nested pitch-processing stages or with Original-moment payloads, range planning across Repeat and Retime occurrences, remaining operations and semantic targeting (see DP-06 selectors). Accepted AI Holds (`generated-pause`, `generated-repeat`, `generated-reframe`) and register callbacks have export fixtures. |
| DP-08 | All framing/picture operations and keyboard target selection. | Complete | [Scoped saved-target ranges](qualification/scoped-target-sampling-2026-10-25.md) qualify exact In/Out-minus-one sampling through nested Plays and Retime, full-owner fixed centers, explicit Camera proposal/save composition, inclusive Out navigation and atomic history, with final strict/native/media checks and retained counterexamples. [Targeted escalation Explode](qualification/targeted-explode-2026-10-25.md) preserves live targets, canvas containment, gains, gaps and history in ordinary editable plays. Core/Plan/App checks, both complete emitted-media cases, final 60/120 Hz native replays and independent review qualify DP-08.4; the native witness also fixes clipped Camera help. [Chosen-boundary return from existing paths](qualification/abrupt-return-2026-10-25.md) verifies the native Split/inner-owner Camera Reset composition and emitted files, and fixes compact target-label crashes; DP-08.2 is locally qualified. [Existing Follow parameter editing](qualification/follow-retune-2026-10-25.md) preserves target, fallback, exact clock and ramp through native Arrival/Curve controls and semantic Keep replay; focused checks, final painted replays and two complete release exports/history cases qualify DP-08.3. [Scoped framing ranges](qualification/scoped-framing-ranges-2026-10-25.md) add exact contained Time ranges to explicit nested Repeat Plays through a bounded decoder-free owner map, with fresh semantic replay and full release/media/history verification. [Pointer target authoring](qualification/pointer-targets-2026-10-25.md) adds temporary rectangle drawing, body/corner edits and point placement/correction with explicit Enter saves; held drags revoke on keyboard, focus and raster changes. [Ordinary ranged live Follow creep](qualification/ranged-follow-creep-2026-10-25.md) retains live centers and exact independent scale windows across all five ordinary owner kinds, with native, semantic, media and portable-history verification. [First-class point targets](qualification/point-targets-2026-10-24.md) retain center-only geometry through exact source-time evaluation, nested Follow, manual corrections, copy and durable history. Camera creates and corrects them without an invented box, keeps target/framing commits separate and refuses rectangle tracking before work. Core/Plan, App, CLI/Models, Python protocol, final native and complete emitted-media checks pass. [Camera salient-region proposals](qualification/camera-regions-2026-10-25.md) adds one shared exact-picture Faces/SalientRegions/Both protocol, pinned saliency2, explicit mixed Camera acceptance and atomic region:N framing; focused Jobs/CLI, real Native worker, recorded App/release and optimized debug UI checks pass, including110 regions checks plus Kestrel and inspected picker labels. current59-source release visual/quiet/media/portable-history and narrow copied-bundle relocation pass; original107 exit1 stays failed while audited continuations establish101 runnable latest passes with six fixture SKIPs. Independent raw audit passes10/10 groups; three immutable evidence collections and their post-publication audit pass with two explicit nonauthored sidecar omissions; controlled synthetic positives do not establish arbitrary-object accuracy. [Authored framing and Camera](FRAMING.md): static/enveloped operations, exact owner clocks, intermediate clipping, counted pan/zoom, numeric fields and a numbered picker of saved targets, center and corners; Camera follows a picked target (`t`), adjusts a follow's scale with a plan-exact preview (outer follows re-resolved; the `targets` replay checks preview equals committed picture), refuses center nudges while following, and draws or corrects point and rectangle targets with the keyboard ([framing](FRAMING.md#keyboard-point-and-rectangle-targets), [targets](TARGETS.md#in-the-native-app)); [earlier qualification](qualification/framing-2026-09-24.md) includes actual Metal/CPU comparisons, migration and native review. [Captured pauses](CAPTURED_FRAMING.md) retain input composition separately from live framing; [their evidence](qualification/captured-framing-2026-09-26.md) records remaining GPU/native checks. [Canvas transactions](PRESENTATION_BASIS.md) preserve time and reevaluate normalized framing. Escalating crop: a Repeat's per-play scale layer composes inside its own framing, centered or, with `:repeat … zoom-target=current`, moving toward a saved target while clamped inside the picture ([Repeat escalation](REPEAT_ESCALATION.md); plan clamp test). [Cutaways](CUTAWAYS.md): picture-only attachments on a host beat show an exact Original selection (hold, loop or gap fit) through the shared picture plan while the host's sound is untouched; they follow Split, copies, isolation, deletion and Trim prefix growth, and `:cutaway register=r` places one over the Edit range (core, plan split-survival and replay tests). [Following creep](qualification/follow-creep-2026-10-24.md) adds whole ordinary Source Keep-center animation with live/fallback scales, retained owner clocks, semantic replay/dot, exact Camera preservation and explicit Static release; all-frame media, complete PCM/history and native replay checks pass. [Whole-Hold following creep and captured projection](qualification/hold-follow-creep-2026-10-24.md) add exact retained-view target resolution and Hold output-clock authoring; Core/Plan, CLI, independent renderer geometry, final native and complete emitted-media checks pass. Independent raw-result, complete portable-history and immutable evidence verification pass. [Whole-Sequence following creep](qualification/sequence-follow-creep-2026-10-24.md) adds ordinary selected-group authoring with complete group scale clocks and per-provider target clocks; focused and broad checks, review, final native and complete emitted-media checks pass; independent history/raw-result and immutable evidence verification pass. [Whole-Repeat following creep and live clipping](qualification/repeat-follow-creep-2026-10-24.md) add ordinary whole-Repeat authoring and irreversible live-scope target visibility; focused and broad checks, review, release, native replay, both emitted-media cases and independent raw-result audit pass; the original derived wrapper failure is explicitly reconciled and immutable package verification passes. [Whole-Retime following creep](qualification/retime-follow-creep-2026-10-24.md) adds ordinary Retime-owner authoring with separate output-scale and fractional provider-target clocks; focused and broad checks, review, release, native replay, actual emitted-media and complete history/final raw-result verification pass; the earlier audit-script schema stop is preserved and immutable package verification passes. [Scoped whole-owner following creep](qualification/scoped-follow-creep-2026-10-24.md) adds valid nested explicit-Play authoring for all five owner kinds through existing EditScoped authority; focused and broad Core/Plan/App checks, three reviews, strict/format/release, same-candidate native replays and full emitted-media verification pass; final fourth independent raw/history audit passes with preserved earlier attempts; final third two-package verification and all-three-tree readback pass, with earlier attempts preserved and four explicit metadata omissions/zero authored omissions. [Zoom and creep](FRAMING.md#zoom-and-creep-commands): `,z` punches in to 1.35× following the selected target (center fallback with a message), `:zoom S target=current\|center\|ID curve=step\|linear\|smoothstep` is a smash zoom or eased change, `:creep from= to= target=` eases toward the target's plan-exact position where the creep arrives, `:zoom off` is an abrupt return, and each applies to the Edit range inside the beat; a follow keeps following on a scale change and a camera path is replaced only by an explicit `target=`, as one `SetFraming` captured at command entry (pure construction and capture unit tests and the `zoom` replay). Macros record whole-beat framing, ordinary ranged Keep/OriginalCenter Zoom/Creep intent ([qualification](qualification/ranged-framing-macros-2026-10-18.md)) and black pauses (`InsertPause { black }`); `:framing-save a` keeps the selected beat's framing (for example an off-center stare) as a one-instruction macro that `@a` applies to another beat, and a static off-center export matches its preview. Black-frame punctuation: `:hold 12f video=black` inserts a silent Background pause ([pause insertion](INSERT_TIME.md); service test and replay). [Captions](CAPTIONS.md): one-line text attachments on Source/Hold hosts with a delay and per-play reveal, rasterized from the bundled Inter font and composited by the shared GPU pass before display and encoder readback in the viewer and export (`:caption`, recordable; Metal composite test, viewer readback in the `captions` replay, and an export that a caption-free revision flags exactly where the caption was). Camera's [numbered face picker](qualification/camera-face-picker-2026-10-24.md) appends exact-picture proposals without renumbering existing choices, pages them with Tab/Shift+Tab, saves only the explicitly selected target and retains Camera for a separate framing commit; refusal, late completion, Undo/Redo and animated Follow continuation are verified. `:zoom S target=face:N` and `:creep … target=face:N` run Apple Vision face detection on the displayed Original picture in the process-isolated `deadpan-track` worker; faces are proposals ordered left to right, and only the chosen one is saved as a target together with the framing in one Compound and one Undo when the captured context is unchanged ([framing](FRAMING.md#face-proposals); `faces` replay with scripted faces through the real service, real Vision on a drawn-face fixture). Video-only deletes (`:delete role=video`) show the background through a removed-picture cutaway, and J-/L-cuts keep every picture through a cutaway over their rolled stretch ([role edits](ROLE_EDITS.md)). [Section 8 coverage](SECTION8_COVERAGE.md) inventories every operation. | None within the primary-Mac DP-08 scope after sampled-target composition is qualified. General scoped Split and structural transforms remain DP-04/05, framed-child Ungroup is DP-03.3 and full effects are DP-16. Physical input and quality/listening checks remain To verify (owner). |
| DP-09 | All audio operations with preserved intentional dynamics. | Partial | Limited [Original/edit/sound audition](PLAYBACK.md) connects canonical PCM to the native device. [Root sound events](SOUND_EVENTS.md#persisted-root-sounds) have qualified durable commands, sample-exact overlays and per-voice gain before the shared limiter; chronological root ripple edits preserve sample phase; [native root placement](qualification/native-sound-placement-2026-09-27.md) adds exact movement, gain, edges and removal. [Beat-owned sounds and independent clocks](OWNED_SOUND_VOICES.md) add qualified owner-local recipes, independent occurrence processing and exact PCM transport for unchanged scopes through supported Sequence edits, whole-owner copies and recopies. [Aligned sound scopes](qualification/aligned-sound-scope-2026-10-23.md) retain ordinary ancestor processing through a new single-Source/Hold group, its EQ/saturation, explicit Split forests, whole-group copy and relocation, with exact PCM, durable history and verified release output. Broader structural transforms stay open. The shared [finite oversampled limiter](AUDIO_MASTERING.md) verifies emitted tiles and preserves source-aware context across reads; full audio authoring remains open. [Raw DSP qualification](qualification/audio-2026-09-20.md) retains failed targets. The [canonical worker prototype](qualification/audio-canonical-2026-09-20.md) supplies the single schedule now used by the bounded [production DSP adapter](AUDIO_DSP.md). [Source preparation](AUDIO_PREPARATION.md) implements exact-phase resampling and explicit matrices without loudness normalization. [Plan-driven source PCM](SOURCE_STAGE_AUDIO.md) binds exact spans to historical qualified media, including repeats, silent Holds and FollowSpeed retimes. [Continuous Preserve stages](AUDIO_STAGE_PREPARATION.md) retain exact fractional grids and nested history with bounded preparation and source/layout-aware caches. [Streamed input](qualification/streamed-input-2026-10-11.md) removes the full input allocation and 21.8-second edit cap: a two-minute decoded-WAV signal retains the exact continuous schedule under a PCM budget smaller than its input. The subsequent [disk PCM cache](qualification/disk-pcm-2026-10-11.md) streams long outputs to private files, with bounded memory/disk, exact reads, cancellation and nested admission. [Active preparation progress](qualification/audio-progress-2026-10-11.md) shows real worker activity, current-stage frames and request elapsed time in Original/Edit and Sound, preserving clocks, cancellation and warm reuse; [idle preparation](qualification/audio-warming-2026-10-11.md) now warms a bounded stopped-cursor horizon, including future Preserve stages, without opening output. [Active preparation](qualification/audio-ahead-2026-10-11.md) now retains up to 29.867 seconds of canonical PCM ahead of the device. Complete persistent cache lifecycle remains open. [Room-tone loops](ROOM_TONE_AUDIO.md) use explicit ranges and exact crossfades, with [qualification](qualification/room-tone-audio-2026-09-21.md). [Authored edges](AUDIO_EDGES.md) add reversible hard exceptions and shared post-mapping fades. Per-play escalation gain is added on the Repeat owner before mixing and limiting ([Repeat escalation](REPEAT_ESCALATION.md)). Native `,m` mutes a Visual range inside a beat or the whole beat, `:audio-lag` sets a Source's explicit audio offset, `:sound-cut` ends a placed sound abruptly at the Edit cursor (bed drop) and `:gag nothing-happens` authors room tone then silence; mute-range, audio-lag, bed-drop and room-tone exports match their previews with independently located sound ([preview/export](PREVIEW_EXPORT_VERIFICATION.md)). [Hanging tails](REVERSE_AND_TAILS.md): `HoldAudio::Tail` renders a deterministic per-channel comb/all-pass reverb (RT60 measured 1.09 s) or 300 ms echo of the processed Original sound heard over the two seconds before it, read live from the current plan at render time, ringing for its maximum and fading to exact silence, as one canonical cached Hold block (`,t`, `:tail`, recordable); reversed Hold sound uses the same preparation; reverb and echo exports match their previews. A serialized [saturation stage](AUDIO_GAIN.md#saturation) (`:saturate 12dB`, tanh after clip gain by default, per voice or summed owner bus before the limiter), [fixed pitch shift](RETIME_EDITING.md#pitch-shift) (`:pitch +3st`, `PitchPolicy::Shift` on the canonical processor at any speed), [J- and L-cuts](ROLE_EDITS.md#j-and-l-cuts) (`:jcut`/`:lcut`: a Roll plus a picture-keeping cutaway) and [role-only deletes](ROLE_EDITS.md#role-only-deletes) (`:delete role=audio\|video`, `:select role=`) are recordable semantic instructions with real-PCM or plan tests, replays and export fixtures that match their previews. [Stereo balance](qualification/stereo-balance-2026-10-11.md) now persists through captured commands, macros, dot and single-play isolation, preserving other treatments and both clocks. Shared PCM, all 419 audio/Gain replay checks and a 54-picture export with two zero-offset audio windows pass; separate emitted-AAC measurements preserve channel silence and the authored level ratio. [Shared nonlinear group mixing](qualification/group-mix-2026-10-11.md) now sums Original and owned voices at each saturation owner, preserving independent root sound scope, repeat clocks, stage order and bounded admission; nested and emitted-file checks qualify this path. [Fractional-sample moves](qualification/group-clocks-2026-10-11.md) preserve shared constant treatments and ignore provably absent Original clocks. [Independent input automation](qualification/input-automation-2026-10-12.md) now retains each input's gain/mute clock before shared saturation/EQ; one-sample mute, envelope, direct-convolution and cold-read checks pass, with 973 plan/audio tests and three archived baseline failures. [Automation through shared stages](qualification/shared-automation-2026-10-12.md) now retains per-contribution clocks after saturation/EQ and through parent treatments; all six stage orders, cancellation, cold partitions and permission isolation pass, alongside 1,069 audio/plan/playback tests and a new real release export. [Stored recursive EQ](qualification/authored-eq-2026-10-11.md) adds ordered semantic recipes and nested Original processing with retained history, silence and tail checks; real limited preview and H.264/AAC export pass. [Shared EQ](qualification/shared-eq-2026-10-12.md) now mixes Original and independent sounds through nested EQ/saturation with per-voice silence permissions, saved crop history and verified H.264/AAC output. [Native EQ](qualification/native-eq-2026-10-12.md) adds captured buffered controls, complete typed recipes, dot/macros, removal and single-play isolation; 500 final visual replay checks and a separate performance run pass. [Selected tail sends](qualification/tail-sends-2026-10-12.md) now admit beat/group and independent sound contributions into allocated pauses, with native captures, 12 new PCM checks and verified release export. [Continuous owner returns](qualification/wet-owners-2026-10-12.md) add captured beat/group wet levels before parent processing, bounded complete history and consuming-sample permissions. [Independent event returns](qualification/event-returns-2026-10-12.md) add retained-clock effects on root and attached sounds before saved delivery routing and parent processing. | Broader incoming-tail/loop policy qualification (including the spec's `custom` Hold policy), room-tone waveform and Repeat-gap/fragment authoring, remaining mixed-voice EQ/filtering qualification, authored layout choice, full signal/format/listening corpus, remaining fade integration, remaining gain-envelope/tail interactions and partial attached-sound scope edits, all remaining audio operations, preview/export equivalence, devices, long-clip preparation and cache lifecycle. |
| DP-10 | Local transcript, timing refinement, shot/silence proposals. | Partial | [Local transcription](TRANSCRIPTION.md): validated word-timed transcripts with exact Original timing and search; a process-isolated whisper.cpp 1.8.3/Metal worker under the shared supervisor with verified model/PCM inputs, hashed artifact admission and real cancellation; analysis PCM from the Original's retained audio through the canonical resampler; schema-59 storage outside history; `transcribe`/`transcript` commands; automatic app transcription with a rail transcript, search and exact picture jumps; and word/sentence motions (`w b e W B`) in Original and Your edit with `iw aw is as` objects composing after `d y r`, resolved by projecting the transcript through each staged document, `/` with `n`/`N` transcript search in both contexts, and the rail's Your edit transcript in playback order (core, planner, projection and replay tests). [Qualification](qualification/transcription-2026-10-04.md) includes a native run. [Speech activity](SPEECH_ACTIVITY.md): Silero VAD v6.2.0 through the same worker and analysis PCM (protocol 2 `DetectSpeech`), measured 10 ms energies, the `deadpan-silence-1` pause rule, schema-60 storage outside history (additive upgrade from 59), a `pauses` command, automatic app detection including Originals already transcribed, and pack version 2 reusing an installed recognizer; on a synthesized 18 s interview (`say -v Samantha`) the rule reports all nine 180–220 ms between-phrase pauses within 10–30 ms of their energy silences. Pauses project onto Your edit like words (quiet Original pictures plus freezes, generated pictures and gaps) and drive `]p`/`[p` motions and `ip`/`ap` objects composing after `d y r`, with each analysis reporting its own absence (core, projection and replay tests, including `dip`). [Shot detection](SHOT_DETECTION.md): every Original picture decoded exactly against its qualified index from the verified snapshot and reduced to a grid/histogram signature with three-byte changes (including a skip-one comparison that rejects returning flashes), the `deadpan-shots-1` rule, schema-61 storage outside history, `detect-shots`/`shots` commands, an automatic app scan with the count on the Original card, and `]s`/`[s` motions and `iS`/`aS` objects over projected shot occurrences; a synthesized 1080p clip's five hard cuts are found at the exact picture, a returning flash is rejected, and the release scan measures 69 pictures/s at 1080p24 (decode-bound) (analysis, media, store, CLI, service and replay tests including `diS`). [Analysis corrections](ANALYSIS_CORRECTIONS.md): manual word and pause corrections stored as regions of the analysis clock apart from the rebuildable proposals (database schema 66, keyed by Original content and audio stream, never model, so transcribing or detecting again keeps them under the `deadpan-corrections-1` overlay rule), with their own versioned and bounded Undo/Redo outside edit history; unreadable or no-longer-applying corrections are reported, refuse word and pause operators with the reason and can be discarded explicitly, and explicit validation parses every stored value; edit text, split with a space, join, remove and move word edges by 10 ms or to `deadpan-edges-1` measured energy edges, and remove, add and resize pauses, in the keyboard-first `:correct` sheet; search, word and pause motions and objects, macros and headless `transcript`/`pauses` read the corrected analyses (analysis property test, store, CLI, app service and `corrections` replay tests). Pauses draw as quiet bands and shots as hairline ticks under the Original range and on Your edit's beat cards, mapped through the plan like the motions (replay check). On 2026-10-05 `ggml-base.en` scored 5.08% WER on all of LibriSpeech test-clean (5.4 h, one long file per chapter; 3.92% without one repetition-loop and one truncated chapter, both silent high-confidence failures), and word timing is coarse: sentence-initial starts are a median 315 ms off and sentence-final ends run a median 650 ms late ([record](qualification/transcription-accuracy-2026-10-05.md)). Shot detection measures `deadpan-picture-signature-2` (23 bytes per picture, including blend-linearity comparisons over 2–25-picture half-spans), and the `deadpan-shots-2` rule adds dissolve, fade-out and fade-in proposals (3–48 pictures, one boundary each) to the unchanged hard cuts; its gradual thresholds are provisional, tuned and qualified on one film with three true transitions. Scans resume from 10 s checkpoints in the operational schema-66 `shot_scan_progress` table, through an exact keyframe seek, byte-identical to an uninterrupted scan, and run 4 decode threads with pipelined signatures (447.8 pictures/s at 1080p24, against 124.7). On Caminandes 2: hard cuts 18/18 precise with 18/24 recall, gradual transitions 2/2 precise with 2/3 recall ([qualification](qualification/shots-2026-10-05.md)). [Local word refinement](qualification/word-refinement-2026-10-25.md) now supplies real English phoneme proposals, waveform/uncalibrated support, manual adjustment, real comparison audition and explicit one-Undo acceptance with pinned/stale guards, including the relocated signed runtime and original-source offline pack smoke. | Original operators with word motions, scheduling by visible range, pause rule qualification on varied real speech, accuracy on conversational and noisy speech and detection of long-form recognizer failures, general word/pause audition from the correction sheet, sentence-boundary correction, recognizer failure detection beyond read English (fuzzy loops, failures leaving under 5 s of speech, conversational/noisy/music material), non-US physical delivery of `[`/`]`, low-change cuts, blends shorter than three pictures, gradual-rule qualification beyond one film, and transition pictures in `aS`. |
| DP-11 | Selected target tracking with manual correction and loss handling. | Partial | [Point targets](qualification/point-targets-2026-10-24.md) now retain their own geometry and manual correction ranges; tracking starts from rectangles, and saved points refuse before preparation or work. [Selected-target tracking](TRACKING.md): a process-isolated `deadpan-track` worker decodes the verified Original through the pinned descriptor-only decoder and runs Apple Vision `VNTrackObjectRequest` under the shared supervisor with hashed-artifact admission, bounded cancellation/deadline drain and an exact check of every decoded and analysed PTS; the pure `deadpan-track-1` policy stops at the first stored shot boundary, measures motion in the display aspect, rejects low-confidence and implausible jumps instead of following another subject, interpolates gaps of at most six pictures, marks longer gaps lost and holds the last confident position until a manual keyframe, and applies corrections all-or-nothing to their range only (unit and property tests). `track --save` maps the path to a core [attention target](TARGETS.md) with stated rounding, state and confidence rules, provenance and tolerance-bounded compaction (core `region_at` now interpolates moving samples), committed as reversible `SetTarget` against the head captured before tracking (stale heads refused, existing ids only with `--replace`); `track-correct` re-tracks only a correction's range. On a synthetic moving-square fixture with occlusion and a hard cut the path is within 1 px, marked lost behind the occluder, stopped at the cut, never jumps to the second square; the saved target reloads, reproduces the path, resolves a `Follow` layer in `RenderPlan`, and a correction changes only its range. [In the app](TRACKING.md#in-the-app), Camera `T` and `:track` run the same host code on one bounded, cancellable job thread drained on close/shutdown, saving against the head captured at command entry; Camera `c` corrects at the displayed picture and re-tracks only that range; service tests and the `targets` replay use a scripted Vision seam with real range, policy and saves. | Real-person footage for tracking quality (spec Section 25, To verify (owner)); rotated or anamorphic real footage beyond the synthetic fixtures; the app's real-worker path exercised in tests or replay; incremental scheduling (needs a policy continuation, reverse decoding and app job priority); general non-face region proposals. |
| DP-12 | Local AI hold generation, exact seams/duration, variants, acceptance. | Complete | [Closure reconciliation](qualification/ai-hold-reconciliation-2026-10-25.md) confirms the implemented and locally verified §12 behavior under §29.1; all eight fixed criteria already scored 1. No new inference or model-quality pass is claimed. A [real supervised MLX development adapter](qualification/model-worker-2026-09-21.md) uses exact bridge planning and interior sampling, with decoded-file timing/color/hash checks. [Generated Hold semantics](GENERATED_HOLDS.md) retain sampling and resize fallback. [Dedicated store acceptance](GENERATION_ACCEPTANCE.md) binds the selected Ready receipt, retained inputs and derived assets to one reversible edit. Generic ingress remains guarded. [Accepted picture admission](qualification/generated-pictures-2026-09-29.md) decodes retained schema-3 Generated Holds without a model in native and captured-revision readers; [The headless AI pause chain](AI_HOLDS.md) runs conditioning from the Hold's boundary pictures, the supervised development LTX MLX worker, bundle qualification, publication, Ready and explicit `accept-hold`; a measured real run generated, accepted and rendered a 30-frame bridge with a symmetric canvas raster policy, and ordinary edits keep pending requests reconciled through a boundary-context resolver. The [native app workflow](AI_HOLDS.md#native-app-workflow) runs conditioning and the real worker on one bounded job thread per project with writer-applied records, shows stage/step/elapsed progress and the runtime's own unavailable text, reads Ready candidates from the store, previews them through the shared generated-picture path without an edit, and accepts through one undoable edit (`:hold-provider ai`, `:generate`, `:cancel-ai`, `:preview-ai`, `:accept-ai`, `:discard-ai`); session changes drain the job before releasing the writer. A real app-service run reached Ready in 96 s and accepted; the `ai-pause` (scripted worker) and `ai-pause-ready` (real bundles) replays pass. [Variants](AI_HOLDS.md#native-app-workflow): every attempt of a request is a seeded variant (`ProviderSelection::for_attempt`, checked by the store against the attempt ordinal); `:generate N` runs up to four attempts in one job and adds them to the Hold's current request while its boundary pictures are unchanged; the inspector lists variants with thumbnails decoded off the UI thread from each sampled master; `:next-ai`/`:prev-ai`/`:pick-ai N` choose through the store's selection; Discard is the durable eviction (not undoable). `:audition-ai` and playback while previewing play the proposed acceptance document, admitted against the exact committed revision (`Snapshot::proposed_generated`), so the pause's own sound plays with the candidate pictures. `generate-hold` (`--variants`, `--another`) and `accept-hold --attempt` route through an open app's live endpoint as the app's own job. A real run generated two variants of one request (Ready at 81.9 s and 173.3 s, consecutive seeds, different masters) and accepted the first; the `ai-variants` replay (synthetic footage through real qualification) and store, service, CLI, live-endpoint and playback tests pass. A packaged `Deadpan.app` now carries the private runtime and uses the installed bridge pack: from a scrubbed copy it generated (76.8–98.9 s), accepted and rendered an AI pause, with decoded pixels identical to the development environment for the same seed ([record](qualification/ai-runtime-2026-10-05.md)). [Comparison](AI_HOLDS.md#comparing-variants): `,x`/`:compare-ai [before|N]` switches the viewer and audition between the pause as committed (Before) and the chosen variant, and `,n`/`:next-ai` between variants, at the same frame; a playing audition restarts the newly shown snapshot in the same window from the exact heard content sample, a paused one retargets its exact sample (Space resumes there), and admitted previews are retained so switching back is immediate; the `ai-compare` replay (18 checks, including batched `,n,n`; Kestrel audit 21,884,016 cases with a matching live source) and navigation tests pass. It is a toggle, not a split screen. [Retention](STORAGE.md#retention-of-unaccepted-ai-variants) (database 68): offered variants that are not kept (`:keep-ai`, `keep-hold`), picked, chosen or accepted expire 7 days after Ready; an idle-gated check after opening and every 6 hours plans off the writer, refuses clock anomalies (a long gap is confirmed in the Storage panel with E after showing what would expire; a clock behind the records is never confirmed), caps each pass at 32, expires them and removes unreferenced `Media/Generated` masters through reference-tracked cleanup after the 24-hour grace; accepted and history-named media is never expired or removed; inspector rows and the Storage panel's AI VARIANTS rows show it; store, service and CLI tests pass. [Source joins](GENERATED_HOLDS.md#source-joins) are hard cuts at exact frames: `tests/generated_joins.rs` proves on real media through production code (synthetic worker) that f-1 and f+N are the unchanged L and R, f..f+N-1 are the sampled master frames at (j+1)(M-1)/(N+1) and never conditioning endpoints, and Undo restores the freeze; an advisory entry/exit discontinuity reading (`generation::joins`, uncalibrated Smooth/Noticeable/Jump) appears per variant in the inspector and `generate-hold`. [Speech preservation](GENERATED_HOLDS.md#speech-preservation): accept, Undo to the freeze and switching variants leave the edge-faded and limited buses bit-identical to the freeze revision across the timeline, and later speech is shifted by exactly B(f+N)-B(f) samples outside the 96-sample seam fades (cfr-bframes and a continuous chirp Original). [Source/colour context](AI_HOLDS.md#source-and-colour-context): manifest schema 5 records each boundary picture's measured colour, stream, receipt, exact PTS and prepared presentation/content rectangles (or generated/authored-black origin), refuses uncovered colour, and qualification fails on a model colour-space mismatch; checked against an independent decoder and a fresh picture-path preparation. No qualified model corpus. | Listening qualification of the audition (To verify (owner); replays simulate delivery), native physical-input and VoiceOver checks of the AI controls (To verify (owner)), and the full qualified model corpus (To verify (owner) where it needs real-person footage). [Motion and optional guidance](qualification/hold-controls-2026-10-07.md) now persist through native/CLI generation and retries with versioned prompts and strict text/token bounds. [Motion/lighting rejection](qualification/bridge-quality-2026-10-07.md) now inspects every native frame pair before Ready with authored-time motion limits, explicit unavailable coverage and versioned retained evidence. A real-media flash regression preserves the fallback, revision and earlier Ready selection. [Endpoint rejection](qualification/bridge-endpoints-2026-10-07.md) compares the actual sampled first/last pictures with retained PNGs inside captured geometry, and host schema 5 binds the measurements and policy to those objects. Broad discontinuity fails before Ready; real-media entry/exit and one-frame regressions preserve the freeze and previous Ready selection. [Face geometry and mouth checks](qualification/bridge-landmarks-2026-10-07.md) now inspect every canonical native picture and both retained inputs through pinned Vision revision 3. Conservative boundary-anchored face geometry and continuous native mouth segments retain explicit unavailable coverage; later faces are checked and later loss cannot erase an earlier rejection. Host schema 6 binds and recomputes this evidence; older schema 3/4/5 footage remains readable. [Selected-region drift](qualification/bridge-region-2026-10-07.md) now captures an explicitly chosen saved target at exact decoded boundary PTS and follows it through the native candidate with pinned Vision revision 2. Context schema 4 and host schema 7 retain seeds, raw boxes/confidence, exact native coverage and recomputed rejection policy; weak or lost tracking cannot erase an earlier rejection. Target corrections/removal stale the request, and omission retains the current request's target choice or absence. [Scoped Repeat/Retime generation](qualification/bridge-scopes-2026-10-07.md) uses exact definition-clock boundaries and the full intrinsic Hold duration. Default and stable Play requests have independent clocks; acceptance isolates one play and Undo/Redo follows proven address mappings. Scope changes revoke pending previews and comparisons. Context 5 and host profile 8 retain this provenance. [Automatic replacement after lengthening](qualification/ai-replacements-2026-10-07.md) now commits the longer fallback and durable preparation together, preserves verified prior controls, prepares off the writer, and exposes retry/discard without automatic acceptance. Real-model, packaged export, repository and replay verification pass as recorded. [Counted AI insertion, provider controls and native timing](qualification/ai-insertion-2026-10-07.md) now implement `,a`/counts and `:hold 1.5s video=ai audio=silence` through one atomic silent fallback plus durable preparation, existing-Hold `:hold-provider ai`, scoped keyboard reversion after later edits, and exact duration/speed reporting that preserves accepted sampling when shortened. The repository gate, 79 real-model assertions, final model-free packaged export, all 69 runnable native scenarios in parts and five separate release performance scenarios pass as recorded. The corrected preview/export verifier passes all 144 pictures and five audio windows, reporting 110 unobservable neighbor comparisons explicitly; 13 unit and nine optimized SDR/HDR integration tests pass. Two fixture-only replay skips and timing outliers remain recorded. [Automatic replacement after changed source inputs](qualification/ai-boundaries-2026-10-07.md) now commits the original edit, exact dependent-provider fallbacks and durable replacement intent atomically. Measured Original/generated identities, immutable accepted origins, bounded dependency resolution and fresh Redo work survive full history validation. Real-model generation, atomic Undo/Redo, final native feedback, packaged model-free export, the repository gate completed in parts and separate release responsiveness checks pass. The §12.2 one-sided extension implementation is recorded below. Its [exact timing, temporal context and native conversion foundations](qualification/extension-foundations-2026-10-07.md) and [supervised extension worker](qualification/extension-worker-2026-10-07.md) are verified, including both real model directions, immutable input capture and exact generated-only sampling. [Project-context capture](qualification/extension-context-2026-10-07.md) now verifies actual project pictures, intervening affine spans and a bounded conservative transition guard. [Durable extension inputs](qualification/extension-inputs-2026-10-07.md) now share independently captured temporal identity through requests, relevance, replacement decisions and history, preserving the resolved operation through missing and restored context. [Retained extension evidence](qualification/extension-evidence-2026-10-08.md) now binds that descriptor to private picture signatures and recomputes input continuity; the Python worker checks the same descriptor and binary contract. [Extension pixel checks](qualification/extension-pixels-2026-10-08.md) now inspect every generated-native pair on the exact extension clock and the actual sampled joins, including one-frame output and explicitly unconditioned opposite seams. [Directional geometry checks](qualification/extension-geometry-2026-10-08.md) now inspect a true single anchor and generated-only frames through pinned Vision, including reverse selected-region tracking and chronological mouth checks, with retained and recomputed evidence. [Complete host qualification](qualification/extension-bundles-2026-10-08.md) binds strict worker provenance, independent capability, canonical media and all required quality reports, then revalidates the retained envelope without a model. [Saved extension acceptance](qualification/extension-saved-2026-10-08.md) now retains the complete operation-specific bundle, verifies every dependency before Ready and explicit acceptance, and reopens exact sampled pictures without a model. Both directions, one-frame definition edges, durable undo/redo, portability and missing-input failures are covered. The inspector distinguishes generated-only timing and conditioned, unconditioned or absent seams. [One-second extension qualification](qualification/extension-one-second-2026-10-08.md) now covers both real model directions from production project capture through full host admission and exact pixel oracles; both fallbacks remain unchanged. [Normal Extension integration](qualification/extension-native-2026-10-08.md) now selects an independent pack, resolves both directions before capture, runs the shared native/CLI attempt lifecycle, preserves captured modes through replacement and exposes mode/seam controls. Automated verification, relocated pack installation and all eight real 0.5/1/2/3-second direction cases pass, including exact pixel sampling, explicit acceptance, Undo/Redo and verified portable export with source-package, model/runtime and network access denied. Native accessibility checks confirm saved controls and cancelled command entry without edits. Still open under DP-24 and Gates A/E/G: resident model reuse and fixed text-embedding caching (§13.5), the distilled 2B macOS/MPS and MLX precision comparisons, and full cold/warm interactive performance measurements (§13.4). Generation and acceptance handle Default or one stable Play per request; multi-play batching is unavailable, without adding a DP-12 closure gate beyond §5.2 and §12.6. Threshold calibration and real-model seam quality are on To verify (owner). [BT.709-to-sRGB conversion](qualification/conditioning-color-2026-10-07.md) is implemented for conditioning and join measurements; focused tests, the full gate and real packaged generation/acceptance/render pass, and older accepted media still renders. Split-screen comparison and a retention setting are optional refinements: the spec requires keyboard comparison and a visible retention policy, both implemented. |
| DP-13 | Model/runtime manager, safe downloads, offline pack installation. | Complete | [Model packs](MODEL_PACKS.md): schema-3 manifests compiled into the app with relative file paths, separate license layers (SPDX or LicenseRef, attribution, link, terms summary, compiled full text, explicit `acceptance_required`) and an approved `ltx-2.3-q4-bridge` pack (31 files, 36.15 GB, LTX-2 Community License and Gemma Terms of Use) matching the worker's qualified baseline receipt; HTTPS range-resumable staging with the requested user agent, free-space checks, exact size/SHA-256 verification, license acceptance enforced before any byte is staged and recorded in the receipt, a smoke test before activation (whisper recognition, or the bundled AI runtime's `worker.py --check`), side-by-side versions, an exclusive install lock and cancellable stall-detecting downloads. Offline installation from a folder (APFS clone, every file hashed, symlinks refused) or an uncompressed ustar/pax archive (names compared with the manifest only; links, duplicates, bad checksums and wrong sizes refused) and `export` to that archive form. CLI parity: `models list/license/install/import/export/remove [--partial]` with `--accept-license` and codes `ModelPackLicense`/`ModelPackVerification`. Unit tests cover manifests and licenses, resume, range-ignoring servers, unexpected offsets, stalls, locking, corruption, space, cancellation, tampered receipts, consent refusal, folder and archive import, tamper and hostile-archive refusal and export round trips; CLI tests cover listing, licenses and refusal before staging. A scrubbed copy of the packaged app imported the 36.2 GB pack offline with its bundled smoke test in 12.4–13.4 s ([record](qualification/ai-runtime-2026-10-05.md)). The native Models panel is recorded in [Model packs](MODEL_PACKS.md#in-the-app). [Signed updates](UPDATES.md): a project Ed25519 key (public half compiled from `models/update-keys.json`, private half outside the repository) verifies domain-separated envelopes before parsing; `models update <file\|https-url> [--from]` admits a pack version only for a compiled family with a shipped runtime version, retains the envelope, installs it side by side through the smoke-tested path and only then switches `.active/<pack>.json`; `models rollback` reselects the previous version, nothing is deleted, and the active version cannot be removed; downgrades and replayed serials need `--allow-downgrade`; licenses that require acceptance or are new or changed against the compiled pack are shown and must be accepted first; envelopes are retained atomically (torn files replaced), pointers are locked across processes and their directories must be private; consumers use the selected version and fall back to the compiled one, with a reported note, when its files are missing or resized or its key is untrusted. The Models panel verifies a chosen update, shows it for review with its licenses and acceptance, then applies the verified bytes; it activates or removes the kept version (the `model-packs` replay passes 27 checks including an untrusted refusal, the review, an applied signed version and activating the previous one). Offline distributions refuse signed-update versions. [Offline distribution](PACKAGING.md#offline-distribution): `cargo xtask offline-dist` copies a signed bundle and exports packs with its own CLI into a checksummed folder with `distribution.json`, licenses and instructions; `offline-dist-verify` re-hashes it and imports each pack with the distribution's CLI in a scrubbed environment (whisper v2 imported in 6.1 s; the 36.15 GB bridge pack exported and re-imported with the bundled AI runtime's smoke test in 22.4 s). Interrupted transfer: a local rustls server drops connections every 5 to 9 MiB of a 48 MiB file and the real `HttpsTransport` resumes from each `.part` length to a verified pack, with Cancel/Resume, range-ignoring and corrupted-resume cases; resumes are conditional on the first response's `ETag`/`Last-Modified` (`If-Range`, tested with a changed tag) and a wrong `Content-Range` keeps the partial bytes; the transport now uses the platform verifier as documented. No approved pack is gated (recorded). | [Real packaged verification](qualification/followup-2026-10-07.md) passes signed bridge update, full installed retry validation, generation with versioned host/worker provenance, explicit acceptance, rollback and verified Render. [Schema-3 constraints](qualification/model-constraints-2026-10-07.md) declare conditioning, frame/raster limits, precisions and hardware/OS requirements; signed updates must preserve the runtime contract. The full gate, fresh packaged import and real schema-3 signed update/generation/acceptance/rollback/render pass. No remaining implementation or agent-verifiable acceptance gap in the audited scope (§29.1). To verify (owner): the real 36 GB download with a physical interruption and clean-machine acceptance on a second Mac. |
| DP-14 | One Original from a YouTube URL with bundled JavaScript support. | Complete | [Final acceptance audit](qualification/youtube-acceptance-2026-10-11.md) closes the complete normative import workflow under §29.1. [Network confinement](qualification/youtube-network-2026-10-11.md) puts all helper connections behind an authenticated, bounded public-YouTube gateway and blocks direct proxy bypass. [Size bounds and packaged 4K import](qualification/youtube-limits-2026-10-11.md) reject overflow/oversized transfers before assembly and verify a full-resolution AV1/Opus Original. [Confirmation thumbnails](qualification/youtube-thumbnail-2026-10-11.md) add bounded worker-side loading, explicit optional-image states, accessible aspect-correct display and compact scrolling with fixed decision controls. [Source range/chroma correction](qualification/source-interpretation-2026-10-11.md) is available after import through an undoable native control. [Headless YouTube import](YOUTUBE_IMPORT.md): pure URL normalization (watch/share/Shorts/embed/live/music/mobile hosts to one 11-character ID; playlists only with an explicit `v=`; HTTPS-only, no authority tricks) with property tests; pinned yt-dlp 2026.08.19 (embedded yt-dlp-ejs 0.8.0) and Deno 2.9.7 installed by `downloader install` with exact size/SHA-256 verification, atomic never-overwrite publication and re-verification before every use, `downloader status --probe` and `doctor` versions; supervised yt-dlp runs by absolute path with argument vectors, `--ignore-config`, `--no-plugin-dirs`, no remote components, only the pinned Deno, a cleared private environment and explicit-file cookies copied owner-only and deleted; metadata-first refusal of playlists, live/upcoming streams and inadmissible videos before transfer or package creation; Deadpan's [qualified AV1/VP9/H.264 and Opus/AAC selection](qualification/youtube-quality-2026-10-09.md), preserving original language and normal dynamics; exact stream-copy assembly in the isolated media worker (protocol 2 `remux`, pinned FFmpeg 8.0.3); creation through the shared single-Original path (`project create-original`), and schema-62 private provenance outside history. Unit, worker and stand-in end-to-end tests cover arguments, environment, selection, refusals and error mapping; a real run imported a 146 s 1080p24 CC-BY Blender short as a Ready project in 15 s. [Native New-from-URL](YOUTUBE_IMPORT.md#in-the-app): the start card's URL field and an over-project sheet (`⌘⇧N`, `:youtube`, File menu) with live normalization or the exact refusal, an explicit verified helper install (never silent), title/uploader/length/streams/destination confirmation before media transfer or package creation, staged progress with cancellation, explicit cookies file, actionable failure codes and opening through the ordinary Open path, on one app-owned job thread split into `inspect`/`download_and_create`. Job, router and library tests use a scripted downloader; the `youtube` UI replay now passes 125 workflow checks plus the shortcut audit; env-gated real app-job runs created a Ready project in 16.6–16.7 s. [Signed downloader updates](UPDATES.md#downloader-updates): `downloader update --manifest <file\|https-url>` and the Models panel verify the Ed25519 envelope, minimum app version, platform, exact hashes, yt-dlp's Mach-O content pin and Deno's signer, install into versioned managed directories (never the bundle), probe the new set and switch `updates/state.json` atomically; `downloader rollback [--baseline]` restores the previous selection; helpers older than the baseline and replayed serials need `--allow-downgrade`; precedence is a compatible active update, then the bundled baseline; a passed-over update is reported by `downloader status`, `doctor`, import events, the app's confirmation step and the Original's provenance; integrity failures refuse; the replay floor comes from retained verified envelopes as well as `state.json`; `downloader rollback --baseline` (and the app's Use the baseline downloader) recovers an unreadable state after reporting it; retained envelopes are written atomically in private directories. Real signed manifests downloaded, probed and activated yt-dlp 2026.08.19 and an explicitly allowed older 2026.07.04, then rolled back to each and to the baseline. | To verify (owner): physical keyboard/IME, VoiceOver, an authenticated cookies/account workflow, clean-machine installation and a signed helper update on a second Mac. The exact checks are listed below; no known DP-14 implementation or reference-Mac verification remains. |
| DP-15 | One local Original plus external audio-only effects, managed/linked assets and relinking. | Complete | [Original ownership](ORIGINAL_MEDIA.md) retains complete originals through APFS clone/verified copy, records linked locations, checks identity on relink, and returns private snapshots. [Source registration](SOURCE_REGISTRATION.md) qualifies explicitly selected streams, retains measured indexes and receipts, and registers/inserts with exact common-origin placements atomically. [Store tests](../crates/deadpan-store/tests/source_registration.rs) cover historical alias reuse, rollback, deduplication, undo/redo and relocation. [Automatic basis tests](../crates/deadpan-store/tests/presentation_basis.rs) cover primary intent, final-rate placement, audio clock locking and geometry adoption. [Background preparation](IMPORT_PREPARATION.md) keeps file verification and receipt preparation independent of the writer and rechecks source freshness at commit. | Managed and [linked Original creation](qualification/linked-original-2026-10-09.md), recovery/retry, bookmark resolution and native relinking are verified. [Catalog sound recovery](qualification/sound-recovery-2026-10-11.md) verifies managed repair, linked sound location changes and history preservation. [Sound placement](qualification/native-sound-placement-2026-09-27.md) has native evidence. [Final local/sound acceptance](qualification/local-import-acceptance-2026-10-11.md) reconciles §15.1 and §20.3: current release missing/corrupt-sound refusal, identical restoration, portable export without the external sound, explicit container-audio selection and catalog deduplication pass. All 360 picture comparisons, six zero-offset signal windows, three silent windows and 36 focused store tests pass. Complete under §29.1; File Provider eviction, external owner media and physical input remain on the owner list. Broader source-format and creative-operation coverage stays with DP-02 and DP-16. |
| DP-16 | Shared realtime/offline renderer, bounded decode and proxy paths. | Partial | [Streamed Preserve input](qualification/streamed-input-2026-10-11.md) uses one canonical processor with bounded borrowed windows, exact PCM comparisons and complete input dependency admission; [disk-backed output](qualification/disk-pcm-2026-10-11.md) removes the full-output memory requirement while keeping disk, work and deadlines bounded. [Preparation progress](qualification/audio-progress-2026-10-11.md) exposes actual worker activity through request-bound transport updates without advancing the heard clock or changing PCM; compact and normal offscreen layouts retain cancellation controls. [Idle preparation](qualification/audio-warming-2026-10-11.md) warms a bounded stopped-cursor horizon, shares canonical caches with Play and preserves waveform/Stop priority without output activation. [Active preparation](qualification/audio-ahead-2026-10-11.md) now buffers bounded canonical PCM ahead of the device; persistent cache reuse remains open. [Final-picture handoff](qualification/terminal-picture-2026-10-11.md) retains the final transport request through GPU submission at the exact audio endpoint. Twelve deterministic cases and seven real-device runs pass with all 8,206 pictures and no audio faults; one final picture explicitly completes after the admitted End update. [Generated FFV1 slice decoding](qualification/generated-slices-2026-10-11.md) preserves exact pixels with four preview threads and one offline/index thread. Five ordinary 1080p60 runs, one 4K30 run and both 30-lap loops present every picture; the long fixture retains a final-Original handoff failure. [Generated selection loops](qualification/generated-loops-2026-10-11.md) pre-admit the missing Original when playback begins on Generated; both 30-lap real-device runs present every picture at 1080p60/4K30. The first 4K Original request falls from 64 ms to 8.7 ms. [Closely spaced Generated providers](qualification/generated-transitions-2026-10-11.md) prepare during the current accepted Hold; two full-resolution 1080p60 runs present every picture across a one-frame Original gap, reducing second entry from 190 ms to 0.15–0.16 ms. [Generated playback decoder reuse](qualification/generated-playback-cache-2026-10-11.md) preserves Original look-ahead across accepted Holds and reuses the Generated decoder; three selection-loop laps match all 90 reference pictures. [Cold Generated preparation](qualification/generated-prefetch-2026-10-11.md) verifies the next accepted provider and decodes its first picture on one cancellable job within the captured horizon/window; four real-media tests verify adoption, cancellation and stale-session rejection. [Optional VideoToolbox decoding](qualification/hardware-decode-2026-10-11.md) preserves software pixels and clocks, with lazy bounded surface transfer. Long-GOP latency is worse on the reference Mac, so production preview remains on software; fast hardware integration stays open. [Source interpretation](qualification/source-interpretation-2026-10-11.md) is shared by preview, playback, immutable export, corrected proxy variants, AI inputs, shots and Vision analysis, with revision-bound invalidation. Structural picture plans plus persistent source decoding and [`deadpan-render`](../crates/deadpan-render/) shared SDR composition. [Metal qualification](qualification/source-preview-2026-09-21.md) compares 76 synthetic cases with a CPU reference; the app displays real decoded sources and exact plan-driven sequence frames through this pipeline. [Presentation state](qualification/preview-presentation-2026-09-21.md) retains actual sequence/revision identity through decode and GPU delays. Native Original/edit audition adds device-clock picture coalescing, context-bounded selection loops and exact paused-sample resume. The [SDR encoder pixel boundary](qualification/sdr-encoder-pixels-2026-09-28.md) reads the same composed working target into bounded owned Rec.709 I420; 172,260 synthetic codes exactly match its independent reference. [Committed project pictures](PROJECT_PICTURES.md) capture historical source receipts and immutable geometry; actual Metal retains repeated/frozen pixels through live writer edits, undo and redo. [Generated pictures](qualification/generated-pictures-2026-09-29.md) share strict six-object admission, canonical sampled-master decoding and retained geometry between both consumers. [Committed encoder pictures](qualification/export-pictures-2026-09-29.md) preserve that revision and framing through even-raster output, exact rational timestamps and one retained I420 result; actual Metal passes all 118 checks and 31 complete-plane references. [Committed encoded candidates](qualification/encoded-render-2026-09-29.md) now preserve those pictures and canonical PCM through a supervised native MP4 encoder, with bounded complete-plane and absolute-PCM fixture comparisons.  The [preview/export verification](PREVIEW_EXPORT_VERIFICATION.md) harness compares the shared committed picture path (at the SDR encoder pixel boundary) and limited audition bus with decoded public-Render output: 44 recipe fixtures pass with exact zero audio offsets ([2026-10-06 record](qualification/preview-export-2026-10-06.md)), including four accepted schema-3 Generated Holds made by the synthetic worker through real conditioning, bundle qualification, publication and acceptance (plain, two-play Repeat, reframed, shortened prefix; 50.0–52.6 dB minimum luma PSNR) and long-GOP B-pyramid 1920×1080 and 3840×2160 Originals (38.3 and 44.4 dB); the earlier fixtures measured 53.1–62.2 dB ([2026-10-04](qualification/preview-export-2026-10-04.md)). Interactive serving decoders now use FFmpeg frame/slice threading (one codec thread per core, at most 12). Seeks skip non-reference preroll pictures before the target when every SPS declares a fixed reorder depth and frame-only coding. Index measurement stays single-threaded, so receipts never depend on threading. [Tests](../native/deadpan-source/tests/threaded_seek.rs) compare every returned picture with a sequential single-threaded decode. Interactive preview admits Originals [progressively](PROJECT_PICTURES.md#original-admission-and-ownership): receipt-verified pictures immediately, complete fresh measurement in the background, with mismatch and interruption handled separately. Export keeps complete admission before the first picture ([seek qualification](qualification/seek-2026-10-05.md)). A rebuildable [preview proxy](PROXIES.md) tier covers Originals above 1080p. The isolated media worker encodes an intra-only VideoToolbox H.264 MP4 of at most 1920×1080 under a stall watch with one confirmed-teardown retry. Each picture keeps its Original picture's exact PTS and duration, verified independently along with sampled pixel fidelity and per-channel bias. Proxies are published atomically into the per-user `~/Library/Caches/Deadpan/Proxies` and read in place, never authoritative. Builds pause during playback, renders, battery, Low Power Mode and thermal pressure. The main viewer shows a proxy picture with a visible Proxy chip for a stopped seek, then the exact Original picture for the same request after a 150 ms rest. Proxy pictures never satisfy Camera, Slip or Trim gates, and export, render, verification, conditioning, tracking and thumbnails never read proxies. 4K30 long-GOP warm seek is 8.2–8.6 ms p95 through the proxy (Original 167 ms); the exact picture follows 213 ms p95 after an isolated request ([proxy qualification](qualification/proxy-2026-10-05.md), [refinement](qualification/pause-and-cold-seek-2026-10-06.md)). [Proxy builds resume](PROXIES.md#resuming) from completed keyframe-aligned ranges recorded in a private journal, after cancellation, a killed worker or a killed host; partial state is never an entry, and damaged ranges and foreign journals are encoded again (kill tests in `tests/proxy_resume.rs`; a 4K build cancelled halfway resumed in 20.6–25.5 s instead of 30–31 s). [During playback](PROXIES.md#playback-pictures) a picture whose measured exact decode would miss 70% of its period comes from the proxy with the same Original ordinal and PTS (per-picture `Picture::tier`, the Proxy chip and label, `:diagnostics` counts), while the Original decoder repositions ahead one picture at a time between requests (`SourceSession::decode_plan`, `begin_reposition`/`advance_reposition`; forward decoding past dropped pictures no longer seeks); the heard clock, requested frames and audio never depend on the tier, and a stopping transport replaces a displayed proxy picture with the exact one. The [playback record](qualification/playback-proxy-2026-10-06.md) (official `cargo xtask perf` playback and stress stages, every stage below load 3) measures 30 s real-device auditions with 0 underruns: 1080p60 and 4K30 as imported drop nothing (decode plus Metal p95 6.8 and 15.9 ms); a 4K edit with a Repeat restart every 2.4 s drops 19 pictures without the proxy and none with it (87 of 900 pictures from the proxy, exact again 207 / 272 ms p50 / p95 after each jump); 10,000-beat projects (Background Holds, and 10,001 Original Source beats at 1080p60 and 4K) drop nothing; and the 4K edit still drops nothing beside a fresh 4K proxy build and a 4K export (0.14× real time). Edit playback now [decodes ahead of cuts](PLAYBACK.md#audio-clock-and-pictures) at every raster (`deadpan_media::lookahead`): the plan is scanned up to 4 s ahead for the next picture that does not continue the Original, at most one companion decoder per Original (shared verified snapshot, receipt index and background measurement) is positioned at its exact picture on its own thread, and the decoders swap roles there; requests, generations and index checks are unchanged, and the proxy remains the fallback. In the [decode-ahead record](qualification/playback-proxy-2026-10-06.md#decode-ahead-at-cuts-follow-up-2026-10-06) the 1080p60 and 4K30 cut edits drop 0 pictures (14 and 22 without it in the same run; decode plus Metal max 6.3 and 22.0 ms), every restart is served ahead (37 and 18 swaps), the 4K edit needs no proxy picture, and the companion costs 118 MB (1080p60) and 518 MB (4K) of peak footprint during edit playback. Seek storms through the real preview worker keep its mailbox at most one pending plus one active request and resident memory bounded (4K: 684 → 1,115 MB at 4 ms requests). Legacy Accepted/Still providers are not read: every picture consumer refuses them explicitly (owner decision below). The shared renderer now also takes RGBA64 PQ/HLG sources into the same working space (1.0 = 203 cd/m²) and converts the composite to Rec.2100 10-bit encoder pixels; 344,520 actual Metal codes match an independent reference (PQ exact, HLG within one) ([HDR pixels](HDR_PIXELS.md)). [Selection-loop preparation](qualification/loop-lookahead-2026-10-11.md) now follows the captured audible window in Original and Edit, including measured VFR, partial-frame context and long final-picture holds. Internal cuts precede the seam; revision/domain/generation changes reject old scheduling. Thirty-second real-device Edit loops drop zero pictures at 1080p60 and 4K30 (17 and 12 without look-ahead), with zero underruns; loop-return maxima fall from 55.4/85.1 ms to 6.7/15.8 ms. [Real app Generated playback](qualification/generated-app-playback-2026-10-11.md) presents all 1,830/615 frames of the 1080p60/4K30 fixtures with accepted real LTX pictures, zero skips or audio faults, and complete ticket-bound GPU timings. [Real Generated export](qualification/real-generated-export-2026-10-11.md) passes 225 full-resolution picture comparisons and six zero-offset audio windows, independent pause timing probes and a 15-picture pre-acceptance fallback negative. | Realtime picture latency at the endpoint, full format/color matrix, mastered playback, effects, broader Generated-provider/loop stress, the 4K Original's own warm seek (167 ms p95; the proxy shows first), HDR preview/export beyond two recipes, and native-window playback. To verify (owner): acoustic synchronization and physical display latency. Legacy Accepted/Still readers are out of scope by the 2026-09-30 development-format decision: no V1 workflow creates them (no still import, AI pauses accept only schema-3 Generated Holds), old packages are refused by schema, and the generic headless vocabulary that can still author them is refused by every picture consumer ([record](qualification/playback-proxy-2026-10-06.md#legacy-accepted-and-still-readers)). |
| DP-17 | One-action automatic SDR/HDR YouTube-oriented output. | Partial | The [encoder experiment](qualification/encoder-timing-2026-09-28.md) and [AVFoundation comparison](qualification/native-audio-2026-09-28.md) retain real CFR/Rec.709/AAC files and exact event checks. Without edit lists, FFmpeg reads events 1,024 samples late; AVFoundation loses the opening event and reads later events 1,088 samples early. The reference aligns in both. On 2026-09-28 the user approved edit lists for verified encoder delay, padding and frame reordering; §22.3 retains complete emitted-file timing verification. [Actual renderer-plane H.264 checks](qualification/sdr-encoder-pixels-2026-09-28.md) pass normal/sanitized full-plane comparisons for two video-only fixtures, without extending AAC qualification. The [committed encoder-picture host](EXPORT_PICTURES.md) now supplies immutable revision/range, exact output timestamps, legal raster mapping and bounded real I420 preparation. The [isolated worker](RENDER_WORKER.md) now supplies bounded raw ranges with document binding and clean-exit artifact admission. The [native encoder and offline PCM boundary](qualification/native-encoding-2026-09-29.md) adds exact descriptor-only MP4, historical canonical audio reads, independently measured edit-list timing and fresh-decoder GOP evidence for bounded synthetic fixtures; hardware B-frame rejection remains retained. [Real project encoding](qualification/encoded-render-2026-09-29.md) adds seven isolated MP4 candidates with full fixture decode, exact markers and fresh-GOP evidence. The [production verifier](FINISHED_FILE_VERIFICATION.md) now checks actual clocks/edit lists, complete fresh-IDR GOPs and both AAC decode modes, retaining failed candidates for retry; [current evidence](qualification/finished-file-verification-2026-09-29.md) covers those seven fresh files. The [publication host](RENDER_PUBLICATION.md) adds exclusive destination staging, exact byte readback, a historical local report and atomic movie publication with explicit post-commit outcomes. The [durable render boundary](RENDER_JOBS.md) retains completed encodes across reopen and requires a fresh verification attempt; the [publication journal](RENDER_PUBLICATION.md#durable-publication-journal) adds exact durable rename permits and explicit APFS crash reconciliation. [Recovery qualification](qualification/publication-recovery-2026-09-30.md) covers twenty real process kills and forty fresh verifications. [Automatic admission](AUTOMATIC_ENCODER_ADMISSION.md) and [durable decisions](RENDER_JOBS.md#automatic-admission-and-recovery) now connect runtime-bound qualification to each fresh encode and preserve its original evidence through checkpoint retry and reconciliation. [Native/public Render](RENDER_JOBS.md#native-and-public-render) adds explicit preview decisions, exact committed-revision capture, native progress/cancel and closed-project CLI start/recovery. [Open-project routing](LIVE_PROJECT.md) extends Render to the existing native owner with exact cancellation and temporary-preview refusal. [Native saved-render recovery](qualification/render-history-2026-09-30.md) adds bounded stored-job browsing, checkpoint retry, historical re-encoding and exact destination reconciliation. [Preview/export verification](PREVIEW_EXPORT_VERIFICATION.md) decodes published movies and compares them with their committed revisions; sixteen recipe exports matched frame count, output grid, declared 1,024-sample priming and exact zero measured audio offset ([qualification](qualification/preview-export-2026-10-04.md)). The complete release export path is not qualified. [Automatic HDR output](HDR_OUTPUT.md) adds the branch end to end: HEVC Main10/H.264 High10 PQ and HLG Originals are admitted and decoded to RGBA64, the basis records the Original's transfer, and one receipt-derived decision per committed revision chooses HDR or tone-mapped SDR (BT.2408-style shoulder) for both preview and export. `AutomaticHdrV1` admits hardware HEVC Main10 with tags, a source `mdcv` and a measured `clli` for PQ; the verifier checks HEVC, color boxes, IDR GOPs and content-light bounds. On the M5 Max, PQ and HLG recipe exports measured 59.9 and 60.4 dB minimum luma PSNR in 10-bit code values with zero audio offset; SDR outputs stay byte-identical ([qualification](qualification/hdr-output-2026-10-05.md)). [SDR quality controls](qualification/encoder-quality-2026-10-10.md) fix the measured concurrent scene-cut loss with QP 24 and a three-times-target one-second rate ceiling. Concurrent release and packaged exports pass all 7,012 decoded pictures and 286 zero-offset signal windows; native sanitizers and the regression gate in parts pass. [Real Generated export](qualification/real-generated-export-2026-10-11.md) passes 225 full-resolution picture comparisons and six zero-offset audio windows, independent pause timing probes and a 15-picture pre-acceptance fallback negative. | A qualified software quality fallback (the OS software QP property is unavailable here), full mastering/effects, HDR display/EDR presentation, real camera HDR files, Dolby Vision/HDR10+, expanded raster/content/runtime qualification, and the complete native workflow and emitted-file corpus. |
| DP-18 | Nonblocking worker lifecycle, cancellation, stale result handling. | Partial | [`deadpan-jobs`](../crates/deadpan-jobs/) adds bounded typed framing, a revision-aware attempt lifecycle, native subprocess supervision, and [contained hash-verified snapshots](ARTIFACT_VERIFICATION.md). A [real MLX development worker](qualification/model-worker-2026-09-21.md) exercises this boundary. [Persistent requests](GENERATION_REQUESTS.md) atomically reconcile relevance; [attempts](GENERATION_ATTEMPTS.md) retain retries, validation receipts, candidate selection, and interrupted states across restart. The [render worker](qualification/render-worker-2026-09-29.md) isolates real SDR picture preparation with a separate protocol, exact document binding and clean-exit raw artifact admission. The [encoded child](ENCODED_RENDER.md) adds shared-deadline native picture/audio encoding, hostile-protocol tests and actual cancellation/byte-exhaustion/retry evidence. The separate [finished-file verifier](FINISHED_FILE_VERIFICATION.md) binds its report to private candidate bytes after clean teardown and preserves the candidate when verification fails. The [publication host](RENDER_PUBLICATION.md) preserves the verified candidate and retained paths on precommit failures, while distinguishing a committed movie whose final durability or integrity could not be confirmed. [Durable render attempts](RENDER_JOBS.md) add immutable intent, exact transition tokens, retained movie/manifest checkpoints, restart interruption and fresh verification retries; owner closure cancels live encoder/verifier supervision. The shared native service workflow now qualifies automatic attempts while Queued and atomically binds each selected decision before encoding. [Native recovery](qualification/render-history-2026-09-30.md) binds stored-job queries and recovery actions to exact sessions and historical targets, including current-session status for owner-started work. The app's [job coordinator](JOBS.md) registers every background job (AI pause generation, transcription, pause and shot detection, tracking, faces, proxies, renders, YouTube imports, model installs, portable copies) with explicit priority classes (preview/audio and editing are never jobs and never wait), a one-model inference budget and a one-scan budget with visible cancellable queueing, yield rules at safe boundaries (shot scans and proxies wait while the edit plays, renders or tracking run; proxies also during inference), and consistent cancellation and shutdown draining that keeps each job's own session/revision and durable-record contracts. App-connected generation waits for the model slot after recording its attempt; a cancel while queued ends it durably Cancelled without a worker. The keyboard-reachable, accessible Jobs panel (`:jobs`) lists state, progress, elapsed time and Cancel, and offers Retry/Discard for attempts interrupted by a crash (store listing and durable dismissal). Deterministic service tests run the real scripted AI and tracking jobs with stub transcription, scan and proxy jobs while editing (edits within budget, priorities and yields, cancellation drain, stale tracking refusal, shutdown release), plus the `jobs` replay. [Admission conditions](JOBS.md#admission-conditions) hold model jobs (AI pauses, transcription) at their admission boundary on battery, in Low Power Mode, under thermal pressure (the proxies' own `pmset` reading), while Original/edit playback runs and under kernel memory pressure (`kern.memorystatus_vm_pressure_level` through `sysctl`; critical pressure also holds scans), with the reason in the Jobs panel and inspector and a per-job Run anyway (O) that still respects the one-model slot. [Memory admission](qualification/memory-admission-2026-10-17.md) additionally requires the selected pack estimate plus an OS/editor reserve to fit observed headroom; this byte budget and missing observations cannot be overridden, and model release invalidates stale probes; AI picture generation now cooperatively yields to playback/rendering through sequenced, acknowledged inference-stream boundaries while retaining its model slot; [real scheduling qualification](qualification/inference-scheduling-2026-10-17.md) covers pause, resumed denoising and cancellation while paused. Several AI pauses per project (up to four) each record their attempt and queue for the model with their own row and Cancel; the Jobs panel shows remaining time only from measured progress; closing or quitting with requested work asks Quit when they finish, Cancel them and quit, or Go back, with the native quit guard handing ⌘Q to that question. Service tests cover the queue, conditions, Run anyway and chaos under load (the scripted worker's real process killed with SIGKILL while another generation waits, tracking runs, a transcription queues and a scan yields: the attempt ends durably Failed, the waiting job runs, edits stay within a five-second budget); the `jobs` replay drives held rows, O, per-row cancel and the quit question through real keys. | Broader measured memory envelopes and runtime allocation limits, running-pause policies for other model jobs, sleep policy for remaining analysis workers beyond [recognition](qualification/transcription-sleep-2026-10-17.md) and [standalone pause detection](qualification/activity-sleep-2026-10-17.md), and historical estimates, broader effects and render/inference interference beyond the [qualified ordinary playback loops](qualification/inference-interference-2026-10-17.md), ⌘Q/Dock Quit and VoiceOver on a physical Mac, expanded media qualification (hostile media inside the real encoder, Vision, whisper.cpp and MLX workers, and resource exhaustion inside workers), and chaos with hostile (protocol-violating) workers and real model processes. |
| DP-19 | Cache integrity and accepted-media portability. | Complete | [Host FFV1 conversion](MEDIA_CONVERSION.md) verifies generated pixels/timing; shared [object storage](ORIGINAL_MEDIA.md) verifies generated objects and complete originals. [Admission](GENERATION_ACCEPTANCE.md) requires six retained objects before Ready/acceptance and derives assets from measured spans. Relocation, undo/redo/revert and independent real-media readback are exercised. [Generated picture qualification](qualification/generated-pictures-2026-09-29.md) adds cold historical decoding after request staleness, exact prefix reuse, revocable worker readers and per-dependency corruption checks. Legacy receipts gain no inferred admission evidence. [Storage](STORAGE.md) adds reference tracking over every retained revision, register, live generation receipt, unreleased render job and `Snapshots/` checkpoint, a `project storage` report and native `:storage` panel with project and per-user usage, explicit cleanup that removes exactly a previewed set of unreferenced objects, unfinished writes, discarded or stale variants, released render candidates, proxies and downloader staging (preview off the writer, fresh writer rescan, 24-hour grace, reader `flock`, one identity-checked unlink each), and verified `project copy-portable` / File › Save Portable Copy… packages with managed originals and only referenced media. The [`offline_portable`](../crates/deadpan-cli/tests/offline_portable.rs) test renders and verifies an accepted AI pause with an empty `HOME`, no runtime and outbound IP denied, then again from a portable copy after the source is deleted, where re-derived conditioning reproduces the bound manifest. Store tests cover undone acceptance, stale variants, a held reader, linked originals and refused copies. | [Real-model portability](qualification/accepted-portability-2026-10-07.md) passes two verified copies, removal of the intermediate copy, complete history validation, accepted-picture comparison, cleanup and verified rendering with the original package, AI runtime, model storage and outbound IP denied. Measured source-clock/colour metadata is implemented and retained. No remaining implementation or agent-verifiable acceptance gap in the audited scope (§29.1). Another-Mac acceptance, physical power loss and File Provider eviction remain To verify (owner), with exact steps below. |
| DP-20 | Focused one-Original UI with visible keybindings and native accessibility. | Partial | [Gain scrolling verification](qualification/gain-scroll-2026-10-11.md) resolves the prior false failure through complete trackpad gestures and settled mouse-wheel observation. All 296 Gain checks pass in visual replay and release at 60/120 Hz, including full graph/axis visibility and four populated keyboard circuits. [`deadpan-app`](../crates/deadpan-app/) has labeled controls, visible context, shortcut help and native file panels. [Expanded UI replay and visual review](qualification/workspace-layout-2026-09-27.md) cover fourteen scenarios through a full run and focused follow-ups. Actual Metal captures show a larger viewer, visible Camera/pause actions, first-frame notices and pinned sound controls; pointer/wheel/text ownership, resize and 10,000-beat selection checks pass. [Repeat queue feedback](qualification/repeat-input-2026-09-27.md) exposes pending work and keeps the header Working through completion. [Footer qualification](qualification/footer-layout-2026-09-27.md) resolves first-frame command-exit gaps and clipped Original shortcuts, with same-input mode/geometry checks and actual Metal captures. Native keyboard/accent evidence remains in the [earlier qualification](qualification/source-preview-2026-09-21.md). The [design pass](qualification/ui-design-pass-2026-10-04.md) adds system fonts, consistent hierarchy and focus cues, two-row priority footer keys and a larger picture across 5,388 replay checks, plus bounded visible-card thumbnails through the shared renderer. The empty start surface now follows the product board's "Choose one Original" card (Choose video, YouTube URL, Open project) at default and minimum sizes, with keyboard-only URL entry, confirmation and cancellation covered by the `youtube` replay. The [accessibility audit](qualification/accessibility-2026-10-05.md) inspected the running app through the macOS Accessibility API and its announcement notifications. It fixed an AccessKit abort on the start screen's second Tab, gave painted panes spoken summaries (viewer frame, time, beat and Camera draft; selected beat), made saves and refusals live regions, labelled modal sheets as dialogs with initial focus, exposed full truncated text and named fields, and follows macOS Reduce motion and Increase contrast; the `accessibility` replay asserts the AccessKit tree, focus and focus ring. The [2026-10-15 GUI review](qualification/gui-review-2026-10-15.md) gives the picture priority at the minimum window: at 960×640 Your edit grows from 163 to 205 points and Original from 149 to 179, enforced by the `workspace` replay (200/175 points with clocks, caption and transport fully painted); the header status no longer wraps at 960 points (Trim pictures 115 to 147), Camera Cancel aligns with Apply, and the `backups` start card no longer paints an unsettled frame. The [follow-up fixes](qualification/gui-findings-2026-10-15.md) close those review findings, each enforced by a replay check that fails on the previous code: compact cards keep a 70x52 thumbnail, strip edges count hidden beats and fade a cut card, a pending thumbnail reads `Loading…` and a failed one `No preview` (only an authored Background is black), side-pane scrollbars sit in the pane margin, Trim's empty junction-error row takes no height (pictures 146 to 170 points), Help shows keycaps, the inspector wraps instead of truncating, a quiet compact notice joins the footer's mode row (minimum Your edit picture 205 to 231 points), Gain fits heading and graph, and compact Your edit with placed sounds grows from 145 to 201 points; the full visual replay passes 80 scenarios with 8,133 checks. | Full workflow, VoiceOver speech and navigation (announcement requests are verified, not speech), CJK IME and non-US layout acceptance, timeline waveform strips at other scales, the sound inspector's fixed label column wrapping values at 960 points, the pre-existing `audio-warming` replay timeout and other busy-command behavior in the [interaction review](INTERACTION_REVIEW.md). |
| DP-21 | CLI/JSON API with revision checks and dry-run. | Partial | [Shared headless API](HEADLESS.md): named Macro inspection/save/run/dry-run with exact bank receipts, explicit Child/Time/Object selections, whole-node identity, discontiguous node/interval unions and named-mark semantic selectors, and qualified register pastes; automatic Render for closed and open native projects with checkpoint retry/reconciliation; project/command/history operations, atomic linked range move, explicit migration, picture/audio-plan and source-PCM inspection, exact boundary and named-mark range resolution, original retention/inventory/verification/relinking, automatic project creation, source registration/insertion, geometry preview/adoption and structured errors. [Owner preparation](LIVE_PROJECT.md#background-preparation) routes retention, relinking, exact registration and consistent checkpoints through the native import worker and captured writer. [Registration subprocess tests](../crates/deadpan-cli/tests/source_registration.rs) and [audio inspection tests](../crates/deadpan-cli/tests/audio_inspection.rs) cover actual media, historical identity, read-only coexistence and stable failures. [Parity inventory](PARITY.md) maps 118 GUI actions to headless forms (92 Equivalent, 26 GUI-only, no Partial or Gap; the registry's `Parity` type now admits only those two). Inline semantic `macro` `apply` reaches the native single-action Apply path (gags, pauses, bleep, captions, cutaways, framing, gain steps), and `corrections` makes versioned `:correct` changes with dry run. Native-derived requests use the app's own derivation code, moved to the CLI crate and shared: `sound` (placed-sound place/move/nudge/gain/edges/cut/allowance/delete via `sound_events`), Macro `yank_original` (Original copies into registers), `project insert-original` (`,i`), `ai-variants`/`select-hold`/`keep-hold`/`discard-hold`/`dismiss-attempt` (shared offered-variant set; `--joins` gives `:compare-ai`'s readings), `gag-inspect` and `sound --write-sting`. Corrections, AI variant choices, storage cleanup (a dry run's `plan` removes exactly its files, as Storage P then R), clock confirmation and backup restore now run on an open app's writer through new live short operations and refresh its inspector, transcript, retention status or session; cleanup and clock confirmation plan and scan on read-only opens off the service thread and reply when done; restore refuses open drafts, reports a database that changed but could not be shown (`BackupRestoredNotShown`, `BackupRestoredUnverified`), and replies through the replaced owner's retired endpoint, which closes its listener and is drained on shutdown. [Parity subprocess tests](../crates/deadpan-cli/tests/headless_parity.rs) and [corrections](../crates/deadpan-cli/tests/corrections.rs) cover closed and live-owner routes, stale refusal and no-write dry runs; [native live tests](../crates/deadpan-app/src/project/tests/headless/operations.rs) and the variant test in [generation](../crates/deadpan-app/src/project/tests/generation.rs) cover app refresh, runtime registers and restore rebinding. | Complete command/selector surface (broader selectors and semantic grammar tracked in DP-05/DP-06) and complete recovery/failure acceptance. Live audition, interactive previews and session reports remain GUI-only by nature, with reasons in [parity](PARITY.md). |
| DP-22 | Self-contained zero-manual-setup personal bundle (no Developer ID signing or notarization, owner decision 2026-10-05). | Complete | [Packaging](PACKAGING.md): `cargo xtask bundle` builds a relocatable `Deadpan.app` in its own path-remapped target directory. It contains release `deadpan-app`, `deadpan-cli`, media, transcription, tracking and alignment workers in `Contents/MacOS`; the pinned LGPL FFmpeg 8.0.3 libraries relocated to `Contents/Frameworks` through `@rpath`; and a read-only yt-dlp/Deno baseline. An `Info.plist` key marks the packaged app. It always uses its own baseline and never falls back to the managed root. Each helper is anchored to its compiled pin: Deno by exact bytes plus Deno Land's signer requirement, re-signed yt-dlp by a signature-independent Mach-O content hash plus signature validity (and the app's expected local/Apple signer when present). A [private AI runtime](PACKAGING.md#ai-runtime) (python-build-standalone CPython 3.12.13, the 36 locked wheels with the macOS 26 MLX build, the pinned LTX source, the worker and a separate GPL `ffmpeg`/`ffprobe`) is assembled from verified pins into `Contents/Resources/ai-runtime`, signed file by file without entitlements, and is the only runtime a packaged app uses (developers may opt in to explicit variables); the October 6 bundle was 721.9 MiB (288.5 MB ZIP). Code is signed inside out with the hardened runtime. Deadpan and yt-dlp have no entitlements, which was measured, and Deno keeps its own. `codesign --verify --deep --strict`, a scripted `otool` audit and the staged bundle's own helper check must pass before publication. `bundle-verify` passes positive checks (smoke test, doctor, hardened helper probe, `create-original`, `render`, the bundled AI runtime and an offline pack import with its smoke test) and negative checks (tampered and deleted helpers with a managed fallback present) on scrubbed relocated copies with an isolated `HOME`. A real YouTube import from such a copy also succeeded.  The historical 721.9 MiB bundle, native shutdown, real packaged AI and manual app rollback evidence are recorded in the [2026-10-06 audit](RELEASE_AUDIT.md). The October 25 audit reopened helper trust. [Native-worker admission](qualification/native-launch-trust-2026-10-25.md) and [private-AI admission](qualification/private-ai-launch-trust-2026-10-25.md) now close that gap: compiled worker/library pins, a closed 12,264-node runtime inventory, expected signer/app seal, bounded interruption, real model Check/Inference, 61 expected tamper refusals and accepted-media export/Undo. The current local bundle is 792.1 MiB; local/ad hoc/disabled-AI builds and stale-host publication refusal are measured. | No remaining implementation or agent-verifiable packaging gap in the audited scope (§29.1). The compiled native closure and private AI inventory are now qualified before execution. The two rejected interior-Hold quality candidates remain failed evidence and establish no general model-quality or performance claim. Developer ID signing and notarization are out of scope by owner decision (personal app, no developer account); the scripted path is kept unused, and personal signing uses the dotfiles local identity. The GPL FFmpeg question does not arise for a personal, undistributed build. Signed downloader and pack updates with rollback are implemented ([updates](UPDATES.md)). Application builds have explicit version/checksum identities and a verified manual replacement and rollback policy; helper and model rollback remain independent. [Native online and offline-distribution workflows](qualification/native-install-2026-10-06.md) and the [corrected bundle follow-up](qualification/followup-2026-10-07.md) complete agent-verifiable acceptance under §26.6 and §29.1, including real model installation/generation, restart, publication recovery and fresh Documents export. True clean-machine acceptance and uninterrupted whole-app physical disconnection are To verify (owner). Bit-for-bit rebuilds are not a specification requirement; checksums and provenance identify each build. |
| DP-23 | License/SBOM/privacy/security requirements. | Complete | [Dependency inventory](DEPENDENCIES.md), native harness build/license hashes, strict bounded domain JSON, schema checks, and initial package-path protections. Qualification explicitly excludes the developer GPL FFmpeg build from distribution. [Bundled notices and SBOM](PACKAGING.md#notices-and-sbom): crate licenses are normalized (legacy `/` to `OR`) and validated against the vendored SPDX 3.27.0 identifiers, and crates without license files must be covered by a bundled SPDX text or the build fails. The generated `THIRD_PARTY_NOTICES.txt` covers the shipped binaries' normal Cargo closure (275 crates, with each crate's published license files including vendored whisper.cpp, SQLite, Signalsmith and fonts, plus authors, copyright lines and SPDX texts for crates without files). It also covers the FFmpeg LGPL-2.1+ notice (built unmodified, relocated and re-signed) with the exact source archive, hash, commit, host-path-free configuration and shipped library hashes, and the vendored, hash-checked yt-dlp/Python, yt-dlp-ejs and Deno notices. A CycloneDX 1.5 SBOM lists crates, FFmpeg, both helpers and the vendored native components, and per-file SHA-256 sums and build provenance accompany the bundle. The AI runtime adds `AI_RUNTIME_NOTICES.txt` (every component with validated SPDX expression, source and hash, the GPL statement with exact sources and configuration, and modified files) and each package's license files, plus SBOM components. Model packs carry separate weight and text-encoder license layers with compiled full texts and recorded sources, require explicit acceptance, and refuse hostile offline archives. Only the documented folder-access usage strings are declared. The [adversarial suite](ADVERSARIAL.md) fuzzes every untrusted input boundary (containers, codec records, decoders, project/command/patch/slice/context JSON, tampered packages, worker protocols, the authenticated live endpoint, yt-dlp metadata, pack archives and manifests, stored indexes and analysis artifacts) in regression, campaign and sanitized modes; the [2026-10-05 run](qualification/adversarial-2026-10-05.md) found no crash or bound violation, and its undetected register slot-row tampering is fixed by the schema-65 register bank digest. The [2026-10-06 release audit](RELEASE_AUDIT.md) adds complete regenerated Deno/V8 notices, bounded local diagnostic export, downloader secret redaction, hostile encoder/render/tracking/transcription/AI workers, 27 instrumented fuzz targets (41.3 million executions), the [privacy audit](PRIVACY.md), production AI network denial with real packaged generation/acceptance/Render, verified manual app replacement and rollback, independent review and the passing full gate (4,700 workspace tests, 1,023 UI harness tests, strict lint and doc tests). | No remaining implementation or agent-verifiable acceptance gap in the audited scope (§29.1). Limitations and owner-only verification remain explicit in the linked audits. Distribution obligations (FFmpeg source offer, GPL AI FFmpeg programs, model-weight redistribution) do not arise for this personal, undistributed app (owner decision 2026-10-05). |
| DP-24 | Measured performance budgets and diagnostics. | Partial | [Performance measurement](PERFORMANCE.md) maps every Section 25 target to a measurement. `cargo xtask perf` runs it on locked copies of real and generated media. A target row PASSes or FAILs only for a successful stage on a quiet machine with enough samples; quick, flagged or short runs are INFO. The [2026-10-04 run 3](qualification/performance-2026-10-04.md) (M5 Max, release, hashed binaries) passes the release UI replays (key-to-state 0.21 ms, navigation to picture 1.53 ms, cached edit 6.24 ms, Hold fallback 7.22 ms and 10,000-beat navigation 0.45 ms p95), 30 s real-device auditions with 0 underruns and no dropped, leading or trailing pictures at 1080p24, 1080p60 and 4K30, and warm seek on the 640×360 fixture (71 ms p95). It failed long-GOP warm seek at 1080p and 4K (265–1,862 ms p95). The [2026-10-05 seek qualification](qualification/seek-2026-10-05.md) adds threaded decode, non-reference preroll skipping and progressive preview admission; see that record for the current 1080p and 4K results. Run 3 also failed headless real-project edits as pauses accumulated (54–109 ms p95), and 10,000-beat edits (1.6–3.8 s, pauses refused after 4), because each pause retained a whole-structure timing layout. [Compact timing storage](TIMING_STORAGE.md) slices new timing tables to their referenced paths, omits provably inert reanchor steps, records granular binding patches and stores revisions as keyframes plus patches (database 63), with bit-exact PCM against the previous representation over 300 random release sequences. The [2026-10-05 record](qualification/timing-storage-2026-10-05.md) passes real and generated project edits (p95 9.8–24.9 ms, databases 7.7–9.2 MB instead of 192–194 MB) and commits all 30 pauses on 10,000 beats; those edits then failed at 340–711 ms p95 (174 MB instead of 4.8 GB), and reopening 121 revisions took 28.9 s instead of 105 s. [Incremental commits](TIMING_STORAGE.md#in-memory-head-and-validation-reuse) keep the validated head in memory, store only each commit's patch with periodic keyframes and size bounds (database 64), validate each result once while reusing unchanged binding owners' proofs, and compile the refreshed plan without revalidating; a [hash-chained history receipt](TIMING_STORAGE.md#verified-history-receipts) replaces replay on open, with `project validate --full` still recomputing everything. The [incremental commit record](qualification/incremental-commit-2026-10-05.md) brings generated-project edits to 3.9–5.1 ms p95 and pauses to 14.9–16.1 ms, and 10,000-beat edits from 372–683 ms to 28.0 ms p95 for undo (PASS), 52.6–52.8 ms for split and wrap and 163 ms for pause (FAIL); opening that 123-revision package read-only takes 0.44 s instead of 24 s. [Command work reuse](TIMING_STORAGE.md#command-work-reuse) shares one structural pass between lineage, marks and the final validation, adopts Split's own validation, reuses the head's durations and binding proof in intermediate validations, bounds the capture's binding bytes without serializing them, merges diffs and checks the inverse without copying the document, with an oracle (`with_reference_command_work`) that 24 random 40-edit sequences match exactly. The [command-work record](qualification/command-work-2026-10-05.md) (load-gated, before/after on the same machine) brings 10,000-beat split from 48–50 to 28 ms p50 (32.6 ms p95), wrap from 48–50 to 32 ms (36.8 ms) and undo to 27.9 ms p95 (all PASS), and pause from 136–145 to 81 ms p50 but 116.8 ms p95 (FAIL; its first pause binds all 10,000 Holds in 221 ms). The [2026-10-06 pause and cold-seek record](qualification/pause-and-cold-seek-2026-10-06.md) (official `cargo xtask perf --stages edit,seek`, every stage below load 3) passes all four 10,000-beat edits: split 18.8 / 24.7 ms, wrap 18.0 / 23.2 ms, undo 16.0 / 18.2 ms and pause 40.9 / 81.2 ms p50 / p95 (from 80.7 / 116.8 ms; a HEAD binary alternating at the same load measured 78–79 / 110–119 ms). A pause now builds a [provisional slice](TIMING_STORAGE.md#pause-work-scoped-to-its-changes) of its timing table that charges and byte-checks as the complete layout, derives the inserted leaf's durations from the base's, and uses hashed or merge-joined identity lookups; history rows store a changed Sequence's children as an exact splice and omit an inverse that is the forward patch's reverse ([compact history rows](TIMING_STORAGE.md#compact-history-rows); 47 MB instead of 102 MB after the run). Random-document property tests, debug assertions against the complete computations at every use and the existing command-work, structural, store and PCM-equivalence suites cover these. Picture and audio sessions adopt the store's verified Original snapshot instead of copying and hashing it again, snapshot verification overlaps BLAKE3 with the copy and its SHA-256, and interactive decoders above 1080p use 16 threads: cold first-picture completion is 125 / 149 ms at 1080p60 (from 235 / 256) and 193 / 275 ms at 4K (from 344 / 444), page-cache-warm and INFO. An isolated proxied request refines after 30 ms instead of 150 ms (scrubbing keeps 150), so the exact 4K Original picture follows 146 / 213 ms after the request (from 307 / 390); the 4K Original's own warm seek still FAILs at 88.5 / 167 ms, and 1080p60 PASSes at 30.2 / 47.7 ms. The new `deadpan-diagnostics` counters (decode queue depths, file I/O, decoded-PCM and limiter caches, GPU submission latency and live worker footprint) appear in `doctor --project` and a native `:diagnostics` panel with a replay. A release app idle with a project open used 0.65–0.67 % CPU over 60 s. Run 3's cold first picture (6.9–15.9 s, page-cache-warm) was INFO because admission decoded the whole Original first. Export runs at 0.135–2.21× real time. `Engine::diagnostics()`, `ProjectPictureSession::stats()` and `doctor --project` add underrun, decoder-cache and per-revision timing diagnostics, and the cursor boundary lookup is now O(log n). `doctor` reports actual core/SQLite probes. [Final footer release qualification](qualification/footer-layout-2026-09-27.md) passes warm navigation p95 5.20 ms, input CPU 0.70 ms, cached Repeat 7.76 ms and Hold fallback 8.59 ms against unchanged targets, after the bounded Repeat queue checks restore the Original. It uses the corrected repaint wait and actual offscreen Metal completion. [Earlier qualification](qualification/ui-feedback-2026-09-26.md) retains the 75.26/138.65 ms edit misses and 1.23 ms 10,000-beat navigation CPU result; the latter performance workload was not rerun here. [Deferred keyframes](qualification/store-and-seek-2026-10-15.md): commits at depth 64 stay patches and `write_due_keyframe` stores the head's keyframe in a separate receipt-linked transaction (hard bound 128, converted on writable open; failpoint, kill and storage tests), which the native service calls once per head revision while idle. The 10,000-beat pause p95 falls from 104.9 to 75.5 ms (load 3.25 / 2.9) and the 47 ms keyframe runs after the edit. A `cold-seek` stage reads fixtures from a freshly attached disk image (`mincore`-verified cold): 1080p60 196-204 ms p50 (max 414) and 4K 294-303 ms p50 (max 633), all at load 3-10, so INFO; a cold page cache adds only tens of ms. [10,000-beat edit latency](qualification/edit-latency-2026-10-15.md): the open writer no longer rehashes the whole history on every edit and Undo/Redo (it checks that the receipt still ends at the head with its dependencies holding, otherwise validates fully as before), and document maps and identities are shared copy-on-write with inline identity ordering. In gated `cargo xtask perf --stages edit` runs (load 2.82 to 2.99, none flagged) pause p95 fell from 71.4 / 74.3 to 40.3 / 44.7 ms, and split, wrap and Undo p95 from 41 to 48 to 18 to 25 ms, so every measured 10,000-beat ordinary edit meets the 50 ms cached-edit target (store commit plus workspace refresh, no picture). | Resident model reuse and fixed text-embedding caching (§13.5), distilled 2B/macOS MPS and MLX precision comparisons, and cold/warm inference workload distributions (§13.1/§13.4) remain open here and in Gates A/E/G; the DP-12 behavior reconciliation grants no credit for them. Run the page-cache-cold `cold-seek` stage gated on a quiet machine (the 2026-10-15 runs were all at load 3-10, so INFO) and qualify the 4K Original alone (decode-bound at 167 ms p95 with 16 threads; the stopped viewer shows the [preview proxy](qualification/proxy-2026-10-05.md) first), share one verified Original snapshot between preview, playback and the proxy builder, finish the large first-pause and native ordinary-edit budgets ([shared preparation](qualification/compound-runtime-preparation-2026-10-23.md) lowers the five-pair offscreen median to 275.697709 ms), and measure native-window playback, inference, physical display latency, memory pressure and lower tiers (To verify (owner) for the other tiers). Diagnostics now count SQLite page-cache hits, misses, writes and spills on store connections, audio-decoder input reads, the endpoint/Edit-junction picture and waveform queues, thumbnail and proxy cache hit rates, and the app's own physical footprint (which includes Metal); they still miss egui's own GPU submission (eframe has no hook), a separate Metal memory figure (wgpu's Metal backend reports none) and SQLite pages on short-lived backup/checkpoint/inspection connections, and the panel has not been observed in a native window or with a real model worker. Tiny-fixture offscreen results do not establish those budgets. |

## Delivery gates

[Specification Section 30](spec/DEADPAN_SPEC.md#30-implementation-workstreams-and-delivery-gates) defines the complete ordered build plan. Gates may have parallel work behind their interfaces; none permits calling an earlier subset the completed product.

The [first actual LTX MLX probe](qualification/model-smoke-2026-09-20.md) adds
single-file generation evidence to Gate A. It does not select a model or satisfy
the corpus, warm-performance, exact seam, color, or packaging gates.
The [supervised adapter](qualification/model-worker-2026-09-21.md) adds real
worker-boundary and exact interior-file evidence while those gates remain open.
The [lossless master probe](qualification/ffv1-2026-09-21.md) adds actual generated
frame conversion through LGPL FFmpeg, with normal/sanitizer runs, failed initial
duration metadata, rejected corrupt payloads, and retained trailer-truncation
limitations. Durable promotion and application rendering remain open.

| Gate | Status | Required work and exit evidence |
| --- | --- | --- |
| A: Qualify risky dependencies | Partial | [Native](qualification/media-2026-09-20.md) and [compatible media](qualification/media-compatible-2026-09-20.md) harnesses include actual decode/seek/audio, sanitizers, and retained frame ownership. Signed LGPL FFmpeg 8.0.3 works with pinned rsmpeg; VFR terminal duration and the mux are now qualified ([VFR recipes](qualification/vfr-recipes-2026-10-09.md)), and hardware B-frame output is refused by typed admission. The [raw DSP candidate](qualification/audio-2026-09-20.md) retains 82/605 failures. A [canonical audio prototype](qualification/audio-canonical-2026-09-20.md) passes 3,447 checks per normal/sanitized run, with 558 identical PCM hashes and failed analysis-window alternatives retained. The GPU viewport, [device output](AUDIO_OUTPUT.md), [private AI runtime packaging](PACKAGING.md#ai-runtime) and [model-pack qualification](MODEL_PACKS.md) are implemented. Still required: the full format/color matrix (high-resolution AV1/VP9 and HDR measurements), mastering and filtered-cache lifecycle, model candidate comparison (q8/BF16 MLX and the distilled 2B MPS model), full-size benchmarks and listening (To verify (owner)). |
| B: Establish the pure editing foundation | Partial | Typed time/nodes, commands/inverses, stable nested instance paths, persistent marks and edit transforms, sparse play subtrees with variable-duration indexing, atomic edits through complete occurrence paths, exact boundary/range queries, schema/history migration, headless API, indexed exact picture plans and bounded structural audio plans exist. Still required: temporal attachments, remaining semantic range/text operators, incremental fragments and full audio rendering. Representative nested edits must render with exact picture/sample selection; serialization and inverse transactions must preserve all authored meaning. |
| C: Build the interactive media workspace | Partial | Non-destructive source preview uses actual persistent decode/index and the shared SDR GPU baseline, with tested frame navigation. Managed/linked original storage is available through the headless boundary. The native workspace creates one-Original projects in Documents/Deadpan, initializes the full video, protects that undo baseline, registers separate sound media, reuses the original and preserves legacy projects. Imagegen targets guide the visible Original/Your edit layout and contextual keyboard teaching. [Proxies](PROXIES.md) and the [accessibility corpus](qualification/accessibility-2026-10-05.md) exist; physical input and VoiceOver speech are To verify (owner). Still required: full import management, mastered audio/playback, complete pane/selection/inspector workflows, full keyboard grammar and measured editorial latency without drift. |
| D: Complete the creative operation surface | Open | Every Section 8 operation and starter recipe, per-play overrides, tails, stretch/pitch, cutaways, framing, saved gags, registers, semantic macros, and shared command/help registry. Each must remain editable/portable and pass preview/export verification without no-op placeholders. The [Section 8 coverage inventory](SECTION8_COVERAGE.md) tracks each operation's status and evidence. A [preview/export verification](PREVIEW_EXPORT_VERIFICATION.md) harness (`verify-export`) now compares exported pictures and audio with the committed preview path; thirty-eight headless recipe fixtures (ranged gain steps, `:gag-set` on One More Time, a bounce micro-loop with marked play seams, the synthesized triumphant sting, saturation, pitch shift, J-cut, L-cut, seeded One More Time, role-only deletes, audio-only and video-only repeats, repeat with gap, One More Time, Nothing Happens and Are We Done? through the headless semantic path, the recorded `:repeat 3 gap= gain-step= zoom-step=`, reverse hiccup, ping-pong, hanging reverb tail, echo tail, delayed captions over Original and black pictures, bleep, lift, audio lag, bed drop, mute range, off-center stare, freeze and black pauses, 50% Preserve retime, zoom/creep/follow framing, cutaway, escalating repeat, exploded repeat with an override, exploded escalation, duplicated range, gain trim, placed sound) pass it after public Render ([qualification](qualification/preview-export-2026-10-04.md)), recorded in the coverage inventory's Export verified column. Broader nested Generated cases, overrides and many rows remain without export comparison, and native key paths are not exercised by it. The [inventory](SECTION8_COVERAGE.md) counts 85 implemented, 10 partial and 0 missing rows; no row is complete for Gate D. Remaining partial rows: Escalation speed progression, `audio-shift`, role-repeat `extend=hold`/fit policies, per-play timing edits, effect sends, variable-rate stretch, framing point/region targets, general occurrence macros and target-sampled ranged-zoom dot coverage, and the shared command/help registry. |
| E: Add analysis and real AI holds | Open | Local analysis and correction, tracking, runtime/model manager, generation planning/validation, audition/acceptance, stale-job handling, and caching. Actual qualified local generations must meet duration/seam contracts; accepted projects must render offline without the model. Publish latency and quality measurements. |
| F: Complete import, export, and distribution | Partial | The [relocatable bundle](PACKAGING.md) ships the pinned helpers, relocated FFmpeg, workers, notices and an SBOM, with ad hoc hardened-runtime signing, and passes a scrubbed relocated import/render check. Bundled single-video YouTube import (DP-14), Original provenance and full-source initialization (DP-01, DP-15), audio-only effect import, [signed safe updates](UPDATES.md) and runtimes (DP-22, ad hoc signing by owner decision), notices and SBOM ([release audit](RELEASE_AUDIT.md)), [rotating backups and release migration](BACKUPS.md) are implemented. Remaining: the HDR/SDR and codec gaps tracked under DP-17, the keyboard-only source-URL-to-MP4 workflow verified from the distribution, and clean-machine acceptance (To verify (owner)). [Recovery](RECOVERY.md) adds unclean-exit detection and reopen, interrupted-job reporting with explicit retries, missing-Original relink and real disk-full/read-only/permission failure tests with truthful native messaging. Complete the keyboard-only source-URL-to-MP4 workflow from the distribution without external setup. |
| G: Release qualification | Open | Run every requirement, crash/chaos/malicious-input suite, long-project stress, preview/export comparisons, clean-machine online/offline installation, accessibility, and performance measurements. Deliver app, approved packs, documentation, fixture/benchmark reports, SBOM/notices, and migration policy; explicitly report any deviation. The reproducible benchmark suite and its first [performance report](qualification/performance-2026-10-04.md) exist; its seek results are superseded by the [2026-10-06 record](qualification/pause-and-cold-seek-2026-10-06.md), where only the 4K Original's own warm seek still fails (167 ms p95; the stopped viewer shows the proxy first); its real- and large-project edit and pause-history storage deviations remain open (see DP-24), and lower-memory tiers, display latency, idle CPU and inference stress are unmeasured. The [adversarial suite](ADVERSARIAL.md) provides the crash/chaos/malicious-input suite (deterministic regression in the normal tests, `cargo xtask chaos` campaigns with ASan/UBSan for the source adapters) and a synthetic long-project stress (`--stress`: 10,000 beats over a two-hour Original, hundreds of revisions, reopen, full replay validation and render-plan range queries within budgets). Its [first run](qualification/adversarial-2026-10-05.md) found no crash; the one integrity finding (register slot rows outside tamper detection) is fixed. Coverage-guided fuzzing exists (27 libFuzzer targets, `cargo xtask fuzz`, one bounded campaign in the [release audit](RELEASE_AUDIT.md)); hostile worker protocols are covered for render, encoding, verification, tracking, faces, transcription and AI. [Native adapter sanitizers](qualification/sanitizers-2026-10-15.md) (`cargo xtask sanitizers`) now run the track, transcribe, output, process, filesystem, file-clone and SQLite-status adapters under ASan/UBSan and process, output and transcribe under TSan, three clean runs each; Apple frameworks and the pinned FFmpeg stay uninstrumented. Still open: sustained fuzzing campaigns with corpus merges, hostile media inside the real encoder, Vision, whisper.cpp and MLX workers, stress on real long media with export, and release-candidate reruns of the whole suite. |

## To verify (owner)

Per [spec §29.1](spec/DEADPAN_SPEC.md#291-verification-scope), these checks need
something the implementing agent cannot exercise on this machine. They are
recorded here and do not block completion. Each row names what was not
exercised; the feature itself is implemented and verified as far as possible.

| Area | What remains for the owner | Related |
| --- | --- | --- |
| Word timing proposals: phonetic judgment and physical native delivery | 1. Extract the [portable synthesized-speech fixture](../tools/model-qualification/evidence/2026-10-25-word-refinement/workflow/Source.deadpan.tar.gz), open it with the qualified English pack installed, run :correct and select Copper. Tab to Refine word timing locally and press Enter. The review must show Approximate / Unsaved, old 0.21..0.52 s and proposed 0.16..0.68 s with the measured waveform. 2. Press Space to hear the fixed comparison context at a comfortable monitor level. Judge actual phoneme onset/offset, inspect both colored edge pairs and try manual adjustment. At 960x640, Hear and Accept must remain visible. Check physical Tab/Enter/Space/Escape and VoiceOver reading order; modified Command-Return must not accept. 3. Accept only if useful, then use one correction Undo: Copper returns to its prior edges, roofs remains pinned at 2.53..2.73 s, both editor clocks/picture and the authored revision stay unchanged. 4. Repeat on rights-cleared own English speech, including rapid/repeated words and noise, and record offered/refused/accepted edges plus missing/extra phonemes. The 24 observed development cases do not establish conversational accuracy; stricter retention/foreign-energy targets remain failed. Local injected input, numerical parity and real device delivery are verified; human hearing, physical delivery and VoiceOver speech are not. | [Word refinement qualification](qualification/word-refinement-2026-10-25.md), DP-10, DP-20 |
| Camera target physical pointer delivery | 1. Open a disposable Original project, set the window to 960×640, press `,f` then `n`, and use a physical mouse or trackpad to drag a rectangle on the visible picture. Drag its center and each corner, including across the opposite corner; a small rectangle's center must move without changing its size. 2. Hold a drag, press an arrow, then keep moving the pointer: the keyboard edit must remain. Repeat while resizing the window and after switching away from the app. 3. Release: the target remains unsaved. Escape must discard it. Reopen `,f`, `n`, press `n` again for a point, drag and press Enter: one target is saved. Press `c`, move it and Enter; Undo twice must restore and then remove it. Local injected input, Metal captures and durable history are qualified; physical event delivery and display presentation remain unexercised. | [Pointer qualification](qualification/pointer-targets-2026-10-25.md), DP-08, DP-20 |
| Selected-play Hold lengths through one fixed Retime: physical keys, speech and listening | 1. Extract each 91f unit-growth archive into its own folder: [inner selected under All plays](../tools/media-qualification/evidence/2026-10-22-selected-fixed-retime-hold-duration/inner-default-unit-grow.deadpan.tar.gz), [inner selected under This play](../tools/media-qualification/evidence/2026-10-22-selected-fixed-retime-hold-duration/inner-play-unit-grow.deadpan.tar.gz), [outer selected with inner All plays](../tools/media-qualification/evidence/2026-10-22-selected-fixed-retime-hold-duration/outer-default-unit-grow.deadpan.tar.gz), and [outer selected with inner This play](../tools/media-qualification/evidence/2026-10-22-selected-fixed-retime-hold-duration/outer-play-unit-grow.deadpan.tar.gz). 2. Open each package and enter its outer Repeat. For the inner-selected cases choose outer `:scope all` or `:scope play 2`, then inner `:scope plays 1,2`. For outer-selected cases choose outer `:scope plays 1,2`, then inner `:scope all` or `:scope play 2`. Enter the fixed stage/body and select the frozen Hold. Physical Enter must prefill `hold-duration 5f`, with both scopes and the selected-play explanation readable. Set `3f`: output stays 91f, the selected families change and all other Holds keep 3f. One Undo restores the selected 5f Holds; Redo restores 3f. Check physical Enter/Escape, focus, breadcrumb and VoiceOver speech, and exercise recording/Run/dot after explicitly reselecting the pair. 3. Listen across changed and unchanged families and compare exports. Also inspect the [73f affine fixture](../tools/media-qualification/evidence/2026-10-22-selected-fixed-retime-hold-duration/inner-default-affine-grow.deadpan.tar.gz) and [its encoded file](../tools/media-qualification/evidence/2026-10-22-selected-fixed-retime-hold-duration/encoded-inner-default-affine-grow.mp4), especially 2.20 to 2.35 seconds around the retained 12.008dB local-block SNR minimum. Local routing, exact PCM, history and emitted files are qualified; physical delivery, speech and acoustic judgement remain unverified. | [Selected fixed-Retime qualification](qualification/selected-fixed-retime-hold-duration-2026-10-22.md), DP-06, DP-07, DP-20 |
| Mixed All plays/This play Hold lengths through one fixed Retime: physical input, speech and listening | 1. Extract the [Default/Play unit fixture](../tools/media-qualification/evidence/2026-10-22-mixed-fixed-retime-hold-duration/mixed-fixed-default-play-unit-grow.deadpan.tar.gz) and [Play/Default unit fixture](../tools/media-qualification/evidence/2026-10-22-mixed-fixed-retime-hold-duration/mixed-fixed-play-default-unit-grow.deadpan.tar.gz) into writable folders. Both validated packages last 91 frames and contain outer and inner three-play Repeats. In the first, choose outer `:scope all` and inner `:scope play 2`; in the second, choose outer `:scope play 2` and inner `:scope all`. Enter the fixed stage/body and select the Freeze Hold. Physical Enter must prefill `hold-duration 5f`, and both scope choices and the mixed explanation must be readable. 2. Set each captured Hold to `3f`: output stays 91 frames, only its three-use family changes, and the other six Holds keep 3f. Undo restores that family's 5f Holds; Redo restores 3f. Exercise recording, saved Run and dot after explicitly reselecting the Hold. Check physical Enter/Escape, focus, breadcrumbs and VoiceOver speech. 3. Listen across both changed families and unchanged speech, and compare exports. Also inspect the [73f affine fixture](../tools/media-qualification/evidence/2026-10-22-mixed-fixed-retime-hold-duration/mixed-fixed-default-play-affine-grow.deadpan.tar.gz) and its [actual encoded file](../tools/media-qualification/evidence/2026-10-22-mixed-fixed-retime-hold-duration/encoded-mixed-fixed-default-play-affine-grow.mp4), especially 2.20 to 2.35 seconds around the retained 12.008dB local-block SNR minimum. Local routing, exact PCM, history and emitted files are qualified; physical delivery, speech and acoustic judgement remain unverified. | [Mixed fixed-Retime qualification](qualification/mixed-fixed-retime-hold-duration-2026-10-22.md), DP-06, DP-07, DP-20 |
| Nested This play Hold lengths through one fixed Retime: physical input, speech and listening | 1. Extract the [retained owner fixture](../tools/media-qualification/evidence/2026-10-22-nested-fixed-retime-hold-duration/nested-fixed-unit-grow.deadpan.tar.gz) into a writable folder and open the validated 91-frame package. Enter its fixture Sequence, select the outer three-play Repeat and choose `:scope play 2`. Enter the inner three-play Repeat and choose `:scope play 2` again, then enter the fixed stage/body and select the Freeze Hold. Physical Enter must prefill `hold-duration 5f`; both play choices and This play explanation must be readable. 2. Set it to `3f`: output stays 91 frames, and all eight other Holds keep their original 3f lengths. Undo restores only the middle/middle 5f Hold; Redo restores 3f. 3. Record and save a duration, run it, then explicitly reselect the same nested Hold before dot. Check physical Enter/Escape, focus, breadcrumb and VoiceOver speech. Listen across the selected stage and unchanged families, and compare an export. Scripted routing, exact PCM, durable history and emitted files are qualified; physical delivery, speech and acoustic judgement remain unverified. | [Nested fixed-Retime qualification](qualification/nested-fixed-retime-hold-duration-2026-10-22.md), DP-06, DP-07, DP-20 |
| This play Hold length through one fixed Retime: physical keys, speech and listening | 1. Extract the [retained owner fixture](../tools/media-qualification/evidence/2026-10-22-this-play-fixed-retime-hold-duration/this-play-fixed-unit-grow.deadpan.tar.gz) into a writable folder and open its 43-frame package. Open the three-play Repeat, choose `:scope play 2`, enter its fixed stage and body, then select the frozen Hold. Physical Enter must prefill `hold-duration 5f`, and the inspector must explain This play. 2. Set it to `3f`: total output stays 43 frames, and the first and third Plays keep their original 3f Holds. One Undo restores only the middle 5f Hold; Redo restores 3f. 3. Record a duration, save and run it, then explicitly reselect the Hold before dot. Check physical Enter/Escape, focus, breadcrumb and VoiceOver speech. Listen across the selected fixed stage and unchanged surrounding Plays, and compare the export with Original audio. Local scripted routing, exact PCM, history and emitted files are qualified; physical delivery, speech and acoustic judgement remain unverified. | [This play fixed-Retime qualification](qualification/this-play-fixed-retime-hold-duration-2026-10-22.md), DP-06, DP-07, DP-20 |
| All plays Hold lengths through fixed Retimes: physical keys, speech and listening | 1. Copy the retained `shared-fixed-retime-unit-grow.deadpan` package identified in the qualification's encoded identities. It lasts 43 frames. Open the shared default fixed stage inside its three-play Repeat, select the frozen Hold and use physical Enter: it must prefill `hold-duration 5f`. 2. Set it to `3f`. Output stays 43 frames and the independent middle play keeps its own 3f Hold. One Undo restores both default 5f Holds; Redo restores their 3f lengths. 3. Record a duration, save it, run the named program and use dot after explicitly reselecting the Hold. Check focus, breadcrumb and VoiceOver speech. Listen across the fixed stage edges and compare the export with Original audio. Local routing, exact PCM, history and emitted files are qualified separately; physical delivery, speech and acoustic judgement remain unverified. | [Fixed-Retime shared duration qualification](qualification/shared-fixed-retime-hold-duration-2026-10-22.md), DP-06, DP-07, DP-20 |
| Several selected Hold durations: physical input and listening | 1. In a disposable speech project, make one 2-frame Hold, wrap it with `:wrap-repeat 3`, then wrap that Repeat with `:wrap-repeat 2`. This outer Repeat lasts 12 frames. Enter both levels with All plays at the outer level, choose `:scope plays 1,3` at the inner level and select the Hold. Physical Enter must prefill `hold-duration 2f`. Set it to 3f: the outer Repeat becomes 16 frames, while each middle play remains 2f. One Undo restores 12; Redo restores 16. 2. Choose outer `:scope play 2`, retain inner `:scope plays 1,3` and set those Holds to 1f. Only that outer play changes, leaving the first outer play's selected Holds at 3f and both middle plays at 2f; the outer Repeat becomes 12 frames. Undo restores 16 and Redo restores 12. 3. Exercise physical Enter/Escape, recording, a saved duration Call and dot on a newly selected pair. Check the selection labels and VoiceOver speech, then listen to RoomTone and surviving speech and compare a render. Local scripted routing, clocks and encoded verification are recorded separately; physical input, speech and acoustic judgement remain unverified. | [Selected duration qualification](qualification/selected-hold-duration-semantics-2026-10-21.md), DP-06, DP-07, DP-20 |
| Mixed All plays/This play Hold duration: physical input and listening | 1. In a disposable speech project, make one 2-frame Hold, wrap it with `:wrap-repeat 2`, then wrap the Repeat with `:wrap-repeat 3`. The outer Repeat lasts 12 frames. Enter the outer Repeat and choose `:scope play 2`; enter the inner Repeat and choose `:scope all`. Enter on the Hold must prefill `hold-duration 2f`. Set it to 3f: only the two Holds in outer play 2 change, so the outer Repeat lasts 14 frames. 2. Return to the outer level, choose `:scope all`, then enter the inner level and choose `:scope play 1`. Set its still-shared 2f Hold to 4f: the outer Repeat becomes 18 frames, while both independent 3f Holds in outer play 2 remain unchanged. One Undo must restore the exact 14-frame state; Redo restores 18. 3. Check physical Enter/Escape, recording, named duration Call and direct-edit dot in both mixed directions, including VoiceOver speech. Listen to the qualified Hold audio and unchanged speech after the Repeat, and compare a render. Scripted routing and local media checks are recorded separately; physical input, speech and acoustic judgement remain unverified. | [Mixed duration qualification](qualification/mixed-hold-duration-semantics-2026-10-21.md), DP-06, DP-07, DP-20 |
| All plays Hold duration: physical input, speech and listening | 1. In a disposable speech project, insert a 2-frame Hold, wrap that Hold with `:wrap-repeat 2`, then wrap the Repeat with `:wrap-repeat 3`. Enter each level and choose `:scope all`. Enter on the Hold must prefill `hold-duration 2f`; record a change to 4f and save. The edit must grow by 12 frames; one Undo restores all six 2-frame uses. 2. Choose This play at both levels and change one Hold to 6f. Return to All plays at both levels and change the literal 2f default to 4f. Only the four uses in the two still-shared outer plays must grow, by 8 frames total; the owned outer subtree stays unchanged. 3. Give each remaining outer play its own Hold change using This play at both levels, then return to All plays at both levels. The unused default must show no active picture; changing its duration must preserve both clocks, total duration and displayed image. 4. Check physical Enter, Escape, recording, named Call and direct-edit dot with VoiceOver speech, then listen and compare a render across the unchanged speech after the Repeat. Scripted routing, exact clocks, real A/V export and offscreen presentation are covered by the qualification. | [Shared duration qualification](qualification/shared-hold-duration-semantics-2026-10-21.md), DP-06, DP-07, DP-20 |
| Speed after an exact pause: physical input and listening | 1. In a disposable project, select a 12-frame speech beat and run `:retime 0.5 pitch=tape`. Move three frames into that stage and run `:hold 3f`. The stage must now last 27 frames. 2. Select the same outer stage, open Change speed with the physical keyboard, replace the entry with `retime 7/8 pitch=tape`, and apply. It must last 15 frames; its inspector must show and pre-fill `9/10`. The Original cursor must stay unchanged. 3. Listen across the scaled freeze and resumed speech, then Undo and Redo once and compare the rendered result. Check VoiceOver feedback for the exact speed and refused processing pitch. Injected input, PCM, history and emitted files are covered in [qualification](qualification/clocked-speed-update-2026-10-20.md); physical delivery, speech and listening remain unverified. | DP-07, DP-09, DP-20 |
| AI generation during physical sleep | 1. In a disposable project with two unaccepted pauses and an installed AI pack, select the first pause and run `:generate`; wait for inference steps in `:jobs`. Start generation for the second pause so it queues. 2. Put the Mac to sleep from the Apple menu, then wake it. Both affected jobs must stop as Interrupted, retain explicit Retry actions, and never restart automatically. The edit revision and accepted providers must stay unchanged. 3. Select one interrupted row and press R; a fresh attempt must start after input validation. Separately cancel a generation before sleeping; it must remain Cancelled rather than reappear for retry. 4. Repeat during candidate validation/publication: a result admitted before the final publication guard may remain Ready; otherwise it must be Interrupted with no new Ready candidate. Injected lifecycle, durable retry and publication boundaries are covered in [qualification](qualification/generation-sleep-2026-10-17.md); physical notification delivery and real inference teardown are not. | DP-12, DP-18 |
| Transcription during physical sleep | 1. Open a disposable Original project with the Whisper pack installed and wait for recognition progress. Put the Mac to sleep, then wake it. Recognition must drain and show an interruption with Try again, without automatically restarting or publishing a partial result. 2. Activate Try again with keyboard focus and Enter; a new run must complete. 3. With a saved transcript and a manual word correction, start Recognize again, sleep/wake during progress, and verify the prior words/correction remain available and the interrupted replacement is not saved. 4. Cancel recognition before sleeping; cancellation must remain the outcome. [Injected lifecycle qualification](qualification/transcription-sleep-2026-10-17.md) covers the real worker and transactional boundaries, not physical notification delivery. | DP-10, DP-18 |
| Standalone pause detection during physical sleep | 1. Open a disposable Original project with a stored transcript but no speech activity and the approved detector installed. Wait for Detecting pauses, then sleep and wake the Mac. The detector must drain, show interruption and never restart automatically; stored words, corrections and the edit revision must stay unchanged. 2. Focus Try again and press Enter after drain; detection must start afresh and publish pause proposals. 3. Repeat with detection cancelled before sleeping; it must retain the cancellation outcome. [Qualification](qualification/activity-sleep-2026-10-17.md) covers injected lifecycle and a real loaded detector, not physical notifications. | DP-10, DP-18 |
| Nested speed pauses: physical input and listening | 1. In a disposable project, select a 120-frame speech beat in Your edit, run `:retime 0.5 pitch=tape`, then `:wrap-retime 1.5 pitch=tape`. Navigate 17 frames into the beat and run `:hold 3f` with the physical keyboard. The edit must grow by exactly 3 frames and freeze the picture immediately before the insertion. 2. Listen across the pause: it must contain silence, then resume speech without changing either stage's tape speed. Undo once and Redo once; the same cut and pause must return. 3. Run `:hold 1f video=black` later in the same beat, audition and render; compare the audible boundaries. Injected commands, exact PCM, history and emitted-file checks are covered in [qualification](qualification/nested-speed-pauses-2026-10-18.md); physical delivery, speech announcements and listening remain unverified. | DP-07, DP-09, DP-20 |
| One-play Hold sound: physical input, speech and listening | 1. In a disposable project with audible Original audio and a Hold inside two nested Repeats, copy a quiet Original moment with `v`/`y`. Return to Your edit and choose `:scope play 1` at each Repeat level. 2. Type `:record b`, then `:room-tone`. Edit both integer sample endpoints, choose Prepare range and Apply room tone, then `:record-stop`. Confirm the Edit cursor, picture and total duration stay unchanged. 3. Choose a different explicit nested play and type `@b`; choose another and press `.`. Only those Holds should use the same exact samples. Use `:hold-silence` and one Undo to check both policies. 4. Listen across every play and the rendered result, and use VoiceOver to check the recording, scope and Apply feedback. [Qualification](qualification/scoped-hold-audio-2026-10-20.md) covers injected controls and canonical PCM; physical input, speech and acoustic judgment remain unverified. | DP-06, DP-07, DP-09, DP-20 |
| Mixed Repeat pauses: physical input and listening | 1. In a disposable speech project, wrap the selected Source with `:wrap-repeat 2`, then wrap the selected Repeat with `:wrap-repeat 3`. Enter the outer contents, choose `:scope all`, enter the inner contents and choose `:scope play 2`. Move three frames into the Source and run `:hold 3f` physically. Duration must grow by 9 frames, with the preceding picture frozen only in inner play 2 of each default outer use. 2. Listen across all six source plays; each affected cut must add silence and resume the same speech. Undo once. 3. At the inner level choose `:scope plays 1,2`, then `:hold 3f video=black`; all six uses must grow by 3 frames each, with one Undo restoring the complete batch. Compare the rendered result. [Qualification](qualification/mixed-repeat-pauses-2026-10-20.md) covers injected input, exact PCM, history and emitted files; physical delivery, speech announcements and listening remain unverified. | DP-07, DP-09, DP-20 |
| Native EQ listening and physical input | 1. Open a disposable project with speech and a placed sound, select a beat in Your edit and enter `:eq`. Using the physical keyboard, Tab through all five fields, enter bass `3`, middle `-6`, treble `1.5`, low cut `80`, high cut `12000`, and activate Set EQ. Enter in a field must not save. 2. Compare Before/Draft through headphones or speakers; both must use the same loop and timing, with only the authored tone/filter changes. Tab must reveal every focused control at 960×640. 3. Apply once, listen and render; Undo must restore the prior sound and recipe. Reopen EQ, change a band and Escape; the saved recipe must remain unchanged. Offscreen focus/paint, immutable proposals and shared numerical/export audio are already verified. | DP-09, DP-16, DP-20 |
| Idle audio power policy | 1. Open a project with a Preserve retime, leave playback stopped and press `l`; wait for the audio preparation status. Neither editor clock may advance further and no sound may start. 2. On a Mac with a battery, unplug power and press `l` again; after the current cooperative read/check yields, expect **Audio cache deferred** with the battery reason on hover. Repeat with Low Power Mode enabled through System Settings and expect that reason. 3. Press Space in each restricted state; explicit Play must still work. Pause, restore AC/normal power, move with `l` and confirm optional preparation can complete again. Simulated policy and cancellation pass in the [qualification](qualification/audio-warming-2026-10-11.md); physical power transitions remain unexercised. | DP-09, DP-16, DP-24 |
| Clean machine | Install the bundle on a Mac with no Homebrew, Python, FFmpeg, yt-dlp, Deno, Xcode tools or model cache; import local and YouTube media, install the model pack in the app, generate and accept a pause, restart offline and render (spec §26.6). The agent approximates this with a relocated bundle in a scrubbed environment. | DP-13, DP-14, DP-22, Gate F, Gate G |
| Whole-app disconnection | Disconnect network interfaces before the full offline distribution's first launch with empty model storage. Install both archives through Models, import a local video, generate and accept a pause, quit, reopen and render. The agent verified initial launch, whisper installation, reopen and render under IP denial, and AI installation/generation under the worker's own denial; macOS rejects nesting the worker sandbox inside the whole-app test sandbox ([evidence](qualification/native-install-2026-10-06.md#network-denial-limits)). | DP-13, DP-22, Gate F |
| Physical download interruption | Begin the 36.15 GB AI pack download in Models, disconnect the network after partial progress, reconnect and resume. Confirm retained progress, complete hash validation and the smoke test before selection. The real HTTPS transport is covered by bounded server disconnect/Cancel/resume tests; a physical interruption of the full pack remains unexercised. | DP-13 |
| Portable AI project on another Mac | Use File › Save Portable Copy… on a project with an accepted AI pause. Move the copy to a second Mac with no model packs, disconnect networking, open it, Undo/Redo the acceptance and render. Confirm both Original and generated pictures survive and the movie is verified and published. This Mac passed the equivalent copy, source-removal and denied-runtime workflow ([evidence](qualification/accepted-portability-2026-10-07.md)). | DP-19 |
| File Provider eviction | Put a disposable portable project in a configured File Provider folder and finish syncing. Close Deadpan, evict the local media through that provider, disconnect networking and reopen. Confirm unavailable media reports a recoverable failure without changing the Original or accepted provider. Reconnect, materialize the files, reopen and render; hashes must verify and history must survive. No real provider-eviction fixture was exercised. | DP-01, DP-15, DP-19 |
| Power loss during copy or cleanup | Use disposable copies of a project with an accepted AI pause and an unreferenced candidate. Interrupt physical power during Save Portable Copy, then separately during confirmed Storage cleanup. After restart, a published copy must pass `project validate` and render; an interrupted copy may leave only hidden staging. Cleanup must leave each object whole or absent and retain every accepted/history dependency. Process and filesystem fault tests cover the software boundaries; physical durability remains unexercised. | DP-01, DP-19 |
| Power loss during saves and database recovery | On a disposable project, record its current revision and backup ID with `project dump <p> --json` and `project backups <p>`. In separate trials, interrupt physical power during (1) a split save, (2) Storage's Back up now, (3) confirmed backup restore, and (4) migration of an eligible older package. After restart, run `project validate <p>` and reopen. A saved revision must remain; an uncommitted split may be absent; published backups must verify with `project backups <p> --verify`; restore must leave either a valid original/replacement or an explicit recovery path retaining the old files. Migration must leave the old database or a fully validated new one plus its pre-migration backup. Repeat recovery using the retained backup and confirm its exact revision. Process-kill tests cover these software boundaries, not physical durability. | DP-01, Gate F, Gate G |
| Hardware tiers | 1. Run `cargo xtask perf` on a 16–24 GB and a 32–64 GB Apple Silicon Mac. 2. On the smaller Mac with the approved 21 GiB Extension pack installed, create an Original project with an eligible pause, run `:generate`, then `:jobs`. The job must wait with the model estimate and OS/editor reserve, offer Cancel without Run anyway, and remain held if O is pressed. Cancel it and verify no candidate was published and editing still works. 3. With sufficient headroom for the installed Whisper estimate plus reserve, transcribe the Original and verify normal completion. Injected capacity, writer and cancellation tests are recorded in [memory qualification](qualification/memory-admission-2026-10-17.md); these physical tiers remain untested. | DP-18, DP-24, Gate G |
| Physical input | Physical keyboard delivery on non-US layouts, dead keys, input-source switching and a real OS input method (CJK) in command, caption, URL and correction fields, including the `[`/`]` pause motions on QWERTZ and AZERTY keyboards (the layouts replay simulates them). Replays inject events only. | DP-05, DP-20 |
| Native YouTube confirmation | In the packaged app, press ⌘⇧N, paste `https://youtu.be/Z4C82eyhwgU`, then Enter. Listen with VoiceOver: the title and image name must identify Caminandes 2, the rights notice and both actions must be reachable, and no new project exists before confirmation. At 960×640, Tab to Download and create and press Enter; the finished project must have one full-length Original. Repeat over an open project and press Escape at confirmation; the existing revision and cursor must remain. For a video requiring authentication, choose an authorized cookies file through the native picker, confirm its path is used only for that import, and verify cancellation removes temporary copies. Injected events, accessible labels, actual paint and the real public-video job are verified; speech, physical input and an authenticated account are not. | DP-05, DP-14, DP-20 |
| Downloader update on a second Mac | Install the ad hoc signed bundle on a second Mac under quarantine. In Models, choose Apply signed update…, select an approved downloader manifest signed by a currently trusted release key, review its versions and apply. Run the installed bundle's `deadpan-cli downloader status --probe`: the active source must be the managed update and its versions must match the manifest. Import `https://youtu.be/Z4C82eyhwgU`. Choose Use the baseline downloader, probe again and confirm bundled yt-dlp 2026.08.19/Deno 2.9.7. `codesign --verify --deep --strict /Applications/Deadpan.app` must pass before and after. Same-Mac signed update/rollback is already verified; this row covers the second-Mac/quarantine environment. | DP-14 |
| Source interpretation on a physical display | Open a disposable project with a known limited/full-range tagging error. Use Original's Interpret picture… control, enter the known correct `range=full` or `range=limited`, and compare against a trusted reference on the physical display. Check Undo, Redo and `:source-interpret reset`; the displayed notice must follow the displayed picture, and the Original/Edit clocks and selection must stay unchanged. Numerical pixels, offscreen controls and export are verified in the [qualification](qualification/source-interpretation-2026-10-11.md); physical appearance and real keyboard delivery remain unexercised. | DP-02, DP-16, DP-20 |
| Dock Quit and logout | On a new disposable project, enter Camera with `,f`, press `l`, then choose Quit from Deadpan's Dock menu. Return or Escape must keep the X=51 draft and its focus. Repeat Quit, use Tab to select Discard and Enter, reopen and confirm the saved revision is unchanged. Separately request macOS logout with a draft open: Keep editing must cancel logout and retain the draft; after an explicit discard, logout may complete and the next launch must retain the saved project. Native Cmd-Q exercises the same AppKit termination handler; the computer-use tool cannot read the Dock, and logging out the owner's desktop requires an owner trial. | DP-01, DP-05, DP-20 |
| Palette forms: physical input and speech | In the native app, split a beat, open `?`, press `/`, type `:undo`, and press Enter; repeat with `:redo` and confirm the split returns. Search `frame`; use Up/Down and VoiceOver to distinguish Previous frame from Next frame, then activate each and confirm the Edit cursor moves one frame in its named direction. Click each row once. Search `apply trim` outside Trim and confirm the spoken explanation keeps Help open. Repeat with the configured keyboard layout. Injected events, painted rows and durable history are verified; physical delivery, native pointer hit-testing and speech are not. | DP-06, DP-20 |
| Original macro physical input and speech | In a disposable project, finish a backward Visual selection in Your edit, switch to Original and navigate to Original boundary 30. Type `qo`, `mv`, `5l`, `'v`, then `q`. Confirm the recording hint names Original, the jump returns to boundary 30, Ctrl O reaches 35 and Ctrl I returns to 30. Move to boundary 40 and type `@o`; the mark must now bind boundary 40 while the Edit cursor, beat and backward range remain unchanged. Repeat with the configured non-US input source and listen to the recording and mark announcements with VoiceOver. Injected events and exact measured PTS are covered; physical delivery and speech remain unverified. | DP-06, DP-20 |
| VoiceOver | Listen to VoiceOver on the start screen, workspace panes, sheets and announcements; Voice Control and Full Keyboard Access. The agent verifies the accessibility tree and announcement requests. | DP-20 |
| Listening | Audition and export listening: playback through real devices, AI pause audition, tails, saturation, pitch shift, limiter and room tone. | DP-09, DP-12, DP-16 |
| Continuous owner-return listening | 1. Open a disposable project with a sharp spoken or percussive event, select its beat in Your edit and enter `:wet-send delay -6dB`. Play through headphones or speakers: the first echo should follow by 300 ms, with later echoes decaying and no added picture time. 2. Enter `:wet-send delay off`, then `:wet-send reverb -6dB`; compare the dry and reverberant versions using Undo/Redo, including playback started in the middle. 3. Insert a silent Hold inside the treated group and confirm no Original return sounds during it; use `:tail 1s effect=delay input=original` at a boundary and confirm that pause does not acquire an extra incoming reverb. Render and listen to the same intervals. Numerical PCM, offscreen controls and encoded-file verification do not establish listening quality or device behavior. | DP-09, DP-16 |
| Real-person footage | Tracking and face proposals on rights-cleared real-person footage, AI pause quality and identity review over the §13.4 corpus. | DP-11, DP-12 |
| AI rejection threshold calibration | On the §13.4 rights-cleared corpus, generate 0.5, 1, 2 and 3-second Holds using Still, Subtle and Moderate. Read retained motion-pair coverage, endpoint crop/differences, face geometry, continuous mouth-segment coverage, selected-region coverage and any rejected frame/value, then audition each offered candidate through both joins. Include faces entering late, occlusion, multiple faces, turning heads and steady open mouths. Select a saved non-face target with `target=ID`; include stationary and moving subjects, endpoint mismatch, small/off-source regions and tracking confidence loss. Confirm unavailable coverage stays explicit and an earlier rejection survives loss. Record false rejections, missed flashes/motion, broad join changes, geometry drift and mouth movement against `deadpan-motion-lighting-1`, `deadpan-endpoints-1`, `deadpan-generated-face-geometry-mouth-1` and `deadpan-generated-region-1`; unavailable measurements do not count as successful checks. | DP-12 |
| Physical events | Power loss during commits, cleanup, copies and publication; sleep/wake and device changes during playback; physical display latency and EDR/HDR display appearance. | DP-01, DP-16, DP-17, DP-24 |
| External media | Choose rights-cleared camera/phone HDR files, Dolby Vision/HDR10+ samples and other owner source media. Create a fresh managed project from each, compare the complete retained file hash with the source, reopen, inspect the first/last pictures and source interpretation, then Render and run `verify-export --every 1` with audio windows across the file. Record supported results or the exact explained refusal; assess the image and sound on the owner's display/speakers. Repeat one linked import after moving its external file and verify identical-content recovery without an authored change. | DP-15, DP-17 |

## Updating evidence

For each completed slice, link the implementation and named tests plus an acceptance report containing the revision, fixture/input, command or interaction, expected/observed result, and environment. Include hardware/OS, dependency/runtime versions, power/cache state, latency distribution, and failed samples where relevant. Preserve unfulfilled behavior explicitly.

Before marking any creative operation complete, establish that it is editable, undoable, serializable, keyboard-accessible, previewable, and exportable. A passing test double proves only its tested boundary. A claimed release requires actual media, actual local generations, verified emitted files, and the distributed clean-machine workflow.
