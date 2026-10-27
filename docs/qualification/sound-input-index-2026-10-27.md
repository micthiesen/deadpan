# Original-input index experiment, 2026-10-27

The [work trace](sound-work-attribution-2026-10-27.md) found that all-owner's
existing 64-slot Original-input cache spent 38.53% of its accepted allowance
scanning and shifting its sorted index on eviction. An uncommitted experiment
removes those operations, but the unchanged Split matrix remains four passes
and three work-limit failures. General sound-bearing Split stays unqualified.

## Change and review

Only `shared_eq/original_inputs.rs` and its tests change from the placement
milestone's candidate runtime. Fixed bucket heads and intrusive hash links
replace the sorted index. Known-slot eviction and insertion charge 22 fixed
units before mutation. Lookup charges six units for hashing its five scalar
key fields and bucket reduction, then one per occupied probe, bounded at 64.
There are no tombstones, growing tables or new PCM/query caches.

Hash links remain separate from the original LRU links. Touch order, stable
slots, free-list reuse, entry count, source admission and full context/range/
depth/fingerprint checks remain unchanged. Hashes select buckets only. Fixed
heads and both new per-entry links fit the old index storage bound, while the
existing `size_of<Cache>()` reservation continues to charge actual metadata.
An empty miss now costs six units and a full collision miss costs 70; those
tradeoffs are retained rather than waived. No work or memory ceiling changes.

Three new tests cover a full collision bucket, exact probe counts, head/middle/
tail unlink, independent LRU order, slot reuse and repeated churn; exact
22-unit insertion/removal versus 21-unit atomic refusal and cancellation; and
fixed metadata admission. Existing hit-admission, revoked source, retained
identity, stale/evictable payload and tight-memory tests remain. Independent
review finds no concrete defect. This establishes local index behavior, not
completion of the failing bus reads.

## Verification

Same qualified M5 Max/macOS/Rust/FFmpeg environment as the preceding trace;
locked dependencies, opt-level 2, debug assertions and overflow checks.

| Check | Result |
| --- | --- |
| Stable three-package build | Pass; 84.50 seconds. |
| Audio library | 202 pass, zero ignored. |
| Full Audio definition target | 171 pass, zero ignored; 265.19 seconds. |
| Strict check | `cargo xtask check --no-test`: four-crate all-target formatting/lint pass. |
| Unchanged all-owner Split test | Fails at Source/Sequence `cut-three`, limited `0+511`, `plan work per read`; 94.87 seconds. |
| Other six unchanged Split tests | Four pass, two work failures; 287.71 seconds. |

The selected case still fails at `prefix-two`, limited `0+4096`, now with
`AudioQueryLimit("sound clock scope work")`; nested selected without root sound
fails at the same boundary with `AudioQueryLimit("structural work")`.
Processed root-sound refinement retains its complete authored/faded/limited PCM
pass. These failures remain incomplete comparisons. The 203 earlier existing
stage tests are not rerun for this experiment; their prior result is not a new
claim for this source. No native UI behavior changes or new GUI claim is made.

All seven Split tests run once across two invocations against one source-stable
copied executable, SHA-256
`fa8fb1b2baef63aba2bc12e044ec50c6fef727ffcee27b6f9c32d077b7cfe672`.
The guarded 1,395 inputs differ from the preceding candidate in exactly the two
index files. Core and Plan source remains unchanged.

A second diagnostic source copy changes only those two files from the first
trace's 5,279-input copy. Its separate build passes in 61.42 seconds and all-owner
reproduces the same failure in 103.66 seconds. Accepted site sums reconcile to
16,777,216 units, optional work is zero, and no bins/counters overflow. Insertion
uses 1,512,698 units and removal 1,509,882; the old scans and shifts disappear.
Transfers now use 4,057,429 units. The request reaches more retained reads before
failure (67,733 versus 50,851), but neither trace completes it. Their different
stopping points and diagnostic overhead prohibit a total-work or timing claim.

## Continuation and evidence

The remaining repeated prefix work needs a different execution structure.
Independent archive review confirms the rejected sibling prototype still ran
one scalar traversal, retained route PCM, then remixed other outputs through
cache lookups. A direct multiple-output traversal would instead fill caller
histories from one actual Components pass. It must preserve each target's alias
mask, used final gains, route-order sums, source admission and cancellation.
Before runtime integration, prove the recursive allocation peak for simultaneous
wet histories and select scalar fallback before doing prefix work. A current-room
check followed by memory-error retry repeats the rejected failure mode. The
retained design note proposes a pure Components oracle first; no such API or
runtime batching is implemented or qualified here.

The two source files stay inside the inherited uncommitted general-Split work.
No requirement, criterion or gate closes. Raw records are in
`/tmp/deadpan-input-index-2026-10-27` and its sibling profile directory. The
[evidence inventory](../../tools/media-qualification/evidence/2026-10-27-input-index/inventory.json)
retains the exact patch and before-files, review, design, all commands, outputs,
input/binary pins, second-copy lineage and charge analysis. All 56 payloads verify
stored and expanded hashes: 2,994,989 stored bytes and 12,256,898 expanded bytes.
Inventory SHA-256:
`1f355b2da5f6c5bb76e62c242c1cee9c799e9e413397c077e61c88d0b797a780`.
