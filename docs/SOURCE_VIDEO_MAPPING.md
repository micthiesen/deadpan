# Exact source picture timing

Core schema 7 gives every Source a required `video_mapping`, independent of its
audio mapping and integer timeline duration. `fit_beat` preserves the historical
mapping of the entire selected span across the beat. `duration` maps that span
over an exact positive project-frame extent and records an endpoint policy.

`SourceVideoMapping::natural_rate(span, frame_rate, endpoints)` derives the exact
extent from original timestamp units and the rational project rate. The host
chooses the integer beat duration separately. Rounding the beat must never
silently fit the picture to that rounded duration.

For example, one second at 30000/1001 fps spans exactly 30000/1001 project frames,
which rounds to 30. With source time base 1/30000 and origin zero, project frame
15 has source center 31031/2 ticks. Fitting to 30 frames would request 15500 ticks
instead. A VFR boundary at tick 15510 makes these requests select different
original frames. Natural mapping retains 31031/2 through the render plan.

Both picture sampling and inverse source-anchor queries use the same exact
extent. Enclosing Repeat and Retime mappings compose without intermediate
rounding. Negative original PTS remain intact. Audio selection, audio mapping,
audio offset, node duration and original-coordinate marks remain unchanged.

## Selected endpoints

`duration` requires one of two explicit policies:

- `reject`: an out-of-selection picture request fails.
- `hold_adjacent`: it holds the first or last original presentation interval
  intersecting the selected half-open span.

An exact duration of 3/2 project frames can round to a two-frame beat. Its second
frame center lands on the selected source end. `hold_adjacent` holds the last
selected frame there; it never chooses a later frame from the full original.
A trim inside a frame interval includes that intersecting frame. A trim ending
exactly at the next frame's PTS excludes that next frame.

`Picture::Source` retains the selected span and policy. Call
`Picture::select_source_frame(index)` to enforce them against the measured index.
The asset and clocks must match, and the index must cover the entire selected
span. Holding cannot conceal a missing index endpoint or missing source media.
Freeze requests reject points outside the measured index. Accepted generated
frames retain their exact original ordinal lookup.

Endpoint holding affects picture selection only. Source-coordinate boundaries
still map to their exact position; an inverse query outside the beat fails.
The policy does not clamp or rebind marks into a held tail.

## Editing and persistence

`set_source_video_mapping` uses the normal revision-checked command and reversible
transaction path. `edit_occurrence` can apply it to one repeated play after
isolating that occurrence. Non-stream Sources, including still images and blank
picture with audio, retain `fit_beat` and reject this command.

Example command body for the one-second selection above:

```json
{
  "command": "set_source_video_mapping",
  "node": "source",
  "mapping": {
    "type": "duration",
    "frames": {"numerator": "30000", "denominator": "1001"},
    "endpoints": "hold_adjacent"
  }
}
```

Use the [headless command envelope](HEADLESS.md) for preview and commit. Duration
must be positive and at most `i64::MAX` project frames. Missing mapping or
endpoint fields, unknown fields and unsupported policies fail parsing.

Database schema 12 replays old history through frozen core schemas. Every old
Source gains `video_mapping: fit_beat`, preserving prior timing. Database schema
11's explicit audio mappings remain intact. New picture fields and commands are
rejected inside old snapshots, commands and patches, even when a field is null.
Migration retains revision identities, undo/redo, abandoned branches and
operational original-media and generation records.

This provides authored timing and frame selection. It does not establish source
qualification, common A/V origin, import rounding policy, first-primary project
basis selection, native editorial playback or export. Those remain required.
