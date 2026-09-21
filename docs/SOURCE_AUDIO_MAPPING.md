# Independent source audio timing

Core schema 6 gives every Source an explicit `audio_mapping`. Picture still maps
its selected span across `SourceNode.duration`. Audio maps its own selected span
over either that beat duration (`fit_beat`) or an independently authored exact
project-frame duration (`duration`). The signed 48 kHz `audio_offset` translates
the audio start. It does not change the audio rate or erase original timestamps.

`SourceAudioMapping::natural_rate(span, frame_rate)` computes the destination
duration from original timestamp units and rational project frame rate, without
rounding. For a one-second audio selection beneath a two-second picture at 30
fps, the result is 30 frames. An offset of 24000 mix samples places that audio
at local frames `[15,45)`. At 30000/1001 fps its duration is exactly 30000/1001
frames. Selected endpoints are valid boundaries; queries beyond the Source's
host interval fail instead of clamping or stretching audio to fit.

Explicit durations must be positive and at most `i64::MAX` project frames. Their
JSON numerators and denominators are decimal strings. A Source without audio
uses `fit_beat`; it cannot carry a separate audio duration. Neither duration mode
establishes codec-delay, priming, A/V origin, or import readiness evidence.

The reversible `set_source_audio_mapping` command accepts a Source node, mapping
and offset. It preserves picture selection, node duration, source selections,
source-coordinate marks and local timeline marks. An occurrence edit uses the
same command after isolating the selected repeated play. Enclosing Repeat and
Retime mappings compose exactly; only final project-boundary output rounds.

Example command body for one second at 30000/1001 fps, delayed by ten frames:

```json
{
  "command": "set_source_audio_mapping",
  "node": "source",
  "mapping": {
    "type": "duration",
    "frames": {"numerator": "30000", "denominator": "1001"}
  },
  "offset": 16016
}
```

Use this inside the normal revision-checked [headless command](HEADLESS.md)
envelope. Preview, commit, undo and redo use the same store transaction path.
This is an authored mapping operation; no UI control or audio renderer is added.

Database schema 11 migrates every prior database directly to current core JSON.
Frozen source wires preserve each old mapping as `fit_beat`, including offsets,
historical snapshots and forward/inverse patches. They reject `audio_mapping`
fields and mapping commands in old history, even null fields. Migration keeps
original-media and generation records, revisions, undo/redo and branches; it
does not reinterpret old selections as natural-rate imports.

Qualified selected-stream receipts, durable source indexes, common A/V origin
selection, atomic authored registration/insertion, audio rendering and native
import remain required. See [source audio decoding](SOURCE_AUDIO.md).
