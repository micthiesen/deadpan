# Admitted Tape policy construction, 2026-10-31

`AudioSignalTape::policy_admitted` and `policy_after_preserve_admitted` construct
policy on the Tape's stored grid under the caller's admission budget. Each child
finishes its guarded ranges and inventory before outer extension. Span limits
remain per provider; overrides retain after-Preserve behavior. Unmetered calls
delegate to the ordinary reader.

This Plan checkpoint follows `a6bc3de7e4e27f9fe11252c28cca07af385e4755`.
The isolated copy is shipping authority; inherited Main WIP is excluded. Native
source-policy integration, input graph construction and whole preparation remain
open. No requirement, coverage score or Gate closes.

## Construction and semantics

The factory preserves validation order, actual binary-search comparisons and
per-run canonical fees. Searches add no LookupStats. Each direct Signal policy
starts at depth zero; Tape override recursion introduces no new depth fee or cap.
Empty Tape requests bypass children and retain work zero.

One budget owns canonical and additional work. A child completion describes usage
without debiting already forwarded work again. Lookup is child-local until a
successful child completes, then joins through Tape's checked additions. Failure
restores parent lookup without refunding accepted work.

Each child has a separate guarded inventory and completes actual range sorting,
filtering and merging before outer extension. Source and destination storage
overlap through growth and movement. The root builds one exact final Signal Vec
with its own lease, separate from inventory and temporary ranges. Sealed
Point/Tape provenance reuses the existing custody protocol while retaining Point
error wording. Tape's ordinary reader and native callers remain unchanged.

New allocation, copying and sorting add actual admission work. Existing canonical
fees and limits stay fixed. Capacity differences and earlier low-work failures
remain literal in the evidence.

## Frozen ordinary evidence

The 37th immutable payload is
`crates/deadpan-plan/tests/fixtures/tape-policy-baseline-v1/snapshot.json`,
430,342 bytes, SHA-256
`98ec737a653d5ec192d991717c70bfce5cc22f5c4a3b37197f8c815398bd81a0`.
Both actual pre-change builds/captures, cross-copy regressions and independent
review passed before production integration. All 36 predecessors remain fixed.

Nine public Tapes supply 165 request descriptors. Each policy endpoint mode has
71 successes and 94 errors; current queries have 74 successes and 91 errors.
The corpus retains complete borrowed queries, capacities, work, lookup, input
state and typed errors. A normal regression compares its canonical bytes exactly.

Actual cases include three-run cap-1 policies with three inventory contents,
a zero-width middle sample allocation, independent Source input/output,
occurrence padding, intrinsic override, stored RoundEven samples `-2..10`, and
a late owned-input error after a separate successful first-run work-6 query.
Selected Source suppression distinguishes ordinary `4..10` from after-Preserve
`4..6`, with canonical work 27 in both modes. No narrower-route-versus-support
example was reached. Ordinary errors expose no partial inventory or work receipt.

## Paired runtime and failures

All four complete canonical bodies from builds 4 and 6, both copies, are exactly
12,179,902 bytes, SHA-256
`395b5ef04e911c5243d787fad3e484f0de8d05fd0ec70f8b2afd3b1484338980`.
Only variant, actual source-manifest hash and published-base envelope fields are
removed. The legacy `scoped-published` variant names the shipping-copy lineage;
these candidate executions preceded publication of this checkpoint.

The 330 endpoint-mode lanes each retain original limits and a separate
default-work lane with the original span cap. All 660 forwarding equations
reconcile. Adequate-work results have 222 successes and 108 matching errors.
At original limits, 126 successes remain successes, 16 ordinary successes meet
additional-admission errors, and 188 ordinary errors remain errors. Fourteen
error strings change through earlier exhaustion; every old/new value is retained.

There are 160 literal capacity differences across both lanes. Contents capacity
4 becomes 1 or 2; suppression capacity 4 becomes 1 or 2. Successful contents,
ranges, lookup and canonical work match the frozen ordinary references. The
independent audit checks 27,502 callback events: 19,884 spends, 5,452 reservations
and 2,166 merges. Public factory errors retain their actual accepted callback
prefix; local canonical/additional counters are explicitly unexposed on error.

Actual callback-refusal sweeps cover the three-run Structural Tape and an
intrinsic override followed by Source, in both modes. Their measured successful
peaks, 1,920 and 4,704 bytes in the existing Plan test ledger, supply exact-cap
success and one-byte-short refusal. Every observed spend/reservation callback
is refused in turn with the literal accepted prefix retained and all leases
released. Cancellation stops at the measured forwarded threshold with storage
live; issuer rotation is rejected. Held results survive backend and fixture
destruction. A late second-child error releases its own storage while an earlier
result on the same issuer stays live.

Custody tests prove exact pointer/len/cap transfer, independent final lease,
inventory-live prepare/commit, issuer and prepare-refusal cleanup. Actual empty
Tape handoff has capacity zero, no lease and one prepare/commit pair. This uses
the existing Plan protocol backends; native Account/Box ownership remains owed.

## Verification and publication boundary

All 29 Plan targets passed in build 3: 765 Main / 752 isolated tests, 34 ignored
and zero filters or failures in each copy. The 28 integration binaries per copy
remain physically byte-identical in final build 6. All nine affected final Tape
tests pass each copy; the later changes are test-only. A complete final-library
rerun is not claimed.

Final Main `cargo xtask check --no-test`, isolated Plan all-target Clippy with
`-D warnings`, both formatting checks, rustdoc runs (zero examples) and hygiene
checks pass. Strict build 4's unnecessary-unwrap failure and build 5's two missed
bound-ID references remain in the evidence. Final build 6 corrects only the
test pattern and its references. Independent source/runtime acceptance is GO.

The isolated 2,632 named inputs are authority. All 14 owned source/fixture paths
match both copies; Main's 2,946 and the isolated copy's 2,618 other initial inputs
remain exact. All 37 immutable payload hashes remain fixed.

The [evidence inventory](../../tools/media-qualification/evidence/2026-10-31-tape-policy-factory/inventory.json)
has SHA-256 `4ac8c0788ec7f191f62bdb2b3580281b7320c7922fe2c17f9b66d897f597cc0f`:
463 named records, 335 unique gzip payloads, 255,099,653 named expanded bytes and
34,833,891 unique stored bytes. Every stored and expanded hash is verified.
Source candidates and drafts are labeled separately from executed inputs.
The independent final runtime authority is `final6-runtime-acceptance.json`,
SHA-256 `604161fa7f358068aa203c990f47f557ef99a31876d4932b6568ef7aca7d8801`.

Next integrate the admitted PreserveTape query into the native routed source
policy path. Retain its real inventory and guards through content preflight,
control checks, full range validation, PCM zeroing and final custody preparation
and commit, while the PreparedBlock Arc remains live. Preserve the ordinary
unforwarded lane and never debit admitted work again. Other SignalInput callers,
graph construction, PCM/dependencies and whole preparation stay outside this
checkpoint. Source `gap.after = Some(...)` and successful non-Leaf carrier
rejection remain unclaimed; general sound-bearing Split remains disabled.
