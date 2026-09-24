# Structural picture plan

`RenderPlan::compile(&ProjectDocument)` validates and owns an immutable snapshot
of one committed revision. `metadata()`, `node_duration()`, and deterministic
`inspect()` expose its identity, presentation basis, duration, and storage
counts. `picture(ProjectFrame)` produces a serializable original-media picture
request for a project frame center. Callers can share this pure mapping between
preview, export, and headless inspection.

Sequences store cumulative child-duration boundaries and select them by binary
search. Empty Sequences retain their authored identities and duration zero but
are skipped during picture selection. Repeats store the child once and index
compact iteration identity runs; even `u32::MAX` plays do not allocate per-play
nodes. `LookupStats` records actual node visits and binary-search comparisons.
Picture descent is O(depth × log(max(children, iteration runs))).

Nested Retime maps retain `ExactRatio` coordinates through the entire structural
path. Sources map that exact coordinate into their original signed stream clock.
`FitBeat` preserves the historical span-to-beat mapping; `SourceVideoMapping::natural_rate`
records an exact video extent that preserves natural-rate timing even when the
beat duration rounds to integer project frames. The frame index is selected only through
`Picture::select_source_frame(index)`, which checks the asset and timestamp clock.
VFR selection uses the measured `SourceFrameIndex`, the selected span, and the
Source's persisted endpoint policy. Holding selects only frames intersecting the
authored span and never supplies missing index coverage. Freeze points reject
out-of-index timestamps. Accepted artifacts retain their exact original-frame
coordinate and floor only after all structural transforms, within the authored
half-open range. Missing accepted frames fail.

Each sample carries a core `InstancePath`. A normal sample targets its Source or
Hold and includes all repeated ancestors. A gap targets the Repeat node with
only its repeated ancestors; `gap_after` identifies the stable preceding
iteration. Both paths satisfy core validation. No gap follows the last play,
and iteration reordering changes positions without changing those identities.

`PictureSample.framing` retains every visited owner in provider-to-root order,
including identity scopes, with its `InstancePath`, exact output-local position,
duration and optional evaluated `FramingPose`. Owner envelopes use the shared
core evaluator: segment/time selection is exact and interpolated values follow
its declared numeric policy. A Retime's own operation uses its output clock;
its child operation uses mapped child time. Child Repeat effects reset per play,
while Repeat-owner effects span the full passage, including gaps. Gap framing
starts with the actual Repeat scope and requires the renderer's separate identity
provider clip. Keeping the full path lets a transient Camera replace the selected
operation in place without appending it after ancestor effects.

The tests cover negative/fractional source origins, exact sequence boundaries,
VFR lookup, nested retimes/repeats, accepted frame mapping, all picture provider
variants, stable gap identities after reordering, binary iteration-run lookup,
zero-duration Sequences, invalid seeks, billions of compact plays, immutable
revision behavior, inverse-compatible deterministic inspection, and explicit
exact-arithmetic overflow. Property tests compare indexed sequence and repeat
lookup against simple enumerated references for bounded generated documents.

```sh
cargo test -p deadpan-plan --locked
cargo clippy -p deadpan-plan --all-targets --locked -- -D warnings
cargo fmt -p deadpan-plan -- --check
```

This is a structural picture mapping layer, not a media renderer. It has no
decoders, pixels, GPU handles, audio DSP, pixel effects, attachments,
color transforms, cache fragments, incremental recompilation, transport, or
export implementation. Compilation currently rebuilds the full structural
index. Exact arithmetic is bounded by checked `i128` intermediates; an
unrepresentable nested mapping returns an error instead of rounding it.
