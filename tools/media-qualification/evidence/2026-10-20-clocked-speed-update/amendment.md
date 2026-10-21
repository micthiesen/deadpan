# Amendment after the old-code witness and independent review

2026-10-20. Read-only investigation, no repository edits/Cargo/Git. Root executed
the retained witness on base25f40767, binarySHA256
8953b78419ecc393d1e1b58180fece0cefd5e7e717616a05c5330b4295fb997e.
The source-derived final sealed-guard prediction in design.md/REGISTRATION.md
was too late. This amendment supersedes it and the flattened PCM proposal.

## Observed refusal and precise source trace

The retained `old-witness/observed.json` says InvalidCommand:

`this part of the project is inside a speed change with exact pause timing, which this view or command cannot use yet`

The positive ordinary update and public fractional pause baseline passed in
root's run. Root subsequently executed the raw unchanged-clocked-stage control:
PASS, duration27/FollowSpeed, nodes/clocks/bindings/inverse unchanged and delta0
(`noop-old.log`). Root also executed the extreme retained transfer witness:
StageAudio refused transfer103 with
`Transfer(Preparation(InvalidRecipe("source step outside 1/64..64")))`, although
the native slope45423/4000 is below64. These executions belong to root, not this
read-only scout. No DSP bound change is proposed.

The message comes from `derived_clock::unsupported`, via
`ProjectDocument::require_integral_clocks`. It is not the more specific final
`sealed_preserved` error predicted before execution. Source tracing identifies
why the recorded fixture reaches that earlier check:

1. `command.rs::apply_with_durations_captured` enters `with_sealed` for this
   derived-clock document. Its initial gate at approximately1193 runs on the
   unchanged before state. Recipe/effective counts outside sealed descendants
   agree there: slow27 and root31. This initial gate is not sufficient evidence
   that a changed result can use integral post-processing.
2. Generic reduce writes slow.recipe.duration15, retaining its effective
   derived.extent27. `old-witness/before.json` confirms original slow.recipe27,
   selected `[0,27/2)`, with lineage owners `inner` and `p-2`.
3. Generic post-reduce `audio_lineage::reconcile_shared` sees that nonempty
   lineage. It calls `command::shared_structure(after)` (or structural_durations
   directly in the reference path), before final sealed_preserved.
4. `structural_durations` calls `require_integral_clocks`; its sealed exception
   calls `sealed_recipes_exact`. slow is the top clocked owner and therefore
   NOT a strict sealed descendant. Its recipe15 disagrees with effective27;
   root recipe19 likewise disagrees with effective31. The exception fails and
   produces the observed generic view refusal. Even without lineage, generic
   mark reconciliation's local path asks shared_structure before transforms.

This is a precise source path consistent with the retained outcome, not an
instrumented runtime stack. If root needs runtime location proof, instrument
only an archived scratch copy of require_integral_clocks to distinguish its
initial caller from post-reduce reconciliation. Do not modify current source
just to accept the old predicted message.

## Dispatch and raw no-op

The audited path must be dispatched BEFORE generic reduce/reconcile and must
never expose the integral-recipe/exact-extent mismatch as an intermediate.
Reuse the exact Hold-duration/pause pattern: validate first, construct coherent
new recipe+effective clock together, call reconcile_exact rather than integral
lineage/mark transforms, validate, and provide the exact-edit receipt only for
this proven leaf. Keep the final sealed guard for every unaudited command.

Handle an ordinary Edit Retime's identical raw recipe duration/pitch BEFORE
rejecting unsupported ancestry or requested processing. An unchanged request
must not remove output bindings, modify derived clocks, or introduce a new
capability refusal. Preserve WrongNodeKind for a Partition; do not turn a
same-shaped Partition request into a new no-op success. Revision/project checks
still run first. Semantic unchanged intent retains its existing no-revision
behavior separately.

Recommended supplemental OLD control: apply raw
`SetRetime { slow, duration27, FollowSpeed }` to the retained before document.
Root has now confirmed that admission and exact unchanged state. The source
explanation is that its result never creates the intermediate mismatch. Also
test raw no-op under unsupported changed-edit ancestry before reordering guards.

## Shared requested-pitch-aware pure queries

Separate cheap exact timing from parameter admission, but share both across
native preview, service, Inspector and semantic planning:

- Exact timing returns selected input span and effective output as ExactRatio,
  using derived geometry when present and ordinary recipe geometry otherwise.
  An ordinary stage with fractional descendants is a clocked family too.
- Parameter admission accepts the REQUESTED duration and pitch. Its result
  distinguishes unchanged raw request, ordinary update, audited exact-affine
  update, and the existing truthful unsupported case. Do not advertise
  Preserve/Shift simply because a FollowSpeed update is supported.
- Speed intent resolves once from the exact selected span. Native
  `project/retime.rs::resolve` currently lacks requested pitch; add that input
  or funnel its result through the same core admission query. Semantic
  resolve_speed/apply_retime must use the same result. Pitch-only intent cannot
  use rounded recipe speed to accidentally retime the stage.
- Inspector `preview/inspector.rs` currently uses mapping.duration/duration for
  BOTH visible Speed and Change speed prefill. Replace both with the shared
  exact fraction. Before update the witness opens as1/2; after output15 it
  opens as9/10, never14/15. Opening/reapplying that exact prefill is unchanged.

No wire type or persisted primitive is needed. A small nonserialized core
timing/admission result is enough. The actual command helper must re-run the
same validation against its revision rather than trusting a host prediction.

Proposed minimal public contract for root to approve before implementation:

```rust
impl ProjectDocument {
    pub fn retime_update_input(
        &self,
        node: &NodeId,
        requested_pitch: PitchPolicy,
    ) -> Result<ExactRatio, EditError>;

    pub fn validate_retime_update(
        &self,
        node: &NodeId,
        duration: FrameDuration,
        requested_pitch: PitchPolicy,
    ) -> Result<(), EditError>;
}
```

The first returns exact eligible selected input; the second checks raw Edit
no-op first, then eligibility and prospective carrier rates. The existing
exact_durations query supplies output for display/prefill. Keep the private
ordinary-versus-exact dispatch decision in core. Root may rename these APIs;
no code or new type contract has been implemented by this scout.

## Transfer admission grounded in retained geometry

The existing two-stage reader has TWO resamplers. `audio_bound.rs::bound_at`
computes (approximately193):

    U = resolved.lattice.local_frames_per_sample()
    C = current signal frames per physical-local frame
    F = current signal frames per mix sample
    transfer_step = (F / C) / U

`StageAudio::read_bound_at` feeds this transfer_step into
DomainSignalTransfer/RootSignalTransfer, whose ResampleRecipe admits positive
steps only in `[1/64,64]`. This rate is reference samples per current output
sample. It is NOT the final native coordinate slope, the requested speed, or
merely old authored output/new output.

Use pure core reference geometry to prove it before commit:

- `AudioBindingState::resolve_in` / `resolve_gap_in` supply the ACTUAL retained
  lattice and U. Resolving only the immediately preceding authored stage is
  insufficient; original lattices can survive many parameter updates.
- `FrozenAudioLayout::project` supplies current frames_per_local_frame. For
  a top-of-family affine update with unchanged input selection, a descendant's
  NEW scale is `C_new = C_current * requested_output / current_effective_output`.
  Therefore no candidate clone or provisional integral timeline is needed
  merely to predict rate. Compute `(F / C_new) / U` with checked exact math.
- Current and historical Root/Point clocks share project frames per mix
  sample `fps/48000`; their origins/grid rounding affect phase, not this rate.
  Retained resume/phase/reanchor terms affect the anchor, not U. Keep all of
  them intact. Do not remove/rebase lattices to fit the rate budget.
- Inspect only the changed family's relevant physical owners. Outside suffix
  owners only translate and need entry-clock preservation, not a new rate cap.
  Do not reject an unrelated branch because another stage has a derived clock.
- Existing media/native-rate admission remains distinct. Bound playback's
  first raw resampler uses its historical native step and its second uses the
  transfer step. A composed native step alone cannot substitute for checking
  those separate recipes, or justify inventing a new composed-step limit.

Concrete extreme witness (root has executed the transfer103 failure): Source12/output100,
then a3-frame public pause. The exact updated stage is extent103 and selected
309/25; the physical Source's original historical root scale is25/3.

- Update to output30: actual transfer103/30 is in bounds.
- Then update to output1: actual transfer103 is out of bounds, although the
  immediately preceding authored ratio30/1 is only30. Final native slope can
  still be below64. Admission must refuse without writes before audio fails.
- Slow-direction dual: output5120 has actual103/5120 in bounds; subsequent
  output10000 has103/10000 below1/64, despite adjacent ratio5120/10000 in bounds.
- Test inclusive endpoints64 and1/64 with exactly representable fixture
  counts, just-outside ratios, older lattices after repeated updates, and a
  prior clipped/resumed binding. Keep no-op before this new changed-edit check.

These are capability limits already enforced by the audio reader, not arbitrary
speed policy. Root's executed failure now justifies atomic precommit refusal
of the unsupported carrier rate. The sequential-history and inclusive endpoint
controls still need execution after implementation.

## Partial bindings: align query and committed carrier inventory

Preferred decision: capture old unbound inventory before changing the stage,
instead of mixing fresh-after-update sampling with historical carriers. Keep
every EXISTING binding byte-for-byte. Add old-entry anchors only to the newly
captured owners; do not append reanchors to existing descendants for this edit.

An unbound cropped Source needs its old visible AllocationEntry as well as its
old lattice. If its fresh lattice defaults to local0 but its visible Partition
starts at local3/2, changing rate changes the rounded distance from local0 to
the allocated entry. A new entry anchor freezes the old entry's actual sample
coordinate before applying the new transfer. Existing public pause carriers
already retain their explicit split/resume history and must remain untouched.

Implementation can reuse existing capture/preparation narrowly:

- Gather pre-edit physically unbound node/gap owner IDs under bounded graph
  traversal. `composite::prepare_owners` captures unbound inventory globally;
  passing ONLY those unbound owners as affected appends AllocationEntry only
  on the new carriers, and inert ones compact normally. Use the full old root
  window and complete old ordinal0 timing layout, retaining existing limits.
- Install that prepared binding state in the unpublished coherent exact edit,
  then validate the new carrier rates against the ACTUAL installed state.
  Do not use a prior freshly rounded post-edit origin or clear any old carrier.
- The pure query does not need to allocate fake timing identities or clone a
  candidate document to predict those fresh units: each unbound root owner
  will capture `U_fresh = F/C_before`. Its prospective transfer is therefore
  `E/N`. Existing owners instead use their actual resolved historical U, which
  can give a very different rate after many earlier updates.
- Only changed-family slopes need prospective checks. Global newly captured
  outside owners have unchanged slope and unit, hence unity carrier transfer;
  outside placement/history still gets the appropriate old-entry anchor.
- Snapshot tests must assert every old OwnedAudioBinding unchanged, show new
  unbound carriers and their old visible entries, and compare query prediction
  with the committed reader's reference_samples_per_output_sample. Add partial
  left/right removal plus a nonzero clipped entry and old fractional resume.

Budget/validation failures still refuse before publication. Pure rate eligibility
does not promise unknown future revision-ID wire length will fit; actual
identity-bearing preparation enforces the existing full byte/work limits.

## Compact Repeat slope proof if the first slice includes them

The native blocked fixture has no Repeat inside the family, so one resolved
current instance per physical owner suffices there. Repeated PARAMETER UPDATES
must still compare the original actual lattice, as above. Do not confuse that
history obligation with supporting new repeated-occurrence edits.

For Repeat families, there is a bounded symbolic extension, not play expansion:

1. Current physical ancestry has a fixed affine scale per owner. Sequence and
   Repeat placement only translate; sparse owned branches have distinct owner
   paths. Default gaps have their own recipe/path. Retime factors are exact
   `extent/(child_end-child_start)` from current geometry.
2. A lattice may choose different historical clock roots on birth. In
   AudioPlacementTemplate::resolve_with, the INNermost failing birth clause
   wins; a gap birth wins over all. Thus possible reference units are bounded
   by original scope + each birth clause + optional gap birth, not the Cartesian
   product of plays. Resume terms do not add transfer-rate states.
3. Check reachability of those finite states from current compact ancestry.
   `IterationOrder::segments()` gives allocation/first/count runs; subtract
   sparse override keys for default uses, retain the one fixed identity for an
   owned branch, and intersect survivors from CapturedRepeat or explicit Run.
   A default whose current uses are all overridden has no live cohort.
4. For each reachable state, construct one representative InstancePath and
   reuse resolve_in/resolve_gap_in plus frozen project scale; alternatively
   extract a private slope-only core helper sharing clock_scope/project's
   exact ancestor factors. Do not assume a first or last play represents every
   birth state. Prove old vs new reference units for each reachable state.
5. Current selected windows can hide whole runs. If admission filters by live
   output, intersect exact Repeat segment allocation intervals with that window
   using floor/ceil/run arithmetic before choosing a representative; do not
   scan to the first visible play. Preserve dormant clocks untouched.

This proof work is bounded by graph, binding clauses, compact segments and
sparse overrides under existing work limits. It is not already a complete
library API today; factor it narrowly if Repeat families enter scope. The
first Sequence-only stage update can retain the existing refusal for unsupported
Repeat/nested outer-clock edits without any document-wide guard.

## PCM correction from independent review

The earlier composed1323/1600 native slope is coordinate arithmetic, not a
bit-exact PCM oracle. The retained pipeline first resamples current finite
native Source on its OLD literal lattice, applies current support and masks,
then sinc-transfers that discrete signal onto the NEW grid, then applies current
output policy/envelopes. Two sinc stages cannot be replaced by one in the test.

For the NTSC44.1k witness use old native step147/320, old left reference3203,
old right reference8008, and new right allocation8542; transfer is9/5.
Independently reconstruct both stages, finite crops and suppression. Compare
literal samples and chunk independence without event alignment or old complete
output equality. Keep plan geometry and semantic rounding tests separate.

## Exact production paths for the next implementation

- core `insert_time/retimed.rs`: coherent exact SetRetime helper, existing
  clock_of/set_clock/reconcile_exact/exact-edit receipt; do not run integral
  post-reduce transforms for its unpublished state.
- core `command.rs`: narrow SetRetime dispatch beside exact Hold-duration
  dispatch, request checks and raw unchanged behavior retained.
- core `derived_clock.rs`: shared pure target timing/capability query if kept
  with existing geometry ownership; existing sealed checks remain default.
- core `audio_binding.rs` and `audio_reference.rs`: reuse actual lattice
  resolution and frozen current projection; only add a bounded cohort/slope
  helper if needed. No dependency on deadpan-plan/audio in core.
- core `semantic/planner/retime.rs`: exact selected input, requested pitch
  admission, accurate no-op/trace/staged continuation.
- app `project/retime.rs`, service retime integration and
  `preview/inspector.rs`: same query for preview, apply, display and prefill.
- tests: core public retimed witness plus rate/no-op controls; semantic exact
  7/8, repeated updates and late refusal; native prefill/apply/recording; PCM
  literal two-stage oracle; store durable atomic refusal and inverse/history.

Preserve/Shift owned projections, binding deletion, broad sealed-content
bypasses, variable speed and changed processing-scope sound transport remain
outside this affine slice. No new runtime success is claimed by this amendment.
