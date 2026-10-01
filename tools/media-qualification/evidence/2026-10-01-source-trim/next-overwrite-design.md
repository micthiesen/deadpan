# Overwrite Source edge Trim: proposed next contract

Read-only design, 2026-10-01. No implementation or runtime checks performed.

## Authority and unresolved choices

The normative spec requires Trim with explicit ripple/overwrite policy (§6.3),
one typed command path with prospective changes (§6.1, §6.4), and boundary
pictures, waveform, keyboard refinement, visible handle clamps, one Enter commit
and Escape restoration (§7.7). Partial structural edits retain their contexts
and do not flatten unrelated composites (§6.3). Linked roles are the default
(§6.5). Lift explicitly leaves same-duration blank/silent time (§6.3).

The spec does **not** spell out overwrite edge algebra, whether extension can
grow the project, whether it can cross the selected group, or the independent
sound policy of an overwrite-created gap. These are proposed defaults, not
additional quoted requirements:

- Keep the selected ordinary Sequence's extent and every ancestor's extent
  unchanged. Extension consumes adjacent time inside that explicit scope;
  contraction leaves blank/silent time. Do not silently reach outside a group.
- Clamp at that scope's start/end as well as Original handles. At the root these
  are project bounds. Show a separate scope-boundary reason. Project growth can
  use ripple; any later overwrite-growth policy should be explicit.
- Use a fresh `HoldVideo::Background` + `HoldAudio::Silence` filler for a
  contraction. Do not infer a freeze, room tone or a permitted tail.
- Preserve independent root sound recipes, phase and routes. The new silent
  Hold nevertheless suppresses their output by the existing default policy.
  Do not grant allowances to the new issuer. Whether Lift/overwrite should keep
  independent music audible is an open product compatibility question; the
  existing explicit silence/allowance policy is the defensible default.

`docs/SOURCE_TRIM.md` describes the current Ripple-only increment. Its target
restrictions are implementation limits, not permission to narrow full Trim.
Broader nested/treated targets, repeated occurrences and audio-only picture
lead/tail remain required work where the full editing language calls for them.

## Exact fixed-time behavior

Let target output be `[T,U)`, named scope output `[S,Q)`, physical allocation
`C=[c0,c1)`, editorial window `W`, and effective selection `E=W∩C=[a,b)`.
Positive integer `d` moves the named edge later in Original material, as today.

| Operation | Target output after | Allocation before physical prefix | Adjacent effect |
| --- | --- | --- | --- |
| In, `d>0` | `[T+d,U)` | `[c0+d,c1)` | Fill `[T,T+d)` |
| In, `d<0` | `[T+d,U)` | `[c0+d,c1)` | Overwrite `[T+d,T)` |
| Out, `d<0` | `[T,U+d)` | `[c0,c1+d)` | Fill `[U+d,U)` |
| Out, `d>0` | `[T,U+d)` | `[c0,c1+d)` | Overwrite `[U,U+d)` |

The target duration changes by `-d` for In and `d` for Out. The scope/project
duration change is **zero**. Distinguish these two values in resolution and UI;
do not retain Ripple's ambiguous single `duration_delta_frames` meaning.
Zero preserves the complete representation and produces no transaction.

All ranges are half-open. The retained target content has the same absolute
position: for an In edit,

```
old global(x) = T + (x-c0)
new global(x) = (T+d) + (x-(c0+d)) = old global(x)
```

After a physical prefix `p`, use `x+p` and `c0+d+p`; the same equality holds.
Out leaves the existing affine placement unchanged directly. Earlier and later
surviving neighbors also keep their absolute placements.

Reuse current `source_trim::candidate` semantics for effective W edges,
fractional padding, hidden context, linked audio intersection, grow-only Source
duration, source-map translation, owner effects and independent audio offset.
Never refit the full measured span into the new allocation. Keep the physical
Source ID; a neutral Partition is allocation, not a new processing edge.

### Bounds

Intersect current exact Source limits with scope limits **before** integer
clamping. If picture support is `[h0,h1)` and target length is `L=U-T`:

- In lower bound: `max(h0-a, S-T)`, inclusive.
- In upper bounds: `L-1`, inclusive, and `b-a`, exclusive.
- Out lower bounds: `1-L`, inclusive, and `a-b`, exclusive.
- Out upper bound: `min(h1-b, Q-U)`, inclusive.

Use the existing inclusive/exclusive inward ceil/floor rules and checked
arithmetic. At least one delivered frame and positive exact selection remain.
Retain the exact rational limit, integer interval, requested/applied d and
actual clamp reason. At equal bounds, retain all limiting reasons or choose a
documented deterministic reason while still reporting the exact interval.
Audio handles do not constrain picture trim: audio may become dormant or
audible; intentionally absent audio stays absent. Unsupported target/neighbor
structure is an explicit failure, not a silent clamp to a convenient seam.

## Structural implementation and reuse

Extend `SourceTrimMode` with `Overwrite` and keep one `Command::TrimSource`.
Resolution should additionally expose the changed adjacent range, whether it
is fill or overwrite, affected child IDs, endpoint Split count, filler need,
scope limits and zero project-duration delta. This keeps preview, CLI and
future native Trim on the same resolved operation.

A small command extension can retain `wrapper` and `timing`, adding an optional
fresh filler ID and a `SplitIdentities` pool. Validate cross-pool uniqueness,
freshness, exact filler/wrapper need, allocation revision, temporary peak nodes
and all existing mark/binding budgets. A zero preview accepts no structural
identities. No new frozen audio vocabulary is needed. The serialized command
grammar must receive the normal format checkpoint; migrations are not required
for unused development packages under this session's authorization.

Suggested reducer order:

1. Resolve against the immutable input and preflight the final affected range,
   media, identities and resource budget before mutation.
2. Detach the root sound bus through the guarded outer command path. Capture
   previously unbound audio lattices from this unchanged tree.
3. For extension, split only the far neighboring endpoint that is interior to
   a child. The original target edge is already a seam. Keep full retained
   contexts, binding copies, effects, Repeat identities and mark fragments.
4. Replace the adjacent selected child interval and update the target together
   in one working result. Contraction inserts the blank/silent filler beside
   the shortened target. Extension removes the overwritten neighbor fragments.
   No shorter deletion-only intermediate clock becomes authoritative.
5. Apply target physical prefix/effect/binding translation once. Finish lineage
   and mark transformation from the split working tree, restore sounds and
   allowances, prune unreferenced bindings, validate the final document, then
   create the normal forward/inverse transaction.

Useful existing helpers:

- `source_edit::admit`, `source_trim::{limits,candidate}` for qualified target
  geometry. Add scope bounds and mode-specific output placement.
- `sequence_range::{split_endpoints,selected_children}` and
  `split::{apply,partition}` for retained endpoint contexts and crop wrappers.
- `insert_time::split_node_count` for bounded identity needs.
- `audio_binding_lifecycle::capture_unbound_audio_bindings`,
  `OwnedAudioBinding::rebase_local`, `audio_lineage::reconcile` and `prune`.
- `SoundAllowanceEdit::{split,restore}` and
  `marks::transform_marks_with_source_prefix` after any neighbor Split.

The current `source_trim::apply` receives only the allocation revision. Neighbor
Split needs the outer `EditContext` so its allowance remapping reaches the
captured allowance state. Pass that context through the atomic Trim branch,
as replacement/move already do; a locally fabricated context with no allowances
would lose permissions on surviving copied Hold issuers.

Two reuse traps need explicit treatment:

1. `sequence_range::preflight_with` currently routes partial endpoints through
   `physical`/`slice_physical`, rejecting partial Repeat, nonunity Retime,
   Sequence and generated Hold endpoints. Whole middle composites are allowed.
   Generic `split::apply` already retains/copies complete contexts. Add a narrow
   overwrite preflight using that retained-context capability, with tests for
   each admitted composite. Do not claim arbitrary endpoints merely because
   the low-level Split exists; qualify provenance, budgets and DSP behavior.
   A staged refusal must remain an explicit unfinished capability, not a new
   specification restriction.
2. Calling public `ReplaceSource`/`ReplaceSlice` is not a suitable implementation:
   they install new/copied content identities, capture moved suffix clocks and
   edit root sound routing. Share their endpoint/removal mechanics instead.

Empty structural siblings at the adjacent range's endpoints should survive;
strictly interior empty siblings retire with the overwritten range, matching
`selected_children`. Insert a contraction filler immediately beside the
explicit target without using a time-only guess that loses empty identities.

## Audio clocks, sounds and marks

**No new target or suffix reanchor is needed for overwrite.** The proof is the
unchanged local-to-global map above. `deadpan-plan::audio_bound::bound_at` takes
its current reference boundary from `B(transform(local_anchor))`. Existing
resume/chronological phases remain unchanged. Prefix rebasing translates the
anchor and transform in opposite directions, leaving that boundary unchanged.
Capture old unbound lattices before any edits; do not append Ripple's target
or suffix steps. Split copies retain their historical bindings. There is no
reason to keep a phase-only current layout when all owners are already bound.

Compute all exposed intervals as `[B(start),B(end))`. Do not compare equal
frame durations using independently rounded sample lengths. Current selected
source support still controls filtering independently of the retained sampling
phase. If W changes, filtering near its changed endpoint can change; unchanged
timing does not promise identical PCM across a newly changed operand boundary.
If W stays unchanged, neutral Partitions add no fade/filter edge and preserve
hidden filtering context. Existing/new owner gain and framing clocks retain
the current Source Trim rules. Ancestor durations and clocks stay unchanged.

Use the **MoveRange-style exact detach/restore branch** in
`RootSoundEditCapture::prepare`, including final root-extent validation.
Returning `None` merely because overwrite has no root time operation would
leave sounds attached and fail the soundless binding-capture boundary.
`RootSoundOperation::Replace` is wrong even at equal duration: its routing map
removes sound support inside the replaced range. Keep recipes/routes byte-exact
and add no journal operation. Final live Hold gates still apply:

- New filler has a fresh silent issuer and no inferred allowances.
- Overwriting an old silent Hold can reveal retained root sound output where
  that gate disappears; source phase continues through both states.
- Split remaps existing concrete allowances only to retained contexts.
  Removing an issuer retires its relation; unrelated permissions remain.
- Undo restores previous sounds, issuers and relations exactly.

Generic mark loss policy remains authoritative. Split must first preserve its
boundary-biased fragments, then final removal retires/marks unresolved only
lost fragments. Prefix translation applies only to physical Source content
points; Source PTS, fixed Sequence coordinates and host edge sentinels keep
their semantics. A retained physical mark can stay bound behind a crop while
the concrete occurrence becomes unavailable. Do not move lost marks to filler
or new target content, and do not revive unresolved marks on re-extension.

## Admission, preview and recovery

Extend `ProjectStore::preview_source_trim` and its admission validator rather
than adding an authorization token. Recheck current revision, immutable asset,
stored full-span receipt, selected ownership and every declared structural ID.
Copied neighbor contexts retain current admitted media; no edit-time hashing
or decoding is needed. Return resolution plus optional transaction, including
for clamped zero; raw zero commit still refuses before a write.

Commit re-resolves the original request once. Revision, history and cursor
remain one SQL transaction. Undo/Redo use fresh revision IDs; inverse restores
exact authored state. Preserve existing saved-receipt/reopen-warning behavior
if refreshing the view fails after the durable save. Failed admission, stale
target, exhausted budget or SQL fault leaves the input/history intact.

Native Trim still owes §7.7's outgoing/incoming boundary pair and waveform,
Tab cycling, visible ripple/overwrite state, h/l and Shift steps, handle/scope
clamp reason, proposed affected interval, one Enter commit and Escape restore.
The Source backend alone does not complete that workflow.

## Tests that can refute the design

1. Four cases from the table in nested ordinary scopes. Assert unchanged root
   and group extents, untouched sibling starts, exact fill/overwritten ranges,
   opposite target edge, fresh filler and retained physical Source IDs.
2. Fractional W/picture origins and VFR handle bounds, minimum exact width,
   integer inward rounding, scope/project clamps, zero and overflow. Dormant
   linked audio can activate; absent audio never gains a mapping.
3. Whole and partial neighbor Source/Hold, treated/nested Partition, Sequence,
   Repeat play/gap and Preserve Retime. Assert complete context retention,
   no flattening, stable occurrence references and peak identity limits.
4. Independent decoded PCM with two chunkings, reverse query order and reads
   capped at 256. Unbound, prebound, resumed and chronological reanchors,
   physical prefixes 1/3, 48 kHz and 44.1 kHz, nonzero audio offset, dormant
   activation, filtering support and owner gain/framing. Compare retained body
   at its original absolute sample positions, including suffix Repeat/gap and
   Preserve output. Check raw and edge-faded PCM separately.
5. Reuse the current chronological fixture as a strong negative control:
   before, body begins at project frame 4, sample 6406, Original phase 4861
   after subtracting offset 7 once. Overwrite In -3 with physical prefix 1
   begins at frame 1, sample 1602. Its new prefix phase is
   `4861-(6406-1602)=57`; retained body is still at sample 6406/phase 4861.
   Ripple's existing phase 56 is deliberately wrong here. Compare against
   `source_oracle`, not plan-derived timing, and restore exact inverse PCM.
6. Root sounds with an existing routing journal and allowance: recipe/route
   equality, unchanged phase outside gates, new filler suppresses sound, no
   automatic allowance, overwritten Hold gate disappears, partial Split
   retains only its issuer relations, and Undo restores exact audible behavior.
7. Marks at each join with both biases, inside removed material, behind a
   retained crop, exact Source PTS, empty siblings and already unresolved
   fragments. Verify genuine loss rather than time-based reassignment.
8. Store preview/commit/reopen/fresh Undo/Redo, immutable historical reads,
   duplicate identities, stale revision, wrong receipt/current asset, SQL
   rollback and successful-save/failed-refresh receipt. Preview and committed
   boundary pictures/PCM must agree for the same immutable proposal.

## Subsequent editorial-edge correction

A final Trim audit found that retained sampling transparency alone does not satisfy the new-cut fade contract. The planned Roll/overwrite operation must record the changed incident editorial edges independently of raw filtering support and retained phase. Preserve unchanged continuous joins and exact Hard precedence. These notes predate that correction and are design input, not implementation evidence.
