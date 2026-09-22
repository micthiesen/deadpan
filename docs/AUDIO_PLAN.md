# Exact structural audio plans

`RenderPlan::audio` inspects one immutable revision over a half-open range of
48 kHz project samples. The result partitions that range into authored source
voices, absent/out-of-placement source silence, silent Holds, room-tone recipes,
and tail recipes. It performs no decoding or DSP. Renderers must implement a
recipe explicitly or reject it; these records do not make effects playable.

Every returned span retains its complete stable occurrence path, preceding play
identity for a gap, exact leaf-to-project affine transform, and outer-to-inner
Retime stages including each pitch policy. Source mappings retain the selected
original span, independent exact placement/duration, and signed audio offset.
An offset is applied in the Source's local clock before enclosing retimes.
Placement is clipped to the Source and all enclosing selection windows; it does
not spill into a neighboring beat or stretch to fill an empty interval.

## Common-origin sample allocation

For an absolute exact project-frame boundary `f`, allocation is
`B(f) = round_even(f × 48000 × fps_den / fps_num)`. A span owns `[B(a), B(b))`.
No local rounded durations are summed. `allocated_samples` and `project_extent`
retain the entire structurally clipped span; `samples` intersects it with the
query. Query chunk size cannot change the origin, source mapping, occurrence,
pitch policies, or allocated boundaries.

The structural search uses the exact point `(n + 1/2)` in sample units when
resolving sample `n`. At an exact structural tie it chooses the right side for
even `n`, and the left side for odd `n`. This is the inverse of ties-to-even
boundary allocation, not a DSP interpolation position. It skips intervals that
round to zero samples without enumerating them. DSP coordinates remain at the
requested exact sample boundary. A rounded edge can map slightly outside an
authored source interval; `source_point` reports that fraction without clamping
or permitting a decoder read outside measured availability. Edge extension,
resampling, fades and DSP context remain explicit renderer responsibilities.
`source_point_at_project_frame` also maps exact fractional structural edges,
without replacing them with rounded sample allocations. The
[source-stage reader](SOURCE_STAGE_AUDIO.md) uses these edges to constrain
filter context and uses the full allocated span as its phase origin.

Sequences use prefix binary search. Repeats use the shared compact
`RepeatLayout`, including sparse overrides, variable play durations and gaps
only between plays. Each next span starts with another indexed descent rather
than expanding intervening plays. Queries cap returned spans at 4,096 and
combined node/comparison work at 65,536; callers can set lower limits. Exhaustion
fails the whole query explicitly. Empty in-range queries return no spans;
negative, reversed and out-of-plan ranges fail. Arithmetic overflow fails
instead of rounding an intermediate. Compilation storage remains proportional
to authored nodes, sequence edges, compact repeat runs and sparse overrides.

## Headless inspection

```sh
cargo run --locked -p deadpan-cli -- inspect-plan /tmp/example.deadpan --audio-samples 0 48000
```

The same command is available through `deadpan-app --headless`. It opens SQLite
read-only and reports protocol 1, the pinned project/revision, spans and measured
lookup counters. It neither renders audio nor changes history.

This API establishes structural planning only. Separate readers connect these
spans to [source preparation](AUDIO_PREPARATION.md), [continuous retime stages](AUDIO_STAGE_PREPARATION.md)
and [room-tone loops](ROOM_TONE_AUDIO.md). Room-tone records retain full intrinsic
Hold duration, including repeat-gap duration, independently of query or ancestor
crops. Attachment voices, effect routing, incremental fragment reuse, tails,
gain/fades/limiting, background cache scheduling, native devices and full
preview/export playback remain required work.
