# Live audio input projections

`AudioSignalTape` is a borrowed preparation view of one immutable render plan.
It projects exact windows of current structural signals or projected intrinsic
Preserve output onto one preparation clock. `StageAudio::read_tape` consumes it
through the existing Source, RoomTone, binding and Preserve renderer. A checked
`AudioStageProjection` supplies separate intrinsic input and output-policy tapes.
These views supply actual PCM without authoring a splice or allocating final
timeline samples.
These borrowed views do not change core schema 28 or database schema 34.

## Coordinates and support

Each `AudioSignalTapeRun` has an exact destination window, exact source window
and an `AudioSignal` carrying its current definition and occurrence scope.
The increasing affine map is:

```text
source = source_start + (destination - destination_start)
         * source_length / destination_length
```

Runs must cover the complete input support exactly, in order, with no overlap
or hole. They all belong to the same plan. The input grid is PointCeil, with
its origin at the full support's start and spacing equal to project fps/48000.
Runs select providers on that grid. They never concatenate separately rounded
lengths, reset phase at a seam, or introduce a per-run sampling-support crop.
The full support is a meaningful input selection; existing narrower support
remains effective after mapping. A positive run may own no input point.
As in ordinary stage reads, selecting prepared Preserve output changes demand
without trimming that stage's canonical history or intrinsic filter context.

At 30000/1001 fps, an input cut at frame 3/2 lies at sample 2402.4. The first
right-side point is label 2403. Its exact frame coordinate remains 12015/8008,
rather than restarting at 3/2. Final project allocation continues to use its
separate absolute RoundEven grid.

## Reading and admission

Queries retain the underlying span's sampling anchor, allocation and complete
filter support. Only the requested sample interval is restricted by a run.
Definition-relative and actual occurrence identities remain distinct. Source
lookup, retained bindings and compact Repeat traversal use the selected current
signal; a tape does not graft an unscoped node into another occurrence.

Dispatch comparisons and underlying traversal share the query's work allowance.
Content queries also cap the aggregate returned spans. Policy retains the
existing per-logical-query span cap, with its aggregate content and suppression
storage bounded by shared work; the count of silence masks is not a structural
span budget. PCM reads share one deadline, cancellation state, source
observations and stage budget
across all runs. Unsupported policies fail before source I/O for the requested
block. Existing nested Preserve stages prepare their full unchanged intrinsic
input and output, and recheck dependencies on cache hits. A tape does not add a
second DSP engine. Unchanged stages retain their ordinary descriptor cache;
projected preparations use the distinct request-local memo described below.

`TapeAudioBlock` labels its result `projected_preparation_pcm_before_effects`.
Its `SignalSample` positions are temporary preparation labels, never root output samples.
Reads are bounded to 1..256 points and reject a handle from another plan, even
when that plan contains an identical revision.

## Intrinsic Preserve projections

An [ordered scoped mix](SOUND_EVENTS.md#borrowed-mixing-boundary) can combine
complete tapes on the same intrinsic grid. `AudioStageProjection::new_mixed_input`
accepts that mixed operand with the same independent output-policy tape and
scope checks described below. `input_tape()` and `input_mix()` expose the two
exclusive input kinds; common support and sample-count accessors avoid assuming
that every processing input is one sequential tape.

`AudioStageProjection::new` binds an immutable current scoped Preserve stage to
its original output duration, an input tape covering `0..duration*rate`, and an
independent output-policy tape covering `0..duration`. The authored rate remains
the current stage's rate. Provider windows choose child-local coordinates;
the intrinsic tape clock is normalized to zero. All providers must belong to
the same plan and the stage's child subtree, retaining definition and concrete
Repeat scope. Nested projection providers must be strict descendants. Construction
is bounded and the immutable graph is acyclic.

`AudioSignalTapeRun::intrinsic` selects a window of a projection's complete
canonical output. A parent input tape refers to that intrinsic output directly.
A separate schedule can interleave that output with current Hold providers.
The inserted pause then bypasses the old stretch history at every level. This
does not replace a current authored stage in the ordinary root plan.

Policy follows the independent output-policy tape, remapped as exact intervals
to the consuming grid before allocation. Never scale rounded input masks: a
positive silent Hold can own an output point while owning no input point.
At a Preserve output boundary, physical Source endpoint masks no longer apply,
so processed decay survives. Explicit silent Holds still suppress output.

Each public PCM request retains one memo keyed by opaque projection identity.
Two routes through the same stage descriptor cannot alias. A nested result is
prepared once per request, even when a parent reads it in many chunks. Reuse
rechecks source admission and depth. Memo bytes, normal cache bytes and active
preparations share the same residency limit; all preparations share work,
cancellation and the deadline. Input and output policy are preflighted before
source I/O, including nested projected inputs omitted from output policy.
Source rechecks retain the existing independent limit of 65,536 observations
per request. Frame, stage and memory limits are upper bounds, not a guarantee
that every combination of their maxima will pass all admission limits.
The memo is discarded with the request and does not provide playback scheduling
or a persistent prepared-audio cache.

## Remaining splice integration

This is the input reader needed by the [splice design](STRUCTURAL_SPLICE_DESIGN.md#preserve-input-projection-design).
An authored route still needs subtree ownership and bounded validation, strict
schema/replay admission, copy/split/isolation transforms, and current-recipe
dependency identity. No route is persisted or accepted by an edit command yet.

The borrowed projection supplies intrinsic preparation and a PointCeil output
schedule. [Projected root placement](AUDIO_PROJECTED_ROOT.md) separately evaluates
one physical projection on the absolute RoundEven timeline grid, retaining
sampling phase, root policy and explicit exhaustion across crops and resumes.
Fractional effective owner extents, persistent route identity, edit transforms,
aggregate output scheduling and normal root-plan integration remain open.
A document compiler must derive these views from validated authored intent
before arbitrary nested insertion can use them.
