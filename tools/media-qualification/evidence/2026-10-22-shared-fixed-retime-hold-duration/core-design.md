# All-Default Hold duration through fixed Edit Retimes

Read-only source-based design. No implementation or runtime claim.

## Public contract

No new context, command, schema or allocation.
Retain SemanticSharedHoldDurationContext {root,target,presentation} and
ProjectDocument::shared_hold_duration_visibility(parent,authority)
 -> Result<Option<HoldDurationVisibility>,EditError>.
The captured root remains a direct ordinary-parent Repeat child.
The target must follow Default at every Repeat ancestor. Some presentation is
the same actual stable instance, never a sampled replacement. None requires a
positive fully-overridden default-body proof; crop invisibility is not dormancy.

Only extend the existing All-Default duration authority. Ordinary no-Repeat,
one-Play, mixed, selected, owned gap, generic framing and Visual gates remain
their existing contracts. Keep the raw sound lifecycle refusal and native
no-sound availability gate. Keep ParameterEdit allocation and raw
SetHoldDuration once, no cloning or temporal occurrence wrapper.

## Factor, do not duplicate geometry

Extract ordinary_hold_duration_visibility's current bottom-up exact interval
walk into one private duration-only helper in semantic/planner/hold_duration.rs.
Tentative private name:
  hold_duration_visibility_for_instance(document, inspector_root, instance)
    -> Result<HoldDurationVisibility,EditError>.
Keep public authority checks in ordinary/shared wrappers. This helper builds
the exact extent map, parent index and sealed-stage set and projects one already
validated Hold instance. If convenient, pass those precomputed maps instead of
rebuilding the shared validator's parent index; no public framework is needed.

Ordinary wrapper retains its explicit empty Repeat instance and direct fixed
Edit Retime root. Therefore adding a Repeat geometry arm to the helper does not
grant ordinary authority inside Repeat.

Shared wrapper extends its complete ancestry admission to fixed unclocked Edit
Retimes below its direct Repeat inspector root. Check all stages' purpose and
derived-clock exclusion even for positively dormant definitions before returning
None. A Retime above the inspector root remains refused by ordinary-parent/
scope_bounds ancestry, not silently omitted.

## Exact compact Repeat translation

Use the existing core ExactRepeatLayout, not integer durations or ordinal math:
  ExactRepeatLayout::compile_with_gap_overrides(
    iterations, child, document.overrides().get(repeat),
    ExactRatio::integer(default_gap_duration), document.gap_overrides().get(repeat),
    &exact_extents
  )?.play(&instance.repeats[step].iteration)?
and require returned play.child == current traversed literal body.
Add play.start to both interval endpoints. Include sparse preceding body and
owned gap overrides and current stable identity order. This API's segments
already merge IterationOrder runs and sparse maps; no expanded play loop.
Owned gaps at preceding plays affect the offset, but an All-Default target does
not gain authority over those gap branches.

Each Repeat on this single ancestry is compiled at most once. Aggregate sparse
entries on distinct ancestors are bounded by the compact graph, and order runs
remain bounded by authored segments. A million-play test must stay compact.

## Clip/scale/sampling

Start with exact [0, Hold.recipe.duration). Ascend:
- Sequence: add sum of preceding children's exact extents.
- Repeat: add exact captured play.start as above.
- Retime: intersect with [mapping.start,mapping.end), subtract mapping.start,
  multiply by output_duration/mapping_duration. Purpose must be Edit; stage
  and any descendant clock sealing it must not have an exact derived pause
  clock. Physical audio bindings alone are not sealing clocks.
- Root: intersect/limit only through the established exact interval contract;
  use the existing ordinary picture-center sampling rule once:
  f + 1/2 in [In,Out) => [ceil(In-1/2), ceil(Out-1/2)), clamped to root frames.
  Start tie is included, end tie excluded. No RoundEven boundary substitute.

Refuse empty clipped or nonempty-but-unsampled LIVE intervals initially and
after each staged setter, atomically. The latter may be detected after a private
staged leaf, before a final semantic command is published. Do not fabricate a
new representative when its stable occurrence becomes invisible.

## Baseline arithmetic

Lead3 + Repeat3(Stage output6, input[0,8), child Before2,Held2,Speech6) + Tail3:
root24. Second stable play starts9. Scale3/4.
Hold local [2,4) -> root[10.5,12), pictures[10,12).
Hold2->4 gives local[2,6) -> root[10.5,13.5), pictures[10,13).
Cursor10 remains10. Stage output6, Repeat18, root24 unchanged. Raw setter
handles physical input clocks; continuation must use exact visibility rather
than intrinsic local-zero Anchor plus authored duration.

## Proposed ownership and files

Core:
- semantic/planner/hold_duration.rs: factor existing geometry + Repeat arm.
- semantic/planner/shared_hold_duration.rs: bounded all-Default/fixed-stage
  ancestry and call shared geometry; no allocation/reducer change.
- focused new shared-duration test child + module registration.
- narrowly reconcile existing shared test whose valid Edit Retime guard is now
  superseded, preferably change it to Partition or derived-clock refusal while
  adding genuine positive coverage.

Native owner:
- cached hold_duration_available ancestry gate for All-Default only.
- captured Target::semantic_shared_hold_duration and service final receipt use
  unchanged core query, preserve actual picture checks at first/last samples.
- model/replay tests including retained stable occurrence and dot/recording.

PCM/store owner:
- reuse actual existing raw fixed-window captures through outer default Repeat,
  including partial historical bindings, 44.1k and all admitted pitch policies.
- accepted Hold shrink vs extension fallback, atomic preview/commit, history,
  close/reopen, macro bank and exact sampled continuation.

Root: baseline identities, Cargo/fmt/Git, independent review, CLI/export/docs.

## Essential independent cases

1. Exact baseline first/last representatives; repeated/count/nested Calls through
   newly captured bindings. Zero node/mark pools and unchanged stage/Repeat/root.
2. Nonzero crop, clipped Hold beginning/end, nested nonunity stages, start/end
   half-center ties, nonempty unsampled interval and post-shrink disappearance.
3. Preceding sparse body override + owned gap, reordered stable IDs and nested
   Repeat prefixes. Retain same instance; exclude overrides from definition edit.
4. A fully overridden default inside an otherwise valid fixed stage: None,
   preserve cursor/output, update dormant definition. Clocked or Partition
   ancestor must still refuse even if the definition is dormant.
5. Existing bound/partial physical clocks do not make authority one-use.
   Root/beat sound refusals, Original/Visual/conflicting authorities,
   stale/forged/omitted Repeat ancestry and unsupported expanded instruction
   remain atomic.
6. Million sparse play count: geometry and timing proportional to compact graph.
7. Ordinary no-Repeat validator remains negative for Repeat addresses; mixed,
   one-Play and selected fixed-Retime timing remain negative, not routed through
   shared authority.
