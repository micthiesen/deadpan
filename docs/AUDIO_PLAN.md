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

`boundaries.start` and `boundaries.end` retain the constraints that formed those
full extent edges: structural node boundaries, Source placement boundaries and
Repeat gap boundaries. Each origin has its own complete occurrence path; an
outer Retime trim does not acquire a descendant play identity. Gap origins also
retain the preceding stable play ID. All constraints meeting at exactly the
same project coordinate remain in outer-to-inner descent order. Coordinates
that merely round to the same sample are not coincident. A query crop changes
neither the full edge coordinates nor their owners.

For example, trimming into the middle of a Source through a Retime retains the
Retime boundary rather than attributing that new cut to the Source's original
start. A placement start also names the end of the preceding out-of-placement
silence; the origin kind describes the original constraint's side, not the side
of that silent span. Each origin captures its owner's authored policy in this
revision. Any exact coincident `hard` suppresses that edge's one fade; defaults
do not override an explicit exception. [Shared edge processing](AUDIO_EDGES.md)
applies these choices after time mapping. The plan itself performs no DSP.

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
combined node/comparison and boundary-copy work at 65,536; callers can set lower
limits. Each captured origin costs one unit plus its repeated-ancestor count,
charged before cloning even if a tighter constraint later replaces it. This
bounds coincident path storage through nested structures. The existing lookup
counters describe traversal; they do not include boundary-copy work. Exhaustion
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

Tests cover original placement sides, coincident constraints, nested trim/gap
ownership, exact versus rounded coincidence, boundary-copy admission and stable
override identities at a billion-play seek. Existing partition/property tests
also compare the complete origin metadata across query boundaries.

Initial boundary provenance at `d1348a5` was independently reviewed with no findings. On 2026-09-23,
the repository format/Clippy/test/build/doctor gate passed, including all 798
tests. The tested plan sources remained unchanged throughout the gate. This is
headless planning evidence. The subsequent [edge stage](AUDIO_EDGES.md) records
its separate implementation and checks; neither establishes GUI or listening
qualification.

This API establishes structural planning only. Separate readers connect these
spans to [source preparation](AUDIO_PREPARATION.md), [continuous retime stages](AUDIO_STAGE_PREPARATION.md)
and [room-tone loops](ROOM_TONE_AUDIO.md). Room-tone records retain full intrinsic
Hold duration, including repeat-gap duration, independently of query or ancestor
crops. Attachment voices, effect routing, incremental fragment reuse, tails,
gain/limiting, background cache scheduling, native device integration and full
preview/export playback remain required work.
