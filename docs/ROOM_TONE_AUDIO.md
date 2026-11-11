# Exact room-tone audio

`StageAudio` renders an authored `HoldAudio::RoomTone` from its explicit original
source range. It does not choose a range, classify it as non-speech, or replace
silence automatically. The selected source remains ordinary retained media with
revision-bound qualification and an explicit speaker interpretation. The current
engineering command path can insert that Hold as an ordinary subtree or change
an existing Hold with the atomic `SetHoldAudio` command. The native workflow
uses copied Original moments, exact sample fields, source audition and explicit
application to a selected pause: an ordinary Hold, one split Hold fragment or a
Repeat's default gap ([below](#repeat-gaps-and-split-fragments)). The
[room-tone board](design/boards/room-tone-board-v2.png) is its visual target.

```sh
cargo run --locked -p deadpan-cli -- inspect-audio /tmp/example.deadpan --samples 0 256 --time-mapped
```

The output remains `time_mapped_pcm_before_effects`. The source-only inspector
still rejects looping, reversal and effect tails; the time-mapped inspector
renders all three ([reversed pauses and tails](REVERSE_AND_TAILS.md)).
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
Tail and Reverse render through the same canonical Hold preparation as room
tone ([reversed pauses and tails](REVERSE_AND_TAILS.md)); there are no effect
sends.

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

## Native selection and audition

Copy a quiet Original range with `:source`, `v`, `h/l`, `y`. Return with
`:sequence`, select a pause (an ordinary Hold, a split fragment or a Repeat with
a gap), then use `:room-tone` or its inspector button. The sheet shows the source, rate, included In sample, excluded Out
sample and actual seconds. The service converts measured picture PTS directly
to source samples, rounding In upward and Out downward, intersecting available
audio and rejecting empty results. It never converts through project frames.
The complete source placement from a copied moment is not the selected range.

Space previews this source range; Shift+Space loops it with no implicit context.
Its separate zero-based clock leaves the retained picture, editor cursors and
beat selection intact. `AudioRange` supports qualified Original A/V audio without
weakening the catalog `Sound` descriptor's audio-only contract. Its temporary
Source view retains complete filter context and exact phase, with a separate
selection mask and measured rounded mix endpoint. Preparation and verified
source reads run off the UI thread. This preview hears the source; auditioning
the committed Hold hears its authored crossfaded loop.

Native sample fields retain text editing and IME. Tab moves through the sheet.
Changing fields invalidates audition and Apply until `Prepare range` or Enter
prepares them again. Enter on ready fields applies one edit; Escape cancels.
Reopening starts from the saved policy range. `Use copied Original range`
explicitly replaces the draft from the copy captured at command entry.
`:hold-silence` restores the selected Hold's silence policy in one undo step.
No preparation or audition writes history; no automatic speech check is claimed.

The UI captures the Hold, Sequence scope, project session, revision, source receipt and
sample span before parameter entry or asynchronous preparation. Captured absence
must remain an error rather than adopting a later selection. The sound catalog's
audio-only audition descriptor cannot represent the Original's selected audio
without the explicit `AudioRange` extension. Success and failure replies both
retain a request ticket and captured session/revision; older replies cannot
consume a newer pending preparation. Apply revalidates the captured target and
uses the same admitted sample span displayed and auditioned by the sheet.

## Verification and remaining work

Pure loop tests check exact phase, overlap weights, constant-level preservation,
fractional periods, tiny ranges, cancellation, input validation and replay.
Real PCM tests cover a 44.1 kHz mono source, repeats/overrides/gaps, nested retimes,
crop history and layout changes. The host test checks an explicit AAC range,
separate silent time, untouched following source audio, and unchanged history.
See [qualification](qualification/room-tone-audio-2026-09-21.md).

The [native workflow qualification](qualification/native-room-tone-2026-09-27.md)
records source-range PCM, transaction and stale-reply tests, keyboard/IME replay,
the design comparison, and the limits of the offscreen evidence.

The sheet shows the prepared range's measured stereo waveform (since
2026-11-07): the playback engine measures the same one-Source view the source
audition plays (`Engine::request_range_waveform`, on the idle analysis lane
shared with Edit waveforms) and the sheet draws its peaks with the preview
position, so quiet words show before Apply. Editing a sample field or closing
the sheet cancels the measurement. Evidence:
`a_source_range_waveform_measures_its_audition_view` and the `room-tone`
replay frame "Room-tone range waveform".

Listening across a representative ambience corpus remains required, as does
room tone for a gap or fragment inside one chosen Repeat play (only ordinary
Holds there). Export of room tone is
verified by the `nothing-happens` fixture
([preview-export-2026-10-04](qualification/preview-export-2026-10-04.md)), and
gain envelopes and tails exist ([AUDIO_GAIN.md](AUDIO_GAIN.md),
[REVERSE_AND_TAILS.md](REVERSE_AND_TAILS.md)); the full voice graph remains
open. No requirement or release gate is complete because a setter exists.

## Repeat gaps and split fragments

A pause's sound has three authored shapes, resolved from the selected direct
child by `ProjectDocument::pause_sound_target` for the native sheet, the
service, the semantic planner and headless callers alike:

- An ordinary Hold: `SetHoldAudio` on it.
- A Split fragment: transparent Partitions over that fragment's own copy of
  the Hold. `SetHoldAudio` changes only that Hold; sibling fragments keep their
  sound. The Partition selects the visible window, so the loop keeps the full
  Hold's phase from its own origin.
- A Repeat with a positive default gap: core `SetRepeatGapAudio { node, audio }`
  changes the gap recipe's audio, heard in every gap without an explicit branch.
  Gap duration and picture, plays, explicit branches (ordinary owned Holds,
  edited as pauses inside the Repeat) and every retained gap and suffix clock
  stay unchanged; each gap still loops from its own origin. Changing away from
  Silence retires only that Repeat's default-gap sound permissions in the same
  reversible patch. Store admission checks the source exactly as for a Hold.

The inspector shows the same `Room tone…` (`:room-tone`) and `Use silence`
(`:hold-silence`) rows under PAUSE SOUND for a fragment and GAP SOUND for a
Repeat. The sheet's heading and Apply note name what changes ("Room tone for
Repeat gaps", "Room tone for pause fragment"). A Repeat without a gap refuses
with the `:repeat N gap=` command that adds one. Inside one chosen Repeat play
only ordinary Holds are targets; a fragment or gap there refuses readably.

Evidence: core `pause_sound` tests (gap audio changes, durations, bindings,
branches and permissions; refusals; fragment isolation; exact Undo), the
service test `native_room_tone_applies_to_a_repeat_default_gap_and_one_split_fragment`,
the `room-tone-gaps` replay (real keys open the sheet on a Repeat and on a split
pause fragment, apply, undo and redo) and the `repeat-gap-room-tone`
preview/export fixture, whose two default gaps each play the looped click of
the copied moment.

## Recording

While a macro records, the sheet's Apply closes it and commits the exact
prepared source span as one semantic `SetHoldAudio { audio: RoomTone }`, and
`:hold-silence` records `SetHoldAudio { audio: Silence }`; a run applies them to
its own selected pause, where store admission rechecks the span against the
qualified Original. Other pause sounds keep their own instructions. The
`room-tone` replay records both and runs them.

## Custom pause sounds

Specification §10.4's fourth policy, `custom`, is `HoldAudio::Custom {
source, looped, fade_in_ms, fade_out_ms }`: a sound the user chooses for the
pause. The source is any qualified asset's exact audio span, a catalog sound
(its complete measured audio) or an Original range, admitted by the store
exactly like room tone (receipt, Original binding, contiguous measured samples,
endpoints on original sample boundaries), for `SetHoldAudio`, occurrence and
scoped edits and for `InsertTime`. It plays at its natural rate from the
Hold's start:

- Once (`looped` false): the first `min(L, H)` prepared samples, where `L` is
  the source's `ceil` 48 kHz length and `H` the Hold's; digital silence
  follows.
- Looped: the [room-tone loop](#loop-construction) of the same span for the
  whole Hold, with its crossfade and exact phase.
- Fades: `F = min(ms × 48, heard)` samples. Heard sample `n` is multiplied by
  `n / F_in` at the start and by `(heard - 1 - n) / F_out` at the end (both
  in `f64`), so the first and last heard samples are exact zero; overlapping
  fades multiply. Zero means no fade. The shared 2 ms Hold edge fades still
  apply afterwards, as for any Hold sound.

Validation bounds the span to one preparation block (1,048,576 samples at
48 kHz, about 21.8 s) and each fade to 10 s. Core omits defaults on the wire
(`looped` false, zero fades). Frozen audio context schema 14 carries
`custom` (with its source as a Hold input); schema 13 and older reject it.
`StageAudio` prepares it as one cached Hold block like room tone and reverse
(`deadpan_audio::custom_sound`, engine `deadpan-custom-pause-sound-linear-fades-v1`),
so a read starting mid-pause hears the same samples, and it composes with
Retimes, Repeats, gaps and fragments exactly as room tone does. The
source-stage inspector refuses it like room tone.

Native: the pause sound sheet (`:room-tone`, or **Custom sound…** in the
inspector's PAUSE SOUND section) has a Room tone / Custom sound choice. Custom
offers **Use copied Original range** and one **Use sound: <label>** button per
audio-only catalog sound (each prepares its complete measured audio, which
the In/Out sample fields can then narrow), **Loop until the pause ends**, and
**Fade in (ms)** / **Fade out (ms)** fields. Choosing the kind, loop or fades
keeps the prepared range; Apply (Enter) authors one undoable `SetHoldAudio`
(recorded in macros as the same instruction). Reopening starts from the saved
sound and options. The inspector shows `Custom sound · once|loop` with the
fades. Headless: `set_hold_audio`, `set_repeat_gap_audio` and the semantic
`set_hold_audio` instruction take `{"type":"custom","source":…,"looped":true,
"fade_in_ms":10,"fade_out_ms":10}`.

Evidence: core `a_custom_sound_round_trips_with_undo_and_its_bounds_validate`,
`a_custom_sound_span_is_bounded_by_the_one_block_dsp_limit`,
`custom_pause_sounds_keep_their_source_input_and_require_context_schema_14`;
store `pauses_inserted_with_reversed_audio_need_admitted_source_samples`
(an unqualified custom source is refused, an admitted one previews);
`StageAudio` decoded PCM in
`a_custom_sound_plays_once_with_exact_fades_then_silence_for_the_whole_pause`
(four fade/length cases, exact pause length, read in several piece sizes)
and `a_looped_custom_sound_is_the_room_tone_loop_with_fades_over_the_whole_pause`;
the `custom-pause-sound` replay (Tab and Space choose Custom, a catalog
sound and Loop, typed fades, Enter applies, reopen, Undo); release exports
`custom-pause-sound` (the click once, 2,000 samples into the pause, then
silence) and `custom-pause-loop` (a 40,000-sample range looped: the click at
29,829 and again 39,904 samples later, both audio windows at offset zero; a
short, periodic loop would make the offset unobservable). Listening is an owner check.
