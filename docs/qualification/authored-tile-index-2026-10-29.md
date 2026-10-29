# Authored tile cache storage admission

Authored EQ now admits the full capacity of its tile index and the shallow Arc allocation for each published tile. The index uses a private sorted Vec in place of BTreeMap. The ordinary native regressions pass. General sound-bearing Split stays disabled; its unfinished work-limit failures receive no acceptance credit.

## Storage and publication

The index keeps exact i64 keys, ordered cache observations and one publication per miss. It admits the actual Vec capacity, including slack. Checked geometric growth uses the existing memory/work limits and introduces no tile-count cap or coupling to maximum_prepared_stages.

Growth is prepared after recursive tile computation and filtering, against the actual current index. The full replacement is reserved while the old allocation remains resident. Allocation checks Vec::capacity against its admitted Layout. Spare insertion keeps its existing full capacity charge.

The pending Arc reserves the same pinned Rust layout used by Plan metadata: two usize counters extended with the aligned WideBlock, then padded. It covers the shallow allocation once; Arc clones share it. The existing OUTPUT*2 PCM bound is unchanged and is not charged again. Variable suppression capacity, deep dependency maps and allocator overhead/RSS qualification remain open.

The final PCM room check sees both new metadata leases while tile workspace, old index and prior graph state remain live. Allocation, entry-work payment, neighbor/capacity validation, cancellation, issuer checks and all active/resident arithmetic finish before moving entries. A dedicated named index/Arc two-lane transfer reuses private ledger mechanics; the existing seven node lanes keep their meaning.

The publication tail has no fallible operations. It moves entries, physically frees the old Vec before retiring its charge, updates checked aggregate counters and disarms the actual leases. Pending values drop before their leases on errors. The native filter is restored on every failed suffix. Actual returned/cache Arc clones finish before the outer Graph drops and releases aggregate residency; this is a caller-scope proof, not a Graph lifetime enforced by the plain Arc type.

Changing the index representation also changes the shallow Node layout. Existing node-vector admission uses actual candidate Node layout; tests independently reconcile this delta against the frozen old layout. They never estimate private BTreeMap node allocations.

## Exact work and error order

The index pays one unit immediately before each actual key comparison. It prepays actual relocated entries plus one new entry write: old_len+1 during growth, or suffix_len+1 in spare capacity. Final neighboring-key comparisons are also paid. These are additional fees; prior controller fees and weights remain unchanged, and accepted work is never refunded.

Production always enables the new fees. The complete recursive reference mode disables only these fees while using the same searches, moves, allocations, memory reservations and publication. A zero-allocation test-only Cell records accepted fees; helper tests independently pin exact comparison/write counts.

Initial control and absolute-depth validation still precede lookup. Cached relative-depth validation follows paid lookup and precedes Arc cloning. With no remaining work, the new lookup can therefore refuse work before an old cached relative-depth error. A focused design review required this explicit resource-prefix distinction; full old error parity is tested with sufficient headroom.

## Independent native baseline

Before production edits, both source variants executed the actual old eq_tile/read_eq_program path. Seven complete bodies agree literally except variant and source_sha256 provenance. Cases cover negative/adjacent keys, repeated hits, at least 16 actual map entries, nested EQ, repeat suppression, a qualified repository stereo WAV with nonempty dependencies, an actual physical Domain and retained capture.

The baseline records complete f64 PCM bits, suppression, dependencies, depths, provider reads, cache entries, Arc counts, observable capacities, filter restoration and old controller work. Setup and imposed policy probes are labeled separately. Private BTreeMap capacity is explicitly opaque.

The immutable 7,650,294-byte fixture SHA-256 is 6dc6f9371528cf6229fa3e0c2e85ffc911f5ba4f687d3141c6e66758a06ce71a; common compact logical SHA-256 is fb1b9a48f6f2caef6a969695b01c73521a7814600cf1970470be99fa3fc745b5. Original producer bytes and run/source/executable pins remain in evidence.

Both recursive modes preserve all remaining frozen fields literally. Every observed controller total is reconciled separately with accepted new fees. Resource counters account for exact Node layout, actual index capacity and one independently calculated Arc shell per tile. Fourteen helper tests cover exact fees, ordering, capacity overlap, refusal prefixes, cleanup, retries, issuer/sum guards and trace nesting/unwind. Five native/resource tests cover the entire old matrix, Arc sharing/drop order and actual late-publication refusal with filter/graph reuse.

Three fresh code reviewers found no unresolved issue. Static checks first found one observer still calling the old map's len method, then two test helpers exceeding argument-count lint. The observer now borrows entries; scalar tuples reduce helper arity without changing any recorded input or output. No lint allowances or product corrections were needed.

## Final verification and remaining work

Optimized Audio library suites pass 370 Main/351 isolated, with three ignored tools each. Ordinary real-WAV Stage suites pass 203/203 in 74.18/69.22 seconds. Main definitions pass 171 in 89.12 seconds and isolated new-group history passes eight in 3.36 seconds. Strict cargo xtask check --no-test passes formatting and Core/Plan/Audio/Store all-target lint. The Audio doctest command passes with no examples. Each final Audio build pins 21 executables; only listed executed targets receive coverage credit.

Plan is unchanged and does not depend on Audio. Its preceding 627/614 passing tests across 29 executables each, with six ignored tools, are reused rather than rerun. All 58 pinned executable hashes and prior guarded inputs were checked: only two unlinked Audio production files changed, while Plan/Core sources, fixtures, manifests, lockfile and compiler configuration remain unchanged. Newly added files are Audio only. The source/dependency basis and prior exact-binary reuse join are preserved in the audit.

The fresh seven-case Split matrix remains **three pass/four work-limit failures**, in 267.306528 seconds:

| Case | Revision and limited window | Error |
| --- | --- | --- |
| Source/Sequence refinement | cut-three, 0+511 | Stage(Limit("plan work per read")) |
| Nested selected play, root_sound=false | prefix-two, 0+4096 | Stage(Plan(AudioQueryLimit("recursive policy work"))) |
| One selected play | prefix-two, 0+4096 | Stage(Plan(AudioQueryLimit("owner occurrence ancestry"))) |
| Root sound, processed=true | root-sound-refinement, 0+511 | Stage(Limit("plan work per read")) |

The failing windows are unchanged. Nested selected play previously reached Core binding-work exhaustion; one selected play previously reported structural query work. Their new refusals are different budget prefixes. Source/Sequence and processed root sound keep their prior plan-work errors. No speedup or successful Split transport follows from the shorter run.

All 17 guarded receipts are terminal, including two negative static-check receipts and the failed Split matrix. Every run preserves its recorded source and executable guards. Final compiled-source/index equality, eight immutable fixtures and the qualified Plan-result reuse were audited separately.

Suppression/dependency containers and their temporary overlaps, other shared Graph containers, mix route/bus/layer clock copies and complete recursive cold-resource closure remain open. Caps, prior weights, refunds and cold activation are unchanged. No requirement or gate closes.

## Publication boundary

All nine staged source files, including seven Rust files and two fixture/README files, equal both Main and the isolated compiled candidate. All other 1,488 Main and 1,259 isolated initially guarded inputs remain unchanged. Eight immutable oracle hashes remain unchanged in both variants.

Base revision: 43f6f7de4c4dbf25dbad05f5eaf3d25eac261643. Scratch: /tmp/deadpan-authored-tile-index-2026-10-29. Scoped source patch SHA-256: 32e16a0a608d4eed8ed90f11c89ef6ac4142b892b247ff122b97e25bde60eac7.

The evidence inventory retains frozen producer sources, old native observations, both source variants, author drafts/corrections, all guarded run receipts/streams, independent reviews, exact Plan reuse and final source/index equality. Stored and expanded hashes are verified before publication.

The [inventory](../../tools/media-qualification/evidence/2026-10-29-authored-tile-index/inventory.json) contains 167 compressed records, 5,816,476 stored bytes and 51,445,953 expanded bytes. Its SHA-256 is 8992240893ed4f6e189df552717d321da38b5d310b4918f8a887eb561a251d3f. Stored and expanded hashes were checked before publication. The preceding Authored-node inventory is linked by its exact digest.
