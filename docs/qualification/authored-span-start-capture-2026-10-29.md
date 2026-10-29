# Reuse the actual Authored span-start owner observation

Authored Root and physical Domain EQ preparation now captures selected inputs from the actual successful wide query at each active span START. It removes the subsequent one-sample membership query. The wide query is still performed and billed once.

Plan owns the sealed parent/query wrapper. Only the actual Root or Domain factory constructs it. Capture checks the same physical parent address, issuer, accounting mode, active span and exact START, then scans in original order to the first private same_at match. It uses the existing capture_in_parent kernel and actual Domain seed. It does not infer membership from overlap, use a coarse treatment key, assume duplicate uniqueness or certify arbitrary interior samples.

The restriction follows the existing walker: every span cursor restarts the same physical seed, both gain queries allow inactive support with no retained-query seed, and requested end caps the span end. Owner affine maps anchor at that cursor. This is a START-only restriction. Inactive spans remain skipped before EQ selection.

The shared query receipt is borrowed attribution, not a summable per-node bill. The new Input work contains only actual selection and capture: canonical capture in unmetered mode; canonical plus additional capture and additional first-match selection in metered mode. The existing unmetered additional operation costs zero. Fresh capture limits bound the new work without subtracting the already paid whole wide query again. AudioOwnerInput layout and ordinary direct APIs are unchanged.

Audio preserves cache-hit and owner-count checks before capture metadata, context before recipe before boundary, source Input and its independent lease through the retained probe and final sharing, and the actual wide query guard through use. Incoming Retained keeps the same Arc and its existing no-membership-query path. No caps, prior weights, refunds, batch/cold activation or PCM processing changed.

## Independent old evidence

Before production changes, optimized pinned executables from inherited Main and the isolated published candidate independently captured matching old observations, differing only in variant/source provenance.

- The 1,555,568-byte START snapshot freezes 14 physical parents, 27 active spans, 73 actual direct captures and 51 full output/event crosschecks against direct258. Complete ordered private owner vectors, support and Original silence match separately observed one-sample spans. It includes Repeat/default-gap/retained-origin/retained-reference Domain cases, three inactive spans and one ownerless witness. The ownerless witness is also inactive. No duplicate was observed; a separate synthetic first-match test supplies that missing case.
- The 301,212-byte metered point-cost annex freezes one actual old point query per parent/span START under default finite work and a 1 MiB allocation cap. It records canonical/additional/forwarded work, full private query facts and before/live/peak/reserved/released/after-drop memory ledgers. It performs no Input capture.

Golden SHA-256s are 9ca2248a54ddff226da794e243e019ba609d8d7695f0c8a1042205f3c9d52181 and ab4d26227514ea952190d6e4893fb537dead27814e0b78a5635a902686430099. Both immutable bodies remain unchanged.

## Verification

Final Plan suites pass 635 Main and 622 isolated tests across 29 executables each, with eight ignored capture tools. Both accounting-mode differentials compare every frozen Input semantic field and actual capture event; work is reconciled separately. The metered test independently observes the unchanged point query, checks its full facts and receipts against the frozen annex, and requires its complete event sequence to equal the direct capture event prefix before comparing all remaining capture events.

Six focused Plan tests cover exact/short work in both modes, first-match duplicates and zero false-mode selection, foreign Root/equivalent Domain, inactive/interior/index rejection, finite exact/one-short peak memory, every actual work/allocation callback refusal and issuer rotation, and cancellation/mode/issuer mismatch cleanup.

Audio libraries pass 372 Main and 353 isolated tests, with three ignored tools each. The full seven-scenario native matrix compares production certified recursion to actual unchanged direct recursion, then direct recursion to frozen OLD. Complete f64 PCM, dependency sets, suppression, depth, cache/resource observations and errors stay literal. Only cumulative actual direct point-query receipt work is removed, with matching positive capture counts. Existing index fees/layout deltas retain their separate prior proof. Focused Audio tests verify Root/Domain description parity and cache/count order.

Strict formatting/all-target lint passes. Three fresh reviews found no actionable issue; the general reviewer rechecked the final test correction.

The initial Plan library run retained two failed new test assertions: ownerless coverage was counted after the inactive skip, and a metered query-only Core event was incorrectly counted as capture work. Only the differential test changed. The fixed test counts the overlapping witness independently and proves the complete removed query-event prefix; it discards no unclassified event. No production change followed first compilation.

The isolated plan.rs initially received inherited Main-only edits during a mirror. Before isolated compilation, its saved baseline was restored through a patch and only the new exports applied. Main inherited bytes are preserved. All other 1,488 Main and 1,259 isolated initial inputs are unchanged. Seventeen task source files equal across variants; plan.rs has identical task export changes over its two distinct baselines. The shipping index uses the isolated published version.

The ordinary Stage suite passes 203 tests in both variants; Main definition/history passes 171 and isolated birth checks pass eight. Plan and Audio doctests compile successfully with zero examples. The Main ordinary Stage and all seven inherited Split cases ran together, single-threaded, in 530.31 seconds: 206 pass, four fail. This timing includes the 203 ordinary tests and is not comparable to earlier Split-only durations.

| Split window | Fresh refusal |
| --- | --- |
| Source/Sequence, cut-three, limited 0+511 | Stage Limit: plan work per read |
| Nested selected play, root_sound=false, prefix-two, limited 0+4096 | Core LimitExceeded: frozen projection work exhausted |
| One selected play, prefix-two, limited 0+4096 | Core LimitExceeded: audio binding work exhausted |
| Processed root sound, root-sound-refinement, limited 0+511 | Stage Limit: plan work per read |

The same four windows remain unresolved. The nested and one-selected-play prefixes changed from recursive-policy and owner-ancestry refusals to Core projection and binding refusals. Reaching another work limit is not a passed witness or an acceptance improvement. General sound-bearing Split remains disabled. No speed, peak-memory, requirement or gate closure is claimed. Variable suppression/dependency containers, other Graph storage, mix copies and full recursive closure remain open.

All 23 guarded execution receipts are terminal (count verified by the audit; the initial two-assertion Plan failure and the fresh four-failure integration run remain negative evidence). The final audit verifies source/executable/stream identities, both old cross-variant matrices, all ten immutable golden bodies, exact shipping index/source equality and preservation of the two different plan.rs baselines. Native executables retain current production source; their only later guarded source difference is the fixed nested cfg(test) Plan differential module, whose entire module chain is recorded.

Scoped source patch SHA-256: 6f5abb0f29d270d0b1b1640721b5d92170399bea12013d1d64d9ef8244c3a7a7.
Base revision: 810b2b739222b2f175e1e73ef9addb5944decba2.
Scratch: /tmp/deadpan-span-start-capture-2026-10-29.

The [evidence inventory](../../tools/media-qualification/evidence/2026-10-29-authored-span-start-capture/inventory.json) contains 349 compressed records: 4,931,732 stored bytes and 27,762,951 expanded bytes. Its SHA-256 is ecded01d7022e30ef6b78c2215b167da099297d5bc0769c88905cd552ac75ece. Stored and expanded hashes were verified before publication. It links the preceding dependency-borrow inventory by its exact digest.
