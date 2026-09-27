# Projected Preserve output on the timeline

`AudioProjectedRoot` places an [intrinsic Preserve projection](AUDIO_INPUT_TAPES.md)
on the absolute 48 kHz timeline grid. `StageAudio::read_projected_root` prepares
the complete projected input history and returns actual time-mapped PCM with
explicit suppression ranges. It is a borrowed evaluation handle for one
immutable plan. It does not author a splice, install a persisted route, or
replace ordinary root-plan rendering.

## Allocation and sampling

Construction takes an `Arc<AudioStageProjection>` and `AudioRootPlacement`.
The placement's positive local support must fit within the projection's
intrinsic output duration. Signed origins and positive exact scales are allowed.
Its root extent is `origin + local_support * scale`; allocation uses absolute
ties-to-even endpoints at project rate. A positive exact extent can own zero
output samples. The intrinsic preparation still uses PointCeil storage and
the original Preserve rate, independent of final allocation length.
Both selected and full retained policy endpoints must fit the signed sample
carrier. Geometry that passes this check can still exceed the reader's admitted
resampling rate or phase; the reader rejects that recipe before media work.

The handle retains separate values for:

- The original meaningful root extent and its rounded support.
- The current exact allocation and its rounded output sample range.
- An exact `AudioSampleMap<AudioSample>` into the intrinsic output.
- An integer continuation into the original root-policy grid.

`crop` selects a positive subrange of the current exact allocation without
changing sampling phase, kernel support, policy support or processing history.
`resume(old_cut, new_extent)` maps the new rounded allocation start to the
current map at `old_cut`. The cut must lie within the current sample allocation,
including its end. Repeated resumes compose the existing phase. They do not
reconstruct it from the original video coordinate or change the sample step.
The new extent selects output allocation; it does not infer a new stretch rate.

Each physical domain needs its own continuation. At 30000/1001 fps, a pair of
two-frame domains illustrates why a single suffix offset is wrong:

| After inserting one frame at frame 1 | New root sample | Retained root sample |
| --- | ---: | ---: |
| Interrupted A resumes at frame 2 | 3203 | 1602 |
| Following B begins at frame 3 | 4805 | 3203 |

A's resumed allocation ends at 4805, so it has one more sample than its retained
remainder. Sample 4804 maps to A's exhausted old endpoint and is explicitly
silent. B's first sample retains its own phase, including its fractional local
offset. No sample count is used to fit either domain to its new allocation.

## Policy and preparation

The independent exact output-policy tape is mapped into the original absolute
RoundEven grid before rounding. Scaling already-rounded PointCeil masks would
lose a short Hold that owns no intrinsic point but gains output samples.
Source endpoint masks belong to the input: they must not erase processed decay
after Preserve. Explicit silent Holds still suppress its output.

Crop and resume retain that root-policy view. Each query translates its current
sample labels into retained labels with checked wide arithmetic, clips against
the original meaningful support, evaluates current policy, and translates the
result back. Exhausted support is explicit suppression, not a new provider or
an invented silent input. Kernel context continues to use the full prepared
intrinsic buffer, including outside the current allocation.

Construction shares the tape's bounded policy expansion. Reads accept 1 through
256 signed output samples inside the handle's allocation. They use the existing
source identity checks, full hidden-history preflight, deadline, cancellation,
depth, work and residency budgets. Projected preparations retain their request
identity and never enter the ordinary descriptor cache. Foreign plans, invalid
ranges and unsupported processing fail explicitly. Results identify
`projected_root_pcm_before_effects`; edge fades, voice effects, mixing and
mastering remain separate responsibilities.

The handle and its output metadata have no deserialization admission path.
Inspection does not recursively serialize the projection graph. Coordinates
and matching node names alone cannot recreate its plan ownership.

## Integration still required

Core schema 25 and database schema 31 are unchanged. The compiler does not yet
derive this handle from persisted splice intent. Exact effective owner clocks,
owned route persistence and lifecycle transforms, strict replay, aggregate
output scheduling and native nested insertion remain required. The enclosing
group must remain an actual live ancestor of an inserted Hold; a physical audio
placement cannot establish that picture/effect ownership.

See [structural splice design](STRUCTURAL_SPLICE_DESIGN.md) for the complete
remaining contract and [qualification](qualification/projected-root-2026-09-26.md)
for tests, independent review and delivery evidence.
