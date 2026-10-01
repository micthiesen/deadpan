# Source Trim editorial edge audit

Read-only audit, 2026-10-01. No code edits or runtime execution.

## Finding: default Trim currently permits an unintended hard cut

This is an actual contract gap, not a test-only mismatch.

- Spec §10.4 requires short fades, normally 2 ms and shortened for tiny
  fragments; hard discontinuity is an explicit creative choice.
- `docs/AUDIO_EDGES.md`, PCM contract, explicitly distinguishes an authored
  trim from a transparent partition: a trim creates a new structural edge and
  fade while continuous time-mapping history remains unchanged beneath it.
- `docs/AUDIO_PARTITIONS.md` similarly says real new cuts need intentional
  edge semantics; transparency exists to preserve Split/splice context.
- `source_trim/apply.rs` constructs or changes only a neutral Partition when
  contraction leaves W unchanged. It authors no new audible edge.
- `bound_reads/source_origin/gain/trim.rs:460` confirms this behavior: default
  Source policies, In +1, unchanged W, and raw PCM equal to edge-faded PCM at
  the newly exposed entry. The raw equality is valuable filtering/phase
  evidence. The edge-faded equality enshrines the wrong editorial behavior.

The source-origin/filtering design and the default-fade requirement are
compatible. Allocation, raw sampling support and creative edge envelopes must
remain separate. A transparent Partition alone should still add no fade.

## What the current implementation can and cannot express

`AudioEdgePolicy` currently has only `Automatic` and `Hard`. There is no
`Fade96`, custom-width or force-fade policy. The 96-sample maximum is the shared
automatic DSP rule.

Explicit nondefault policies on a Partition are **not** already honored:

1. `document.rs:973` rejects a Partition unless timing is unity and all policies
   are Automatic.
2. `deadpan-plan/src/audio.rs:495` skips a Partition's envelope constraint and
   boundary origins entirely.
3. `audio_signal.rs:931` omits Partition constraints from raw sampling support.
4. `audio_bound.rs:182` retains the binding's historical sample reference.
   `audio_fades::bound_fade` follows its retained envelope domain. Nothing
   converts a newly shortened allocation into an editorial boundary.
5. `StageAudio::read_queries` applies retained-domain silence, then the creative
   fades returned by `audio_fades`. The DSP is not overlooking an available
   edge; the plan has no authored edge to return.

Removing the validator restriction alone would still leave the policy ignored.
`Automatic` does not mean an explicit fade at an otherwise nonexistent edge.

## Smallest correct direction

Add explicit **one-sided editorial edge intent**, carried independently of the
neutral allocation and selected sampling support. A narrow optional start/end
edge marker on the relevant allocation owner is sufficient in principle. Reuse
its existing `node_start` / `node_end` policy values for Automatic/Hard instead
of adding a force-fade value. The exact wire shape is an implementation choice;
the required separation is not.

- Unmarked Partition sides retain current transparency, including Split's
  complete raw sampling and envelope context.
- Trim marks the changed audible join's relevant sides. A newly authored edge
  defaults to Automatic. A later explicit Hard change remains possible at
  that concrete owner/side.
- Preserve existing policies. At exactly coincident boundaries, any existing
  Hard still wins under `AUDIO_EDGES.md`. A noncoincident Source or ancestor
  Hard must not be copied onto a new interior edge. Do not introduce a force
  fade or silently replace explicit intent.
- A marker affects creative output envelopes only. Keep W, selected Source
  support, raw filter taps, retained binding/resume/reanchor, gain and framing
  clocks unchanged except for the already intended Trim transforms.
- Copy/retain the marker with its meaningful owner when Split retains complete
  contexts. A marked crop must no longer qualify for a refinement that would
  discard that owner's context. Re-trimming updates the appropriate side;
  Undo restores it exactly.

This does require a small authored representation/plan change. No existing
policy-only mutation provides the missing meaning today.

### Important plan boundary

Do not insert the new constraint into the current `audio_walk` envelope path
without separating its sampling use: `audio.rs` passes that envelope as
`BoundPlacement.support`; `audio_bound.rs` uses it to restrict the raw domain.
That would accidentally narrow the filter context this increment preserves.

Carry the new creative constraint separately through the fade query and bound
domain handling. Derive its progress on the consuming output clock, starting
at the actual `B(edge)` sample, independently of an old binding's fractional
source phase. Preserve retained old envelopes for sides that did not change.
Merge automatic origins without stacking multiplicative attenuation; the
existing contract is one envelope and exact Hard precedence. Tiny-fragment
width and overlapping edge behavior need explicit scalar expectations.

`StageAudio` already has a separate creative-fade query/application boundary,
so no new decoder, resampler, Preserve preparation stage or PCM cache is needed
merely to apply an admitted editorial edge.

### The changed seam can affect the neighbor too

Consider one Source split transparently into A=[0,5), B=[5,10). Before a new
edit, that seam must be sample-continuous. Trim A Out -1 gives A=[0,4), B=[5,10).
The new join is discontinuous. Fading only A's outgoing samples still leaves
a jump from near zero to B's unfaded incoming sample. Both incident audible
sides of a newly changed join need their edge intent considered, including a
neighbor whose boundary was previously transparent. Existing true edges and
explicit Hard exceptions retain their policies.

Conversely, trim A In while its Out remains the continuous join at5. That
unchanged Out join must not acquire a fade merely because A was edited.
The current test's silent lead exposes only the target-In half of this issue.
At a nested scope boundary, inspect the actual audible predecessor/successor
through ordinary ancestors; do not infer that the immediate child list has no
neighbor. Dormant or intentionally absent audio does not manufacture a voice.

## Fixes to avoid

- **Narrow W to force a Source placement fade.** This also changes filtering
  operands and editorial selection, conflating separate contracts.
- **Change all Partitions into meaningful edges.** This breaks Split and
  retained-context PCM continuity.
- **Turn each Trim wrapper into an ordinary Edit Retime.** It narrows sampling
  support and adds both endpoint constraints. It creates an unwanted fade at
  an unchanged transparent opposite join, and changes raw PCM.
- **Set Hard in the fixture to keep the present assertion.** A separate
  explicit-Hard witness is useful, but does not fix default Trim.
- **Change the raw source oracle to include fades.** Keep source sampling and
  creative attenuation independently testable.

## Required corrected evidence

1. Keep the current test's raw entry oracle and unchanged W/mapping assertions.
   For default edge-faded entry, multiply the independent raw samples by the
   scalar automatic ramp. At its long interval, sample0 gain is `1/192`,
   sample95 gain is `191/192`, and sample96 is unity. Use two chunkings, a
   mid-ramp query and exact inverse restoration. Source phase remains
   `8007/5` at this fixture's new entry.
2. Add symmetric Out and extension cases, including an existing neutral
   Partition. Check unchanged opposite-edge fade progress/policies.
3. Add transparent Split continuity before Trim, both-sided fade at a newly
   discontinuous join after Trim, and no new fade at the untouched join.
4. Add explicit Hard at the new owner side; exact-coincident ancestor/Source
   Hard; a nearby but noncoincident Hard that merely rounds to the same sample.
   Hard suppresses the intended fade without changing raw phase/filtering.
5. Reuse the resumed/chronological binding fixture: new editorial envelope
   starts at output sample0 while its Original sample phase remains nonzero.
   Include NTSC rounding, 44.1 kHz input and bounded reads.
6. Test tiny intervals, default edge shortening, query/chunk independence,
   nested ordinary scopes, silent/absent neighbors and retained root sounds.
   Structural Original edges do not become arbitrary master-bus fades.
7. Persist edge intent through commit/reopen, Split, fresh Undo/Redo and inverse
   patches. Correct `docs/SOURCE_TRIM.md` and the overwrite/Roll design notes:
   neutral allocation remains transparent, but authored Trim joins require
   separate explicit edge semantics.

Scope: this audit establishes the current Source Trim gap. Other structural
edits that use transparent retained contexts may warrant the same seam audit;
their correctness was not established or changed by this read-only review.
