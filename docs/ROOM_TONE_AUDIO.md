# Exact room-tone audio

`StageAudio` renders an authored `HoldAudio::RoomTone` from its explicit original
source range. It does not choose a range, classify it as non-speech, or replace
silence automatically. The selected source remains ordinary retained media with
revision-bound qualification and an explicit speaker interpretation. The current
engineering command path can insert that Hold as an ordinary subtree or change
an existing Hold with the atomic `SetHoldAudio` command. Native range selection,
source audition, and an audio-policy inspector remain open; the
[room-tone board](design/boards/room-tone-board-v2.png) is their visual target.

```sh
cargo run --locked -p deadpan-cli -- inspect-audio /tmp/example.deadpan --samples 0 256 --time-mapped
```

The output remains `time_mapped_pcm_before_effects`. The source-only inspector
still rejects looping, and effect tails remain unsupported by both inspectors.
Room tone and digital silence retain distinct authored and rendered behavior.

## Authored policy changes

Core 32/database 38 add `SetHoldAudio { node, audio }` and the corresponding
occurrence edit. Change only the Hold's audio policy and exact source range.
Its duration, picture/provider, captured framing, marks and retained sample
clocks stay intact. Changing raw audio invalidates its copy lineage and affected
processing ancestors through the existing reconciliation; it does not reset
their sampling clocks. A concrete occurrence edit isolates only the selected
play before applying the same setter.

Changing Silence to RoomTone or Tail removes that Hold's now-obsolete sound
allowances in the same reversible patch. Other Holds, plays and gaps keep their
permissions. Explicitly choosing Silence later does not recreate removed
permissions; Undo restores the exact prior policy and permissions together.
Tail remains authorable vocabulary but the renderer still rejects unsupported
tail processing. This command does not implement effects or sends.

The store admits a new RoomTone/Tail choice only from the expected revision's
qualified asset. It checks the immutable receipt, Original ownership binding,
measured contiguous audio and exact original sample endpoints. The generic
command API, dry run and chronological history replay use this validation.
This is stored source admission, not fresh byte verification; playback opens a
verified snapshot. Silence needs no new media admission. Unchanged legacy Hold
recipes remain valid on unrelated edits, while an explicit new source-policy
edit cannot reuse an unqualified legacy asset. Existing generation relevance
requirements remain unchanged.

Database-37 migration uses a closed core-31 grammar that preserves allowances
and rejects the new direct/occurrence setter. All snapshots and both patch
directions are compared before promotion, including abandoned history and redo.
See the [headless grammar](HEADLESS.md#hold-audio-policy) and
[authoring qualification](qualification/hold-audio-2026-09-27.md).

## Loop construction

First prepare the exact selected original interval on a 48 kHz stereo grid using
the qualified resampler and matrix. Original selection endpoints must be on
original sample boundaries. Let `L` be its exact length in 48 kHz samples. Store
`ceil(L)` points without changing `L`. Crossfade length is `F=min(96,L/2)` samples,
normally 2 ms and shortened for small selections. The overlap period is `P=L-F`.
These values remain rational even when the source rate is 44.1 kHz.

For absolute integer Hold-local sample `n`, compute `k=floor(n/P)` and
`x=n-k*P` exactly. The first pass uses the selected head at `x`. Later passes
with `x<F` combine the preceding tail at `P+x` with the new head at `x`, using
weights `1-x/F` and `x/F`. Other points use the head at `x`. Fractional positions
reconstruct through the qualified sinc sampler using the selected source's full
retained context. The complementary linear weights do not normalize source
loudness, add makeup gain, or clip. Engine identity:
`deadpan-room-tone-exact-linear-overlap-v1`.

Every point derives from the absolute Hold origin. No rounded loop durations are
accumulated, and empty cycles between integer samples are skipped without being
enumerated. Integer modular multiplication stays exact even when its direct
intermediate would overflow. Only normalized weights and fractional filter phase
convert to floating point.

## Duration, retimes and cache

The full Hold's canonical output stores `ceil(C*duration)` points, where
`C=48000*fps_den/fps_num`. Final project allocation still uses the exact absolute
round-even frame boundaries. Loop overlaps never add or remove authored time.
Each new Hold or repeat gap starts from its own local origin. Plan spans retain
the full intrinsic duration, including when a query or ancestor crops the Hold.

FollowSpeed reconstructs this prepared signal; Preserve consumes it with its
continuous canonical history. Mixed stage order stays explicit. Outer crops and
random queries keep the full intrinsic loop phase and filter context. Silent
Holds continue to suppress incoming energy after time mapping; room-tone Holds
do not acquire silence suppression merely because they are Holds.

Room-tone and Preserve entries share one bounded cache, residency accounting,
preparation quota and cooperative deadline. Keys distinguish node occurrences,
preceding repeat-play identities for gaps, selected sources, and intrinsic
durations. Transitive source/index/layout fingerprints are revalidated on hits.
The cache is private to one immutable plan and one executable's engine versions.

The current limits admit up to 1,048,576 prepared source frames and 8,388,608
Hold output frames, with the shared depth, memory and per-read budgets in
[stage preparation](AUDIO_STAGE_PREPARATION.md). This prepares the complete Hold
on a worker. It is not a callback algorithm or a long-input cache scheduler.
Failed preparation publishes no partial PCM or cache entry.

## Verification and remaining work

The next native workflow must retain one resolved sample span for its display,
source audition and Apply request. A copied Original moment is a picture-ordinal
selection, not yet a room-tone source: its derived audio placement retains the
complete measured source with a separate fractional selection. Never discard
that selection and loop the complete source accidentally. Resolve sample
endpoints directly against measured source time, display any boundary snapping,
and reject an empty result. Source preview needs zero implicit audition context;
the committed pause preview must exercise the actual room-tone loop.

Capture the Hold, Sequence scope, project session, revision, source receipt and
sample span before parameter entry or asynchronous preparation. Captured absence
must remain an error rather than adopting a later selection. The sound catalog's
audio-only audition descriptor cannot represent the Original's selected audio
without an explicit extension. Preserve the edit cursor, picture and selected
beat while auditioning source material. Apply remains an explicit undoable edit,
and native text fields retain macOS composition and key ownership.

Pure loop tests check exact phase, overlap weights, constant-level preservation,
fractional periods, tiny ranges, cancellation, input validation and replay.
Real PCM tests cover a 44.1 kHz mono source, repeats/overrides/gaps, nested retimes,
crop history and layout changes. The host test checks an explicit AAC range,
separate silent time, untouched following source audio, and unchanged history.
See [qualification](qualification/room-tone-audio-2026-09-21.md).

Listening across a representative ambience corpus, native policy/range editing
and dedicated source audition remain required. Existing Sequence audition can
render an authored room-tone Hold, but does not implement the range-selection
workflow. Effect tails, gain envelopes, the full voice graph and export remain
unfinished. No requirement or release gate is complete because a setter exists.
