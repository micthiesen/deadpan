# Next duration scope after the current pending increment

The current one-Repeat/one-fixed-Retime increment remains unverified pending
root's compile and execution. This note recommends a subsequent increment only.
No repository source was changed while preparing it.

## Recommendation

Allow two explicit This-play Repeat body choices, with one fixed unclocked
purpose Edit Retime below both Repeat ancestors. All other ancestry remains
Sequence. Keep mixed Default/Play choices, selected counterparts, gaps as the
target, Partition, derived clocks, sounds, additional Retimes and a Retime
between the two Repeat ancestors outside this next increment.

This is smaller than mixed scope because it reuses the current
SemanticScopedContext, EditOccurrence::SetHoldDuration, actual temporal
isolation predispatch, fixed visibility, optional intrinsic boundary and native
capture route. The compact geometry projector already handles effective body
children, stable Repeat layout offsets and multiple Repeat steps.

## Public old-code control

Root = Sequence[Lead3, Outer x3, Tail3]
Outer literal body = Inner x2
Inner literal body = Stage(output6, input[0,8), tape speed, purpose Edit)
Stage child = Body[Before2, Held2, Speech6]
Selected branch = Outer second stable Play, Inner second stable Play
Requested edit = Held2 to4

Original duration is 42f:

3 + 3 * (2 * 6) + 3 = 42.

The selected Stage starts at 21:

3 + 1 * 12 + 1 * 6 = 21.

Hold before = [45/2,24), sampled [22,24).
Hold after = [45/2,51/2), sampled [22,25).
Continuation = 22, because the center at 22.5 belongs to In.
Speech live origin before = 24; after = 51/2.
The Source recipe's retained old lattice origin must stay24, slope3/4.
No project duration, outer/inner Play start, fixed output or mapping changes.

Perform a real public EditScoped Rename control at
ScopedNodeTarget(Held, [Outer:Play(second), Inner:Play(second)]):

- scoped_edit_requirements must return exactly11 node IDs and0 mark IDs.
- Outer isolation clones the Inner subtree: Inner + Stage + Body + three
  physical leaves =6 nodes.
- Inner isolation then clones its Stage subtree: Stage + Body + three leaves
  =5 additional nodes.
- prepare_scoped_edit must report two actual isolation steps.
- Obtain the mapped Hold from prepared.target and map captured Held, Speech,
  Stage and Inner addresses with map_target/map_instance.
- Call ordinary public SetHoldDuration on the mapped Hold.
- Assert raw duration_delta0, project42, unchanged original definitions,
  unchanged sibling Play durations and identities, correct nested overrides,
  fixed Stage6/[0,8), exact geometry and sampled interval, both inverses and JSON.
- Use the silent structural fixture for native admission only. Fresh silent
  placement lattices move the chosen Speech live origin; they do not prove
  retained audible phase. Use separately qualified nonzero Source PCM for
  retained lattice and export evidence.
- The sole last positive native assertion should expect availability true.
  Current code must fail there only after the public controls pass.

Native inspector navigation should use real scope methods, not mutate captured
authority: create State(root Outer,cursor22), switch_play_in(2), enter Inner,
switch_play_in(2), enter Stage, enter Body, select Held. The copied inner Repeat
ID must come from the actual isolation mapping after commit.

## Current guard seams

crates/deadpan-core/src/scoped_edit/hold_duration.rs:

- fixed_occurrence_preflight requires instance.repeats.len()==1.
- Its single repeat/saw_repeat state admits one Repeat and insists the one
  Retime appears before reaching it during the leaf-to-root walk.
- Generalize to exactly two Repeat steps for this next scope, consumed
  innermost to outermost, checking each effective stable body child. Keep the
  only Retime strictly below the innermost Repeat.
- Return the outermost captured Repeat as inspector root, preserving public
  parent/root validation.
- prepare_fixed_command and prepare_occurrence_hold_duration already retain
  a vector of real isolation steps and map_instance over each. Their structure
  needs no new wire command or context.

crates/deadpan-app/src/preview/scoped/model.rs:

- this_play_fixed_retime_path_supported currently destructures exactly [step].
- Generalize the cached parent walk to two ordered explicit Play choices and
  their actual effective body roots. Keep it bounded by selected ancestry.
- project/scoped.rs's fixed duration capture can retain its current public
  fixed_occurrence_hold_duration_visibility query and endpoint-picture checks.

The ordinary SetHoldDuration fixed-window reducer requires no speculative
change. Since the one Retime is below both Repeats, root ripple is stopped
before either Repeat; only the chosen Stage's local suffix moves.

One existing refusal must be updated deliberately: in
semantic/planner/tests/scoped_hold_duration.rs,
scoped_hold_duration_noop_mixed_nonhold_and_retime_fail_before_allocation wraps
the inner A in one fixed Retime below two explicit Repeats. That shape would
become supported. Make its refusal genuinely outside the next scope, such as
three Repeat ancestors or a Partition, rather than preserving the old guard.
The current Outer -> Stage -> Inner refusal stays unsupported if this next
increment keeps the fixed Retime below both Repeat ancestors.

## Why mixed scope comes later

The same fixture with [Outer:Play(second), Inner:Default] would require only6
clones, but changes both Inner plays in one outer Play. It also needs changes
in more policy layers:

- mixed_body_dormant calls duration_target_dormant, which rejects all Retime
  ancestry and carries dormant-definition/gap liveness policy.
- mixed_hold_duration_visibility requires an integral intrinsic start and
  computes end as start plus intrinsic duration.
- prepare_scoped_hold_duration resolves the intrinsic AnchorIndex start;
  left-cropped live Holds need separate sampled visibility from dormant None.
- Mixed semantic continuation/trace and native cached hints must use that
  visibility while preserving Default meaning and dormant authority.

The nested explicit increment therefore has fewer policy changes despite its
larger exact isolation pool. It also strengthens the actual outside-in
isolation proof needed by a later mixed implementation.

## Verification risks

The chosen fixture has no unrelated sparse overrides or gaps. Add independent
owned siblings and nonuniform earlier outer/inner durations after the first
valid old-code witness to prove stable layout offsets rather than ordinal
multiplication. Also check fresh11, pre-owned outer5, fully owned0, staged
second setter0; first/last stable choices; reorder; cropped starts; center ties;
late sampled disappearance; sound/derived/Partition refusals; exact inverse,
durable history and nonzero PCM. Do not infer success from the current pending
single-Repeat run.
