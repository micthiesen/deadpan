# Updating a speed stage that owns exact pause clocks

2026-10-20. Read-only code-path investigation. No Cargo, runtime reproduction,
Git operation, or repository edit was performed for this scout. Existing test
fixtures below are source evidence, not a claim that a new witness ran.

## Concrete public-command gap

The smallest useful case already has a public construction path in
`crates/deadpan-core/tests/insert_time/retimed.rs`:

1. `fixture()` constructs root `[a: Source2, slow, b: Source2]`, with `slow`
   an Edit/FollowSpeed Retime of Source12 over output24, child window `[0,12)`.
2. `pause(&before, "p", 5, 3, 5)` authors public `InsertTime`: three silent
   frames at root5, with exactly five split identities plus the Hold identity.
3. The source test `fractional_child_cuts_author_exact_clocks_and_accept_later_pauses`
   records the resulting geometry:
   - `slow`: extent27, exact child window `[0,27/2)`;
   - left Partition `p-0`: extent3/2, window `[0,3/2)`;
   - pause adapter `p-4`: extent3/2, window `[0,3)`;
   - right Partition `p-1`: extent21/2, window `[3/2,12)`;
   - root31, including its two two-frame outside Sources.
4. Issue public `SetRetime { node: slow, duration: 15, pitch: FollowSpeed }`
   with fresh request revision. This currently reaches the ordinary reducer,
   changes the integral duration, then `derived_clock::sealed_preserved`
   refuses the changed clocked owner. Expected source-derived error is
   InvalidCommand, message `this edit would change content inside a speed
   change with exact pause clocks; edit outside it or undo those pauses first`.
   Confirm that exact runtime error and unchanged document/history in the
   retained old-code witness before implementing.

This is an explicit unimplemented command path, not an observed phase defect.
Native input refuses even earlier in `deadpan-app/src/project/retime.rs::resolve`:
`clocked_retimes.contains(node)` rejects an update unless `wrap` was requested.
`project/tests/retime.rs` already tests that a stage remains blocked even after
its OWN derived entry becomes integral and disappears, while descendants still
have exact clocks. A helper checking only `derived_clocks.contains_key(node)`
would miss that ordinary-owner case.

## Why relaxing the guard is incorrect

`command.rs::reduce` currently mutates only `NodeKind::Retime.duration/pitch`.
It never updates a `DerivedRetimeClock`. In the witness, the old effective
extent27 would continue overriding a changed recipe15. With the sealed guard
removed this would either preserve the wrong audible duration or fail the
ordinary recipe/exact extent agreement. The exact selected window must not be
replaced by the rounded recipe window `[0,14)`.

Both native `project/retime.rs::resolve` and semantic
`semantic/planner/retime.rs::resolve_speed` currently use
`mapping.duration(): FrameDuration` when updating an Edit Retime. This is also
wrong after a fractional pause. The witness's exact input is13.5 frames, its
covering recipe is14. At requested speed7/8:

- correct output: round_even((27/2)/(7/8)) = round_even(108/7) =15;
- rounded-recipe output: round_even(14/(7/8)) =16.

Native and semantic preview, effective-speed reporting, no-op detection, and
the committed command must share exact input semantics. `:pitch` currently
derives speed from recipe mapping/duration too; it must not accidentally
rewrite speed when a clocked target is admitted later. Preserve/Shift on this
family remains a separate processing-input/output problem, described below.

## Normative behavior and the smallest complete slice

Spec §4.4 makes Retime an explicit output count with a valid selected child
window, and §10.5 requires the same timing/DSP semantics for preview/export.
The retimed pause implementation explicitly keeps the speed stage as a live
owner so later speed edits apply to the inserted pause as well. Therefore an
update scales that pause together with the selected child. It must not keep
the pause at its old outer three frames by modifying its adapter.

First qualify **an existing ordinary Edit/FollowSpeed stage, at the top of its
clocked family, reached through ordinary Sequence ancestors**, with retained
affine descendant clocks from public pause insertion. Keep child identities,
exact child selection, all descendant clocks, recipes, Source binding lattices,
resumes, chronological reanchors and provider policies. Change only this
stage's requested integer output extent; update its covering recipe using the
existing exact setter convention. Preserve no-op, wrong-kind/Partition,
positive-output, overflow and all current sound/marks admission.

This is the native user-visible blocked case and can be complete without
inventing a new primitive or format. It is not a reason to change the existing
ordinary SetRetime behavior for unrelated unclocked stages. More general
ancestor/occurrence cases must retain their current explicit refusal until
their extent propagation and integer-root result are separately qualified.

For the witness's output15, retain `[0,27/2)` and compute slope9/10 child
frames/output frame. Derived descendants remain byte-for-byte unchanged.
The exact root allocations become:

- a: `[0,2)`;
- left input fragment: `[2,11/3)`;
- pause: `[11/3,16/3)`;
- right input fragment: `[16/3,17)`;
- b: `[17,19)`.

The authored Hold remains duration3 below its existing extent3/2 adapter. Its
new outer duration5/3 is the deliberate effect of changing the containing
speed. Root19 is integral. No intermediate boundary is rounded.

## Bounded production integration

1. `crates/deadpan-core/src/insert_time/retimed.rs`
   - Reuse `clock_of` and `set_clock`, possibly extracting the latter to a
     small shared exact-clock module if ownership warrants it. `clock_of`
     returns live exact geometry or ordinary integer geometry. `set_clock`
     keeps exact values, emits covering integer recipes, and removes only a
     now-redundant derived record when all coordinates are integral.
   - Add a narrow audited `set_retime` path for the supported clocked owner:
     validate target/purpose/FollowSpeed and ancestry before cloning; capture
     old effective selected window and extents; set extent to the requested
     integer count while retaining child_start/child_end; keep every descendant
     derived entry and audio binding untouched; reconcile changed lineage;
     validate final exact geometry before setting the existing exact-edit
     receipt flag. Do not bypass sealed checks for arbitrary SetRetime.
   - Prove the helper changed no other subtree timing. Current
     `take_exact_edit` is a leaf-local audited escape used by exact pause and
     Hold-duration paths, not blanket permission to mutate clocked content.
   - A valid FollowSpeed target cannot own a physical output binding:
     `audio_binding.rs::validate_for` permits Retime node bindings only when
     `pitch.processes(recipe_unity)`. Thus this affine route can preserve the
     full binding inventory and need not use generic reduce's output removal.

2. `crates/deadpan-core/src/command.rs`
   - Dispatch supported clocked SetRetime to that exact helper before generic
     reduce, analogous to SetHoldDuration's `inside_clocked` dispatch.
   - Keep ordinary unclocked SetRetime's established fresh processing-output
     policy unchanged; it is not the fix for derived clocks.
   - Preserve request revision checks, compound leaf allocation, atomic
     restore/validation, inverse and clock pruning. No new wire fields.

3. Exact host/semantic input resolution
   - Existing public `derived_clocks`, `exact_durations` and recipe access are
     enough to derive the exact input span without a new persisted type.
     Prefer one small pure core query returning the current exact output and
     selected child span if this prevents duplicate host/planner arithmetic.
     The query must include an ordinary stage with exact descendants, not just
     a stage with its own map entry.
   - `semantic/planner/retime.rs`: resolve update durations from exact child
     span; detect no-op against effective extent and current pitch, preserve
     staged target identity, trace end and continuation. Keep ordinary wrap
     semantics distinct.
   - `deadpan-app/src/project/retime.rs`: remove only the guard covered by this
     new capability query; change Resolution.input to exact rational (or add
     an explicit exact field) and keep preview/description arithmetic exact.
     Update service receipt/range calculations if they currently assume a
     rounded input. Existing root/beat selection and macro recording remain.
   - CLI macros already use the semantic planner. Public raw CLI SetRetime
     requires no schema change.

4. Outside physical clocks
   - Changing the selected stage's output moves later outer allocations. Keep
     descendant input clocks; do not reanchor them to new root timing as a
     substitute for the intended rate change.
   - Public retimed insertion currently captures unbound physical clocks
     globally, so the simple witness already has retained outside clocks.
     Still qualify partially bound later Sources, RoomTone and gaps. If they
     reset, use existing bounded pre-edit suffix preparation for ONLY owners
     outside the updated stage, with full old ordinal0 layout. Do not assume
     an old full inventory exists after arbitrary supported history.
   - `composite::occurrence_owners_after` and `prepare_owners` are existing
     candidates for that outer suffix. On the first Sequence-only family no
     transformed ancestor clock or expanded play traversal is needed. Do not
     broaden current insertion collectors to support an unrelated inner edit.

## Required witness and verification

Old-code public core witness:
- Construct the existing retimed test fixture, InsertTime root5/length3, then
  SetRetime slow/output15/FollowSpeed. Record exact refusal, request and before
  JSON. Repeating the identical existing stage parameters is a raw no-op
  control; semantic unchanged intent should retain its current no-revision
  refusal. Explicit WrapRetime around the complete clocked stage is the
  existing admitted alternative, not equivalent to updating its identity.

Core new-code matrix:
- Output15 grow/shrink relative to suitable starting lengths, output27 no-op,
  invalid zero/overflow, exact source and pause boundaries above, target
  identity, identical descendant clocks/binding entries, inverse and JSON.
- Stage with ordinary integral own geometry but fractional descendant clocks;
  first/last exact cut, nonzero child start, nested affine descendant stage;
  unrelated clocked branch unchanged. No document-wide refusal.
- Exact semantic speed7/8 resolves15 rather than16; named call/dot against a
  second staged target; trace/context end; late compound failure leaves the
  entire document/macro state unchanged. Store preview, commit, reopen,
  Undo/Redo and stale request refusal.

PCM oracle:
- Use 30000/1001 root frames,44.1kHz natural Source mapping with nonzero native
  span start3100, mixed impulses/ramp/speech fixtures and unaligned root lead.
- Capture independent native input coordinates for the pre-edit original
  Source fragments, then evaluate the NEW affine rate with the old retained
  fragment entry phase. Do not compare old complete output after arbitrary
  stretching or align events. The new right fragment entry is root16/3;
  its absolute output allocation starts round_even((16/3)*8008/5)=8542.
- Derive native read step from the new slope9/10 and44100/48000, i.e.
  1323/1600 native samples per mix sample. The retained source fragment phase
  comes from the old exact split's independent source map, including its
  old RootRoundEven allocation residue, not a newly rounded source boundary.
- Compare at least256 literal samples at each surviving fragment entry and
  selected interior windows against an independently authored scalar/resampler
  reference; compare outside Source/room/gap clocks at their exact translated
  allocations. Check every sample at finite crop edges against CURRENT support,
  not an uncropped or old-support stream. Test chunk independence and repeated
  edits, preserving original binding history.
- Run a real qualified-media picture/PCM export through the public macro and
  raw command, emitted-frame count/endpoints and zero event alignment. This
  affine slice needs no Signalsmith equivalence claim.

## Explicit non-goals / remaining boundaries

- Preserve/Shift parameter updates on a clocked family are not qualified by an
  affine extent edit. `derived_clock::effective_durations` currently refuses
  processing with retained bindings/lineage unless its distinct owned
  projection conditions hold; `FrozenAudioLayout::require_follow_speed_clocks`
  likewise protects retained processing output. Do not clear bindings or
  delete pause clocks to make pitch processing admissible.
- No mutation of `PreserveInputProjection`, no output-route reconstruction,
  no changed processing subtree attachment transport.
- No new Split/Trim/provider editing inside exact clocks, no variable rate,
  reverse, arbitrary nested ancestor-duration propagation, Repeat occurrence
  isolation, or selected-play batching in this slice.
- No schema migration is necessary from the scoped design above. Existing
  owned output and affine derived types must retain their current meaning.

## Relevant source / authority

- Spec `docs/spec/DEADPAN_SPEC.md` §4.4, §8, §10.5.
- `docs/RETIME_EDITING.md`: existing SetRetime child-selection contract, fresh
  processing-output policy, exact pause update limitation.
- `docs/spec/AGENT_HANDOFF.md`: remaining edits inside clocked changes and
  separate changed-processing-scope sound lifecycle.
- `derived_clock.rs::clocked_owners`, `retimes_with_derived_clocks`,
  `sealed_preserved`, `sealed_recipes_exact`, `effective_durations`.
- `insert_time/retimed.rs::{clock_of,set_clock,take_exact_edit,set_hold_duration}`.
- `audio_binding.rs::validate_for`; `audio_reference/exact_clock.rs`.
- `semantic/planner/retime.rs::{resolve_speed,apply_retime,pitch}`.
- `deadpan-app/src/project/retime.rs::resolve`, service `ProjectEdit` retime
  integration, and `project/tests/retime.rs` exact-pause refusal test.
- `deadpan-core/tests/insert_time/retimed.rs` and `tests/retime_edits.rs`;
  `deadpan-audio/tests/stages/retime_edits.rs` for independent fresh-DSP controls.
