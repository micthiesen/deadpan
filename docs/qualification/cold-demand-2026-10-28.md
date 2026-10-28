# Shared owner-phase demand driver, 2026-10-28 (UTC)

Both numerical engines now execute one shared owner-phase body. A typed cold
consumer uses the same EQ/member and wet/member/sample/effect sequence, emits
borrowed demand metadata, and never invokes a numerical callback. The cold
entry remains unconnected to recursive resource admission and batching.

## Authority and behavior

Constant execution keeps owner/stage/sample order; dynamic execution keeps
sample/owner/stage order. Numeric gain flushes, finite checks, initial/tail sends,
output sums, masks, first-clock clone scopes and synchronous error barriers stay
in place. Constant wet transfers retain their original lexical Program lifetime.
An indexed checkpoint replaces the old per-sample iterator at the same point
after callback success and before transfer application.

Cold dispatch returns no PCM. It borrows route clocks, uses the actual completed
policy rows and shares the constant-layer predicate. Inactive dynamic routes
cannot switch otherwise-constant execution. Permission, clock, Tail, stage,
member, effect, word and owner-order visits charge actual reached inspections,
including short-circuit and empty-traversal cases. Owned numerical clocks keep
their old clone points; cold constant lookup compares the full existing owner
key without allocating a clone. That key identifies a contribution group and
does not prove retained raw-context identity.

Only `populate_completed` can create the completion capability, after the same
real `policy_prediction::populate` succeeds and allocated shape validates.
Allocation or all-false storage alone grants nothing. The witness immutably
borrows its exact Components, route storage, window and flags. Its closed outer
adapter obtains owners from that same object. The caller admits the complete
owner-order Vec header/payload before the fallible stable insertion walk.
Other recursive/callback scratch remains separate.

Scalar reads retain their prior unit-returning population path. They do not pay
for a sealed witness they never consume. Actual observations still independently
verify all positive and negative prediction bits before numerical finish.
The intentionally unused cold entry and its residency estimate have narrow,
explained non-test dead-code expectations while recursive admission is absent.

## Verification

Rust 1.97.1, locked dependencies, pinned qualified FFmpeg/ORT, opt-level 2, debug
assertions and overflow checks on the same qualified macOS host. Guards cover
1,422 main and 1,193 isolated inputs. Exactly eleven files change and five are
added. All sixteen published files match isolated and staged source bytes;
only shared_eq.rs has an inherited main overlay.

| Check | Final result |
| --- | --- |
| Main / isolated builds | Pass, 85.90 / 85.06 seconds. |
| Main Audio library / complete definition | 273 / 171 pass, zero ignored. |
| Main enabled real-WAV stages | 203 pass, zero ignored; seven unfinished Split cases filtered, 70.88 seconds. |
| Isolated Audio library | 254 pass, zero ignored. |
| Full isolated real-WAV stages | 203 pass, zero ignored or filtered, 72.55 seconds. |
| Isolated historical birth/processing PCM | Eight pass, zero ignored; 163 other definition cases filtered. |
| Required strict gate | cargo xtask check --no-test passes formatting and strict all-target Core/Plan/Audio/Store lint. |
| All seven unfinished Split cases | Four pass / three work-limit failures across two final-source invocations. Targeted processed-root refinement passes in 199.66 seconds; the other six give three pass / three fail in 206.57 seconds. |

All-owner Source/Sequence refinement, cut-three, limited 0+511, reports
`plan work per read`. Nested selected play without root sound, prefix-two,
limited 0+4096, reports `structural work`. One selected play, prefix-two,
limited 0+4096, reports `frozen projection work exhausted`. These fresh results
retain the unfinished failures. Durations are verification, not performance
acceptance measurements.

Seventeen new tests cover literal EQ/Reverb/Delay schedules and PCM bits,
driver-only voices, gaps/stop/skip, finite/callback/cancel prefixes, exact and
partial work, actual prepared Root/Beat population, failed completion/window
checks, borrowed clocks, Tail exclusions, padded bits, empty traversal, and an
inactive dynamic beat route alongside an active constant root sound. The WAV
fixture is setup, not cold source-admission evidence. Independent review finds
no supported defect in the final driver or completion/accounting correction.

Retained failures include test-only Debug diagnostics, two unit-return type
errors and two style/type-complexity lint errors. Corrections preserve compared
fields and behavior. The exact failing lint source is reconstructed and checked
against its recorded SHA-256. The first integrated root refinement exhausted
`sound processing birth work`: scalar chunks unnecessarily sealed and discarded
completion witnesses. Explicit completion-on-demand removes that operation;
cold sealing keeps its real validation and charge. The restored root case and
all final suites run against rebuilt, pinned binaries. No performed work is
refunded or unbilled, and no limits/weights/cache/retry policy changes.

## Still required

Demand fields do not identify a raw parent. Preserve the actual incoming
Program/context, checked ancestry and sampling-grid handles. Root/Domain
`owner_input` can prove exact membership; a different retained Original clock
needs typed correspondence evidence. OwnerKey and matches_clock_at are not
that proof. Retained policy recursion, arbitrary Projected forms and unproved
absence remain unsupported for cold resource admission.

Share exact metadata node descriptions with numerical construction, then fold
future outputs/tiles, context heap, full native constructor/retained bounds and
recursive scratch before extra histories allocate. The Plan intrinsic query
remains unused by Audio and does not classify caller-created projected PCM.
Graph reads remain scalar; no source/query/discovery/prefix cache, fake provider,
native probe or failed-batch scalar retry is introduced. No requirement/gate
closes: coverage remains about 85%, ten Complete requirements and all seven
gates open. GUI, native ABI and packaging checks are not repeated.

The [inventory](../../tools/media-qualification/evidence/2026-10-28-cold-demand/inventory.json)
retains exact source/binary pins, commands/output, task patches and snapshots,
lineages, intermediate failures and independent reviews. Its exact-context
research is read-only design, not implementation evidence. Raw work is in
/tmp/deadpan-cold-demand-2026-10-28 and the linked numeric/context research dirs.
All 281 gzip payloads verify stored and expanded hashes: 6,704,821 stored bytes
and 28,360,181 expanded bytes. Inventory SHA-256:
`44c4b03efa2106ae01b65c60ac3698062da292376c742add2d6f8fa38a21b5a7`.
