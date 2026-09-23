# Authored audio edges and shared fades

Each `BeatNode` retains six `audio_edges` policies: `node_start`,
`node_end`, `source_placement_start`, `source_placement_end`, `repeat_gap_start`
and `repeat_gap_end`. Each is `automatic` or `hard`. JSON omits the policy object
when all six are automatic; an absent object means that default. A present
object requires all six fields, and null/partial/unknown values are rejected.
This keeps default migration within the existing snapshot/request/patch byte
limits. Node edges apply to every
primitive. Only a Source may carry a nondefault placement choice, and only a
Repeat may carry a nondefault gap choice. Removing selected audio or a gap
retains that owner's settings for later reuse.

`SetAudioEdge { node, edge, policy }` changes one choice through the normal
revision-checked reversible transaction. The `SetAudioEdge` occurrence operation
isolates repeated ancestors in the same transaction, copying existing edge
intent with the owned nodes. A direct node edit affects all its occurrences.
Repeat gap settings apply to every gap of that authored Repeat. They do not
claim individual-gap editing. Ungroup refuses a Sequence with explicit edge
exceptions until they are reset, so removing the wrapper cannot silently lose
them. Deletion removes the deleted node's choices with its other authored state.

Direct policy changes preserve temporal coordinates, marks and presentation
basis. Occurrence edits preserve timing while copying or relocating owned marks
under the existing isolation rules; their mark inventory may grow. Edge choices
do not lock an empty provisional project's frame rate. Core schema 11 and
database schema 16 introduced this intent, retained in database schema 17. Database schemas 1 through 15 replay
their complete history through strict frozen adapters and gain automatic edges.
Legacy snapshots, command subtrees and forward/inverse patches reject the new
fields, including null values, and legacy commands reject both direct and
occurrence edge edits. Migration retains its consistent pre-upgrade backup.
Database 15 uses frozen core schema 10 and keeps its existing basis state and
presentation transactions. Database 16 already uses core schema 11 and keeps its authored edges. Database 17 adds the optional single-Original profile without inventing one for legacy projects. Operational media/generation rows are unchanged.

## Exact ownership and precedence

The immutable [audio plan](AUDIO_PLAN.md) captures the policy on every retained
boundary origin. The origin's kind identifies the original constraint's side,
including where a placement start ends the preceding silent span. A trim into
the middle of a Source belongs to the trimming Retime, not the Source's old
start. Interior child cuts do not inherit noncoincident ancestor edge choices.

If several constraints coincide exactly, any explicit `hard` disables that
edge's one fade. `automatic` supplies the default and cannot cancel another
owner's explicit exception. Constraints that merely round to the same sample
are not coincident. Multiple automatic origins produce one envelope, not
stacked attenuation. A future explicit force-fade policy would need a separate
precedence design; it cannot be added to this two-state rule implicitly.

## PCM contract

`StageAudio::read_edge_faded` uses the same continuous time/pitch preparation as
`read`, then applies one envelope to each flattened voice's root sample
allocation. It leaves prepared Preserve and room-tone caches before effects.
Internal loop crossfades receive no additional fade. Explicit silent Holds
remain numeric zero and retain their suppression ranges for later effects.
Unsupported tails and other unimplemented audio recipes still fail explicitly.

The engine is `deadpan-voice-edge-sample-centered-linear-2ms-v1`. For allocation
length `N`, sample offset `i`, and width `F = min(96, N/2)` at 48 kHz:

```text
start gain = automatic ? min(1, (i + 0.5) / F) : 1
end gain   = automatic ? min(1, (N - i - 0.5) / F) : 1
gain       = min(start gain, end gain)
```

Both stereo channels receive the same gain. The width is independent of which
edge is hard, so changing the other edge does not reshape this edge. Integer
distances are bounded before conversion to floating point. One-sample fragments
retain unity; two-sample fragments with both automatic edges use `[0.5, 0.5]`.
This avoids applying a fixed 96-sample attenuation to arbitrarily tiny clips.
It is a specified short-fragment compromise, not a guarantee that every possible
one-sample event is click-free. Fades allocate no extra samples, introduce no
latency and do not change source level away from the edges.

Envelopes use full `allocated_samples`, never the requested chunk endpoints.
Partitioned reads and fresh cropped reads therefore match the same full render.
An authored trim is different: it creates a new structural edge and a fade at
that new edge. Continuous time-mapping history remains unchanged beneath it.
The envelope uses the existing read deadline/cancellation checks and bounded
256-sample output block; it starts no worker or additional preparation pass.

The returned `EdgeFadedBlock` identifies
`edge_faded_pcm_before_voice_effects`, its engine and fixed implemented order
`[time_pitch_mapping, edge_fades]`. The raw `TimeMappedBlock` contract remains
`time_mapped_pcm_before_effects`.

```sh
cargo run --locked -p deadpan-cli -- inspect-audio /tmp/example.deadpan --samples 0 256 --edge-faded
```

The same command runs through `deadpan-app --headless`; it opens one immutable
revision read-only. `--edge-faded` and `--time-mapped` are exclusive inspection
stages. Authored hard choices use the normal `command --json` entrypoint, for
example the command member `{"command":"set_audio_edge","node":"word",
"edge":"node_end","policy":"hard"}` in a complete revision-bound request.

## Verification and remaining scope

Core tests cover strict current serialization and default migration without
JSON growth, kind validation, reversible
edits, provisional basis preservation, nested occurrence isolation, retained
settings and guarded ungrouping. Plan tests cover exact owner-policy capture
and immutable revisions. Audio tests compare scalar envelopes over real
prepared PCM, continuous Preserve child cuts, mixed nested retimes, room tone,
authored crops, placements, repeat overrides and tiny fragments. Headless
integration exercises decoded AAC, silence and unchanged stored history.
Migration tests use authentic historical SQL, including database 15 generated
twice with the pre-change revision, and reject new vocabulary in old histories.

On 2026-09-23, the final repository format/Clippy/test/build/doctor gate passed
with 816 tests and no failures; the tested sources remained unchanged throughout.
General, PCM and migration review covered the change. Review caught default
policy serialization growth near the existing JSON cap; omitting automatic
objects fixed that without raising the limits and passed re-review. The initial
full gate also exposed an older CLI negative fixture missing the new object;
the compact default now preserves its intended domain error. No findings remain.
An earlier isolated audio doctest attempt failed to locate `deadpan_media`;
its retry and the final full workspace doctests passed.

This implements the edge stage for existing sequential source/room-tone voices.
Native authoring controls and keyboard bindings, listening qualification,
simultaneous attachment voices, full effect-order authoring, gain/EQ/saturation,
sends and tails, group/master processing, application playback and export remain
open. The earlier failed limiter experiments remain unadopted. This stage is
neither a limiter nor the final mix, and it establishes no GUI-quality evidence.
