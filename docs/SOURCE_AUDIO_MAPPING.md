# Independent source audio timing

Every Source has an explicit `audio_mapping`, introduced in core schema 6.
[Picture mapping](SOURCE_VIDEO_MAPPING.md) is independent. Audio maps its own selected span
over either that beat duration (`fit_beat`) or an independently authored exact
project-frame duration (`duration`). Core schema 8 adds an exact project-frame
start with `placement`; see [import timing](SOURCE_IMPORT_TIMING.md).
The signed 48 kHz `audio_offset` additionally translates
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
uses `fit_beat`; it cannot carry a separate audio placement. No mapping mode
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

Database schema 18 migrates schemas 1 through 17 directly to core schema 12 JSON.
Frozen source wires preserve pre-schema-11 audio mappings as `fit_beat`, including
offsets, historical snapshots and forward/inverse patches. They reject
`audio_mapping` fields and commands in history predating that vocabulary, even
null fields. Schema-11 explicit audio mappings remain unchanged. Migration keeps
original-media and generation records, revisions, undo/redo and branches; it
does not reinterpret old selections as natural-rate imports.

The measured import helper derives a common A/V origin while retaining all
available audio. [Source registration](SOURCE_REGISTRATION.md) retains selected-stream
receipts and indexes and applies atomic registration/insertion with
[presentation state](PRESENTATION_BASIS.md). Native source registration is connected;
[source-stage PCM](SOURCE_STAGE_AUDIO.md) now renders natural placements and
explicit tape-speed retimes. Complete voice processing and playback remain required.
See [source audio decoding](SOURCE_AUDIO.md).
