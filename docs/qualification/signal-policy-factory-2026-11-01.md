# Standalone Signal policy factory admission

AudioSignal::policy_admitted and policy_after_preserve_admitted now construct
the actual public PointCeil policy through the existing provider-aware budget.
The exact final Signal Vec has its own lease, separate from retained content
inventory and temporary guards. Sealed Signal provenance uses the existing
prepare/commit custody protocol. Unmetered calls use the corresponding ordinary
public method literally. Existing Point/Tape behavior and strings remain intact.

This is a Plan factory prerequisite. Native intrinsic preflight integration,
graph construction, PCM/dependency ownership and whole preparation remain open.
No requirement or Gate status changes. General sound-bearing Split remains
disabled in the shipping copy.

## Frozen ordinary behavior

[Baseline39](../../crates/deadpan-plan/tests/fixtures/signal-policy-baseline-v1/snapshot.json)
was captured in both copies and independently frozen before integration:
816,706 bytes, SHA
9bf26d8e60022c503f33fb74f7926caabb976dae6dbc813a2c39b1d9414157dd.
All 39 immutable payloads remain exact.

Public constructors supply 16 Signals/207 requests: eight Source, three Silence,
three Projected, one Structural and one RequiresOwnedInput. Each ordinary policy
mode has 67 successes/140 errors; the separate public query reference has
123 successes/84 errors. Both endpoint modes, full/cropped/empty/outside/reversed
ranges, original limits and input snapshots are retained. Six endpoint-mode
differences remain literal. Whole-public empty work is one for
Structural/Source/Silence and two for Projected; RequiresOwnedInput rejects.
Owned root extraction retains its actual splice-placement InvalidPlan error.
These are checked Plan metadata fixtures, without new media/store qualification.
Repeat-gap-after Some coverage remains unclaimed.

## Paired admitted results

Both complete final4 bodies are byte-identical: 14,621,432 bytes, SHA
ce28e66d38e8354c97e7b802219f4d87f31f5b185d338e316796a8ddf99c8077.
They retain 828 metered lanes: every request in both modes at original work and
separate default work with the original span cap. Every lane also executes the
unmetered facade. Only three named provenance fields are removed for comparison;
no work, capacity or errors are normalized.

Original-work lanes retain 116 successful pairs, 280 failed pairs and 18 ordinary
successes that now refuse earlier. Eighteen failed pairs report a different
earlier resource-limit reason. Every changed refusal is typed AudioQueryLimit.
Default-work lanes have 264 successful/150 failed pairs with matching logical
values/errors. All 380 successful metered results retain canonical = actual
ordinary work and total = canonical + additional = view work = accepted callback
work, charged once. Literal capacities differ in 220 successful pairs and remain
captured separately from equal suppression, content and lookup values.
Error-local canonical/additional work is unexposed by the public constructor;
only the actual accepted callback prefix is claimed.

Six actual failure families cover Structural, Source output and late Projected
requests in both modes. Every actual spend/reservation can refuse with cleanup.
Measured Plan allocation peaks are 4,728, 5,072 and 872 bytes respectively.
Exact-peak and one-byte-below attempts retain actual results; Projected retains
its late policy rejection. Cancellation and issuer rotation use real callbacks
without seeded work. All input snapshots remain unchanged.

Custody covers 64 attempts, including 11 positive and 43 zero-capacity committed
masks; ten provider failures remain explicit. Tests compare the actual final
pointer/length/capacity and independent lease, keep inventory live through
prepare/commit, and exercise foreign issuer, unmetered handoff and preparation
refusal. A held positive result survives backend and all fixture-builder
destruction, then releases on drop. These use the existing Plan protocol backend,
not native Account/Box custody.

## Verification and retained failures

All 29 Plan targets are covered per copy: one full fresh final4 library and
28 physically byte-identical integration executables that passed matrix3.
Main passes 770 tests; shipping passes 757. Libraries pass 320/307, 37 ignored
manual entrypoints each, zero filters/failures. Pure Plan uses opt0/incremental1,
debug assertions and overflow checks. Main strict check, shipping all-target
Plan Clippy, both formatting, rustdoc (zero examples) and hygiene pass.
Independent source, baseline and final runtime acceptance pass.

Matrix3 remains failed: its libraries passed 318/305 with two failures each.
The new failure/custody tests reached typed issuer mismatch, then the borrowed
ordinary serializer panicked because it had no named variant. Only two new
K test serializers changed; production and old serializers stayed exact.
Final4 reran both entire changed libraries. Failed3 is not reported as green.

The [evidence inventory](../../tools/media-qualification/evidence/2026-11-01-signal-policy-factory/inventory.json)
retains guarded builds/runs, raw/canonical captures, source/artifact identities,
reviews, candidates and failures. Candidates are not execution claims.
Shipping's 2,646 named inputs own publication. Main's 2,964 and shipping's 2,636
other initial inputs remain exact; inherited Main WIP is excluded.
