# Wide gain-query and temporary stop admission

Ordinary media regressions pass. The unfinished Split matrix is now three pass/four work-limit failures, including a newly refused root-sound refinement. General sound-bearing Split stays disabled.

The ordered Audio renderers now retain admitted wide owner queries through their raw reads, native preparation and recursive tiles. Root mix routing also retains its query through route construction. Each query uses one shared work/admission policy; the controller skips work already forwarded by Plan.

The shared tile copies its exact stop Boundary into a guarded local value before taking the native filter. SharedMix borrows that value, removing its second stop copy. The snapshot remains charged through nested reads and mixing. Existing caps and work weights stay fixed; accepted work remains spent after failures.

## Query construction and validation

RenderPlan::audio_gain_owners_admitted and AudioDomain::gain_owners_admitted reuse the existing query_tracked walker. Their normal APIs use the same orchestration in false mode. A checked internal Product constructor reuses the existing retained-query carrier, issuer check and canonical/additional provenance check. The retained-query algorithm is unchanged.

Root keeps range/audio-duration and root initialization before query limit and reversed-range validation. Domain keeps its physical-range check and seed copy before that inherited validation. Additional seed-copy work consumes the same remaining budget while validation receives its original limits. An exact zero remainder can therefore still produce a valid empty query. Read-only issuer checks remain in false mode; spending, reservation and merge callbacks stay absent.

Context::owners_admitted validates all query layers after constructing/paying the result. The raw Context::owner_query_admitted helper serves root mix routing: it pays once, selects the root identity, then leaves validation to the selected route, preserving the old order. False mode bills canonical work once; true mode streams canonical and additional work during construction. The mutable Admission borrow ends before later renderer calls; the returned leases retain only the request ledger.

## Exact temporary stop ownership

AdmittedStop::from_boundary copies the actual Boundary's node, RepeatInstance array, gap, optional iteration and stage index. It reserves the array before allocating/copying it and checks the issuer around callbacks. Copy work is five plus repeat count; the existing owner-to-stop constructor stays four plus repeat count. The value drops before its lease.

The tile snapshot is independent of Graph storage, so recursive Graph growth cannot invalidate its borrow. Snapshot refusal happens before filter.take. Later PCM-headroom refusal restores the taken filter and drops the snapshot. SharedMix has a separate stop-borrow lifetime and reads program.stop directly. The stop comparison uses the same borrowed OwnerKey predicate.

## Independent old observations and regressions

Before changing Plan production, the old public Root and actual physical-Domain APIs ran once per measured case in both source variants. Their complete 14-case bodies agree, excluding only variant/source-manifest provenance. Separate checked setup queries are labeled separately. Cases cover multi-span Repeat/default-gap paths, nonempty Repeat identities, fractional Point-derived grids, clipped windows, retained history and exhausted inactive tails.

The unchanged private query serializer records complete owner fields, sample maps, support/silence, lookup and work; physical Domain setup also records its full seed identity. The immutable 155,065-byte fixture SHA-256 is f8ea96137c9678007b9488881d64a57a25a803c9ed8f47541491e01993e0f9e0. Its common compact logical digest is 66bbb672abdae06b807feb6761a696ed2531717fdf130aeb7c6254f3cfdf86d7.

Normal/false-mode replay matches the entire frozen matrix literally. Metered tests match every field except total work, require canonical work equal to the frozen value, and reconcile total with canonical plus additional and forwarded policy work. They cover exact/short memory and work, each reservation and work callback refusal, two simultaneously retained queries, carrier lifetime beyond the policy, cancellation, issuer changes, range/limit/seed order and an empty query after seed work exhausts its local budget. A private compiled-metadata mutation tests duration-overflow ordering only; it is not a normal-document oracle.

Audio checks false-mode caller work once and metered control work equal to recorded forwarded work. A real wide-query lease blocks one-short native node admission, stays charged after refusal, then releases memory for retry without refunding accepted work. A repeated-owner tile test separately refuses its snapshot reservation and one-short PCM workspace, retaining the published filter and Graph counters without provider reads.

Three fresh reviewers found no unresolved issues in general correctness, Plan budget/provenance or Audio lifetimes/validation. The first compile found one owned-stop test needing as_ref. A misplaced test hook then caused an inner-module-doc parse error; it was moved below the comments. The first native Audio run passed 341 tests and failed the new headroom test's incorrect expectation that failed node preparation did no work. Its correction explicitly retains that accepted work. Product code did not change for the correction.

## Final verification and remaining failure set

Optimized main/isolated Audio pass 342/323, with two ignored capture tools each. Plan passes 620/607 across all 29 executables, with five ignored capture tools each. All 29 final Main Plan executables are byte-identical to the preceding passing run, whose result is reused with an explicit source-difference audit limited to the unlinked Audio test correction. Fourteen Audio executables changed, including Stage; all native results use their final build.

All 203 ordinary real-WAV Stage tests pass in each variant, in 85.02/68.99 seconds. Main definitions pass 171 in 76.42 seconds and isolated historical-group tests eight. Strict cargo xtask check --no-test passes formatting and all-target lint. Both final builds pin 54 executables, including four example harnesses whose unexecuted targets receive no coverage credit.

The fresh seven-case Split matrix is **three pass/four work-limit failures** in 275.395131 seconds:

| Case | Revision and limited window | Error |
| --- | --- | --- |
| Source/Sequence refinement | cut-three, 0+511 | Stage(Plan(AudioQueryLimit("structural work"))) |
| Nested selected play, root_sound=false | prefix-two, 0+4096 | Stage(Plan(AudioQueryLimit("structural work"))) |
| One selected play | prefix-two, 0+4096 | Stage(Plan(AudioQueryLimit("structural work"))) |
| Root sound, processed=true | root-sound-refinement, 0+511 | Stage(Limit("plan work per read")) |

The root-sound refinement case passed at the preceding caller checkpoint and now refuses at the unchanged plan-work limit. This expands the known failure set. The shorter matrix run ends more cases early and establishes no speedup. General sound-bearing Split remains disabled. No requirement or gate is closed.

## Publication boundary

All 18 staged files equal the isolated compiled candidate; 15 also equal Main. The three differences preserve inherited Main Graph fields. The 16 Rust files and two fixture files are the complete source publication. All other 1,464 Main and 1,235 isolated initially guarded inputs remain unchanged. Five immutable oracle hashes are unchanged in both variants.

All 26 run receipts are terminal, including four negative compiler/test/Split receipts. Every executed run preserves its recorded source and executable guards. Final compiled-source equality and the qualified Main Plan binary reuse are audited separately.

Authored EQ's persistent context/boundary/node allocations, its tile boundary clone, other Graph containers, mix route/bus/layer clock copies and complete recursive cold-resource closure remain open. The next Authored increment needs an exact admitted direct-owner-input seam: its private same_at selection includes play identity, so Shared Description's coarse first OwnerKey match is not a substitute. This checkpoint is construction accounting, not allocator instrumentation or RSS qualification.

Base revision: f75cb108f5e6c11618783dea99340b369679e081. Scratch: /tmp/deadpan-wide-owner-query-2026-10-29. The 18-file scoped patch SHA-256 is 9df64bbf86bdb54f5ef36e182f9699edd122742d13812e9ca20de474978555de.

The [inventory](../../tools/media-qualification/evidence/2026-10-29-wide-owner-query-admission/inventory.json) contains 346 compressed records, 4,826,285 stored bytes and 19,804,743 expanded bytes. Its SHA-256 is 928f705f3cd2341f93fc87c9b4643ec9d9dac8add7d2fa9a8eb80bedb80301fb. Stored and expanded hashes were checked before publication. It retains old producer/source pins, immutable fixtures, author drafts and corrections, all run streams and receipts, the Main Plan binary-reuse audit, final source/index equality, independent reviews and next-step research. The preceding caller-storage inventory is linked by its exact digest.
